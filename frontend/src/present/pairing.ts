import { currentAccessToken } from '../api/client';
import type { SessionMessage } from './session';

/**
 * Pairing a stage device with the control device over the local network.
 *
 * The data channel is peer-to-peer and DTLS-encrypted; the only thing that touches the server is
 * the offer, the answer and the ICE candidates, through a relay that holds nothing. Session
 * state — which includes copyrighted lyrics — never leaves the LAN.
 *
 * The two LAN-local signalling paths the stage-view document prefers (mDNS, and a local HTTP
 * endpoint on the control device) are not reachable from a browser: a page cannot advertise a
 * service or listen on a port. So the relay, which that document makes the last resort, is the
 * path a browser can actually take, and the CR that added it says exactly why.
 */

type Signal =
  | { kind: 'offer'; sdp: string }
  | { kind: 'answer'; sdp: string }
  | { kind: 'ice'; candidate: RTCIceCandidateInit };

const CONFIG: RTCConfiguration = {
  // No STUN, no TURN: both peers are on the same LAN by design, and host candidates are enough.
  // Adding a public STUN server would leak the fact of a session to a third party for nothing.
  iceServers: [],
};

/**
 * The relay is its own process on its own port — it holds sockets open, which the API's request
 * lifecycle cannot — so it has its own base URL rather than being derived from the API's.
 */
const SIGNAL_URL = (import.meta.env.VITE_SIGNAL_URL as string | undefined) ?? 'ws://localhost:8081';

function socketUrl(code: string, workspaceId?: string): string {
  const token = currentAccessToken();
  const workspace = workspaceId === undefined ? '' : `&workspace=${encodeURIComponent(workspaceId)}`;

  return `${SIGNAL_URL}/api/v1/sessions/${code}/signal?token=${encodeURIComponent(token ?? '')}${workspace}`;
}

export interface Peer {
  outputId: string;
  send: (message: SessionMessage) => void;
  close: () => void;
}

/**
 * The control device's side: waits on the relay for a device that has been given the code,
 * answers its offer, and hands back a channel that behaves like any other output.
 */
export class PairingHost {
  private socket: WebSocket | null = null;
  private readonly peers = new Set<RTCPeerConnection>();

  constructor(
    private readonly code: string,
    private readonly workspaceId: string,
    private readonly onPeer: (peer: Peer) => void,
    private readonly onMessage: (message: SessionMessage) => void,
    private readonly onStatus: (status: 'waiting' | 'connected' | 'failed') => void,
  ) {}

  listen(): void {
    let socket: WebSocket;

    try {
      socket = new WebSocket(socketUrl(this.code, this.workspaceId));
    } catch {
      this.onStatus('failed');
      return;
    }

    this.socket = socket;
    socket.onopen = () => this.onStatus('waiting');
    socket.onerror = () => this.onStatus('failed');

    socket.onmessage = async (event: MessageEvent<string>) => {
      const signal = JSON.parse(event.data) as Signal;

      if (signal.kind !== 'offer') {
        return;
      }

      const connection = new RTCPeerConnection(CONFIG);
      this.peers.add(connection);

      connection.onicecandidate = (event) => {
        if (event.candidate !== null) {
          socket.send(JSON.stringify({ kind: 'ice', candidate: event.candidate.toJSON() } satisfies Signal));
        }
      };

      connection.ondatachannel = (event) => {
        const channel = event.channel;
        const outputId = crypto.randomUUID();

        channel.onmessage = (message: MessageEvent<string>) =>
          this.onMessage(JSON.parse(message.data) as SessionMessage);

        channel.onopen = () => {
          this.onStatus('connected');
          this.onPeer({
            outputId,
            send: (message) => channel.readyState === 'open' && channel.send(JSON.stringify(message)),
            close: () => connection.close(),
          });

          // The relay is a setup channel with a short life: once the data channel is open there
          // is nothing left for the server to carry.
          socket.close();
        };
      };

      await connection.setRemoteDescription({ type: 'offer', sdp: signal.sdp });
      const answer = await connection.createAnswer();
      await connection.setLocalDescription(answer);

      socket.send(JSON.stringify({ kind: 'answer', sdp: answer.sdp! } satisfies Signal));
    };
  }

  close(): void {
    this.socket?.close();

    for (const connection of this.peers) {
      connection.close();
    }
  }
}

/** The joining device's side: offers, waits for the answer, then talks peer-to-peer. */
export async function joinSession(
  code: string,
  onMessage: (message: SessionMessage) => void,
  onClose: () => void,
): Promise<(message: SessionMessage) => void> {
  const connection = new RTCPeerConnection(CONFIG);
  const channel = connection.createDataChannel('session', { ordered: true });

  channel.onmessage = (event: MessageEvent<string>) => onMessage(JSON.parse(event.data) as SessionMessage);
  channel.onclose = onClose;

  const socket = new WebSocket(socketUrl(code));

  await new Promise<void>((resolve, reject) => {
    socket.onopen = () => resolve();
    socket.onerror = () => reject(new Error('The session could not be reached. Are you on the same network and signed in?'));
    socket.onclose = (event) => {
      if (event.code === 4404) reject(new Error('That code is not a running session, or it has expired.'));
      if (event.code === 4401) reject(new Error('Sign in again to join this session.'));
      if (event.code === 4409) reject(new Error('Another device is already joining. Try again in a moment.'));
    };
  });

  connection.onicecandidate = (event) => {
    if (event.candidate !== null && socket.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify({ kind: 'ice', candidate: event.candidate.toJSON() } satisfies Signal));
    }
  };

  socket.onmessage = async (event: MessageEvent<string>) => {
    const signal = JSON.parse(event.data) as Signal;

    if (signal.kind === 'answer') {
      await connection.setRemoteDescription({ type: 'answer', sdp: signal.sdp });
    }

    if (signal.kind === 'ice') {
      await connection.addIceCandidate(signal.candidate).catch(() => undefined);
    }
  };

  const offer = await connection.createOffer();
  await connection.setLocalDescription(offer);
  socket.send(JSON.stringify({ kind: 'offer', sdp: offer.sdp! } satisfies Signal));

  await new Promise<void>((resolve, reject) => {
    channel.onopen = () => { socket.close(); resolve(); };
    setTimeout(() => reject(new Error('The session did not answer. It may have ended.')), 20_000);
  });

  return (message: SessionMessage) => {
    if (channel.readyState === 'open') {
      channel.send(JSON.stringify(message));
    }
  };
}
