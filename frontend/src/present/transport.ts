import { channelName, type OutputKind, type OutputStatus, type SessionMessage, type SessionState } from './session';

/**
 * Getting the state to every screen.
 *
 * Windows on the same device use a BroadcastChannel: no permission, no network, no server, and
 * nothing to fail on a venue's guest wifi. Devices on the same LAN use a WebRTC data channel,
 * set up through the signalling relay and then peer-to-peer.
 *
 * Both carry the same messages, so nothing above this file knows which one it is talking to.
 */

/** An output that has not acked within this long is shown as not responding. */
export const NOT_RESPONDING_MS = 2000;
const HEARTBEAT_MS = 1000;

type Send = (message: SessionMessage) => void;

/** The control surface's side: one writer, many outputs. */
export class ControlTransport {
  private readonly channel: BroadcastChannel;
  private readonly peers = new Map<string, Send>();
  private outputs = new Map<string, OutputStatus>();
  private state: SessionState | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor(
    sessionId: string,
    private readonly onOutputs: (outputs: OutputStatus[]) => void,
    private readonly onAdvance: (delta: number, outputId: string) => void,
  ) {
    this.channel = new BroadcastChannel(channelName(sessionId));
    this.channel.onmessage = (event: MessageEvent<SessionMessage>) => this.receive(event.data);

    this.timer = setInterval(() => this.sweep(), HEARTBEAT_MS);
  }

  /** Adds a paired device's data channel as another output of the same session. */
  attachPeer(outputId: string, send: Send): void {
    this.peers.set(outputId, send);

    if (this.state !== null) {
      send({ type: 'state', state: this.state });
    }
  }

  detachPeer(outputId: string): void {
    this.peers.delete(outputId);
    this.outputs.delete(outputId);
    this.publish();
  }

  broadcast(state: SessionState): void {
    this.state = state;
    const message: SessionMessage = { type: 'state', state };

    this.channel.postMessage(message);

    for (const send of this.peers.values()) {
      try {
        send(message);
      } catch {
        // A dead channel is not a reason to stop updating the others (acceptance criterion 6).
      }
    }
  }

  grantAdvance(outputId: string, allowed: boolean): void {
    for (const [id, output] of this.outputs) {
      // Only one device may hold the grant at a time (stage-view business rule 8).
      this.outputs.set(id, { ...output, can_advance: allowed && id === outputId });
    }

    this.publish();
  }

  close(): void {
    if (this.timer !== null) {
      clearInterval(this.timer);
    }

    this.channel.close();
    this.peers.clear();
  }

  receive(message: SessionMessage): void {
    switch (message.type) {
      case 'hello':
        this.outputs.set(message.output_id, {
          output_id: message.output_id,
          kind: message.kind,
          label: message.label,
          joined_at: new Date().toISOString(),
          last_ack_revision: -1,
          responding: true,
          can_advance: false,
        });

        if (this.state !== null) {
          this.broadcast(this.state);
        }

        this.publish();
        break;

      case 'ack': {
        const existing = this.outputs.get(message.output_id);

        this.outputs.set(message.output_id, {
          output_id: message.output_id,
          kind: message.kind,
          label: message.label,
          joined_at: existing?.joined_at ?? new Date().toISOString(),
          last_ack_revision: message.revision,
          responding: true,
          can_advance: existing?.can_advance ?? false,
        });

        this.seen.set(message.output_id, Date.now());
        this.publish();
        break;
      }

      case 'advance':
        if (this.outputs.get(message.output_id)?.can_advance === true) {
          this.onAdvance(message.delta, message.output_id);
        }
        break;

      case 'request-state':
        if (this.state !== null) {
          this.broadcast(this.state);
        }
        break;

      case 'bye':
        this.outputs.delete(message.output_id);
        this.publish();
        break;

      default:
        break;
    }
  }

  private readonly seen = new Map<string, number>();

  /** Anything that has not acked recently is marked, without touching the others. */
  private sweep(): void {
    let changed = false;

    for (const [id, output] of this.outputs) {
      const last = this.seen.get(id) ?? 0;
      const responding = Date.now() - last < NOT_RESPONDING_MS;

      if (responding !== output.responding) {
        this.outputs.set(id, { ...output, responding });
        changed = true;
      }
    }

    if (changed) {
      this.publish();
    }
  }

  private publish(): void {
    this.onOutputs([...this.outputs.values()]);
  }
}

/** An output's side: receive state, ack it, and say hello loudly enough to be counted. */
export class OutputTransport {
  private readonly channel: BroadcastChannel;
  private readonly outputId = crypto.randomUUID();
  private timer: ReturnType<typeof setInterval> | null = null;
  private revision = -1;

  constructor(
    sessionId: string,
    private readonly kind: OutputKind,
    private readonly label: string,
    private readonly onState: (state: SessionState) => void,
  ) {
    this.channel = new BroadcastChannel(channelName(sessionId));
    this.channel.onmessage = (event: MessageEvent<SessionMessage>) => this.receive(event.data);

    this.channel.postMessage({ type: 'hello', output_id: this.outputId, kind, label } satisfies SessionMessage);
    this.channel.postMessage({ type: 'request-state', output_id: this.outputId } satisfies SessionMessage);

    this.timer = setInterval(() => this.ack(), HEARTBEAT_MS);
  }

  receive(message: SessionMessage): void {
    if (message.type !== 'state') {
      return;
    }

    // Business rule 5 of the stage view: an older revision is a message that overtook a newer
    // one, and applying it would move the screen backwards.
    if (message.state.revision < this.revision) {
      return;
    }

    this.revision = message.state.revision;
    this.onState(message.state);
    this.ack();
  }

  /** Ask the control surface to send the current state again — after a failed render, say. */
  requestState(): void {
    this.revision = -1;
    this.channel.postMessage({ type: 'request-state', output_id: this.outputId } satisfies SessionMessage);
  }

  requestAdvance(delta: number): void {
    this.channel.postMessage({ type: 'advance', output_id: this.outputId, delta } satisfies SessionMessage);
  }

  close(): void {
    if (this.timer !== null) {
      clearInterval(this.timer);
    }

    this.channel.postMessage({ type: 'bye', output_id: this.outputId } satisfies SessionMessage);
    this.channel.close();
  }

  private ack(): void {
    this.channel.postMessage({
      type: 'ack',
      output_id: this.outputId,
      kind: this.kind,
      label: this.label,
      revision: this.revision,
    } satisfies SessionMessage);
  }
}
