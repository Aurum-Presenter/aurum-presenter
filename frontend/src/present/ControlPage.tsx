import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';
import { openAudience, type OpenedOutput } from './displays';
import { PairingHost, type Peer } from './pairing';
import {
  advance, audienceSlide, codeLife, CODE_TTL_MS, jump, nextSlide, pairingCode, setBlank,
  withMessage, withStageMessage, type BlankMode, type OutputStatus, type SessionState,
} from './session';
import { AudienceSlide, StageSlide } from './SlideView';
import { holdUpdates } from '../pwa/update';
import { useWakeLock } from '../pwa/wakeLock';
import { endSession, load, logAdvance, save } from './store';
import { ThemeDrawer } from './ThemeDrawer';
import { ControlTransport } from './transport';

/**
 * The control surface: one writer, many screens.
 *
 * Every change bumps the revision and goes out to every output at once. Outputs that stop
 * acking are marked and left alone — one dead tablet must not stall the projector.
 */
/** Per device, not per workspace: it is about this laptop's screens (presenter-output prefs). */
const HINT_DISMISSED = 'aurum.presenter.hint-dismissed';

export function ControlPage() {
  const { sessionId } = useParams();
  const { db, workspace } = useWorkspace();
  const navigate = useNavigate();

  const [state, setState] = useState<SessionState | null>(null);
  const [outputs, setOutputs] = useState<OutputStatus[]>([]);
  const [audience, setAudience] = useState<OpenedOutput | null>(null);
  const [code, setCode] = useState<{ value: string; expires: number } | null>(null);
  const [pairing, setPairing] = useState<'waiting' | 'connected' | 'failed' | null>(null);
  const [codeExpired, setCodeExpired] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const [message, setMessage] = useState('');
  const [stageMessage, setStageMessage] = useState('');
  const [themeOpen, setThemeOpen] = useState(false);
  const [ending, setEnding] = useState(false);
  const [hintDismissed, setHintDismissed] = useState(() => {
    try {
      return localStorage.getItem(HINT_DISMISSED) === '1';
    } catch {
      return false;
    }
  });

  // The operator's laptop must not dim between songs any more than a musician's phone does.
  useWakeLock();

  const transport = useRef<ControlTransport | null>(null);
  const host = useRef<PairingHost | null>(null);
  const current = useRef<SessionState | null>(null);
  current.current = state;

  /** One place where a new state is stored, mirrored and broadcast, so the three cannot drift. */
  const apply = useCallback(async (next: SessionState): Promise<void> => {
    setState(next);
    await save(db, next);
    transport.current?.broadcast(next);
  }, [db]);

  useEffect(() => {
    void load(db, sessionId!).then((loaded) => {
      setState(loaded);

      if (loaded !== null) {
        transport.current?.broadcast(loaded);
      }
    });
  }, [db, sessionId]);

  useEffect(() => {
    const control = new ControlTransport(
      sessionId!,
      setOutputs,
      (delta) => {
        const now = current.current;

        if (now !== null) {
          void apply(advance(now, delta));
        }
      },
    );

    transport.current = control;

    return () => {
      control.close();
      transport.current = null;
    };
  }, [sessionId, apply]);

  const move = useCallback(async (delta: number): Promise<void> => {
    const now = current.current;

    if (now === null) {
      return;
    }

    const next = advance(now, delta);
    await apply(next);
    await logAdvance(db, next);
  }, [apply, db]);

  useEffect(() => {
    const key = (event: KeyboardEvent): void => {
      if (document.activeElement instanceof HTMLInputElement) {
        return;
      }

      if (event.key === 'ArrowRight' || event.key === 'PageDown' || event.key === ' ') {
        event.preventDefault();
        void move(1);
      }

      if (event.key === 'ArrowLeft' || event.key === 'PageUp') {
        event.preventDefault();
        void move(-1);
      }

      if (event.key === 'b' || event.key === 'B') {
        const now = current.current;
        if (now !== null) void apply(setBlank(now, 'black'));
      }
    };

    window.addEventListener('keydown', key);

    return () => window.removeEventListener('keydown', key);
  }, [move, apply]);

  const openOutput = async (kind: 'audience' | 'stage'): Promise<void> => {
    const url = `${window.location.origin}/output/${kind}?session=${sessionId}&workspace=${workspace.id}`;

    if (kind === 'stage') {
      window.open(url, `aurum-stage`, 'width=1000,height=700');
      return;
    }

    setAudience(await openAudience(url));
  };

  /** The pairing code is the session's shared secret, and it expires in half an hour. */
  const startPairing = (): void => {
    const value = pairingCode();
    setCode({ value, expires: Date.now() + CODE_TTL_MS });
    setPairing('waiting');
    setCodeExpired(false);
    setNow(Date.now());

    host.current?.close();
    host.current = new PairingHost(
      value,
      workspace.id,
      (peer: Peer) => {
        transport.current?.attachPeer(peer.outputId, peer.send);
        transport.current?.receive({
          type: 'hello',
          output_id: peer.outputId,
          kind: 'paired-stage',
          label: 'Paired device',
        });
      },
      (outputId, incoming) => transport.current?.receive(
        // A state message can only travel outwards; anything else is stamped with the id this
        // control gave the peer, so a device cannot answer as another output.
        incoming.type === 'state' ? incoming : { ...incoming, output_id: outputId },
      ),
      setPairing,
    );

    host.current.listen();
  };

  /**
   * A code that has run out stops working (stage-view acceptance criterion 8). The control
   * surface stops listening rather than merely saying the code is old — a code written on a
   * whiteboard an hour ago must not still be a way into a session.
   */
  useEffect(() => {
    if (code === null || pairing === 'connected') {
      return;
    }

    const tick = setInterval(() => {
      const at = Date.now();

      if (codeLife(code.expires, at).expired) {
        host.current?.close();
        host.current = null;
        setPairing(null);
        setCode(null);
        setCodeExpired(true);
        return;
      }

      // Only when the displayed figure would change: the control surface must not re-render
      // behind an operator every few seconds for a countdown measured in minutes.
      setNow((last) => (codeLife(code.expires, at).minutesLeft === codeLife(code.expires, last).minutesLeft ? last : at));
    }, 5000);

    return () => clearInterval(tick);
  }, [code, pairing]);

  useEffect(() => () => host.current?.close(), []);

  // A reload mid-song is the worst thing the app could do, so a downloaded update waits for the
  // session to end (PWA business rule 3).
  useEffect(() => {
    holdUpdates(true);

    return () => holdUpdates(false);
  }, []);

  const finish = async (): Promise<void> => {
    if (state !== null) {
      await endSession(db, state);
      transport.current?.broadcast({ ...state, ended: true, revision: state.revision + 1 });
    }

    audience?.window?.close();
    audience?.connection?.terminate();
    navigate(state?.set_snapshot.setId === null ? '/sets' : `/sets/${state?.set_snapshot.setId}`);
  };

  const slides = state?.slides ?? [];
  const grouped = useMemo(() => groupByItem(slides), [slides]);

  if (state === null) {
    return (
      <div className="p-6 text-sm">
        <p className="text-slate-500">That session is not running on this device.</p>
        <Link className="underline" to="/sets">Back to sets</Link>
      </div>
    );
  }

  return (
    <div className="grid h-[calc(100dvh-3.5rem)] grid-cols-1 gap-4 p-4 lg:grid-cols-[2fr_1fr]">
      <div className="flex min-h-0 flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span className="font-medium">{state.set_snapshot.setName}</span>
          <span className="text-slate-500">{state.index + 1} / {slides.length}</span>

          <span className="ml-auto flex flex-wrap gap-2">
            <button className="rounded border border-slate-300 px-3 py-1 dark:border-slate-700" onClick={() => void move(-1)}>← Previous</button>
            <button className="rounded bg-slate-900 px-3 py-1 text-white dark:bg-slate-100 dark:text-slate-900" onClick={() => void move(1)}>Next →</button>
            <Blank state={state} onBlank={(mode) => void apply(setBlank(state, mode))} />
            <button className="rounded border border-slate-300 px-3 py-1 dark:border-slate-700" onClick={() => setThemeOpen(true)}>Theme</button>
            <button className="rounded border border-red-300 px-3 py-1 text-red-700 dark:text-red-400" onClick={() => setEnding(true)}>End</button>
          </span>
        </div>

        <div className="grid min-h-0 flex-1 grid-cols-2 gap-3">
          <figure className="flex min-h-0 flex-col">
            <figcaption className="mb-1 text-xs uppercase tracking-widest text-slate-500">On the audience screen</figcaption>
            <div
              className="aspect-video overflow-hidden rounded border border-slate-200 dark:border-slate-800"
              style={{ background: state.theme.background_value, color: state.theme.text_color }}
            >
              <AudienceSlide slide={audienceSlide(state)} theme={state.theme} workspaceId={state.workspace_id} />
            </div>
          </figure>

          <figure className="flex min-h-0 flex-col">
            <figcaption className="mb-1 text-xs uppercase tracking-widest text-slate-500">Next</figcaption>
            <div className="aspect-video overflow-hidden rounded border border-slate-200 bg-black p-3 text-slate-100 dark:border-slate-800">
              <StageSlide slide={nextSlide(state)} targetKey={null} showChords scale={2} empty="End of the set." />
            </div>
          </figure>
        </div>

        <div className="flex flex-wrap gap-2 text-sm">
          <input
            className="min-w-40 flex-1 rounded border border-slate-300 px-3 py-2 dark:border-slate-700 dark:bg-slate-900"
            placeholder="Message on the audience screen…"
            value={message}
            onChange={(event) => setMessage(event.target.value)}
          />
          <button className="rounded border border-slate-300 px-3 dark:border-slate-700" onClick={() => void apply(withMessage(state, message))}>
            Show
          </button>
          <button className="rounded border border-slate-300 px-3 dark:border-slate-700" onClick={() => { setMessage(''); void apply(withMessage(state, null)); }}>
            Clear
          </button>
        </div>

        <div className="flex flex-wrap gap-2 text-sm">
          <input
            className="min-w-40 flex-1 rounded border border-slate-300 px-3 py-2 dark:border-slate-700 dark:bg-slate-900"
            placeholder="Message to the stage only — two more times…"
            value={stageMessage}
            onChange={(event) => setStageMessage(event.target.value)}
          />
          <button className="rounded border border-slate-300 px-3 dark:border-slate-700" onClick={() => void apply(withStageMessage(state, stageMessage))}>
            Send
          </button>
          <button className="rounded border border-slate-300 px-3 dark:border-slate-700" onClick={() => { setStageMessage(''); void apply(withStageMessage(state, null)); }}>
            Clear
          </button>
        </div>
      </div>

      <aside className="flex min-h-0 flex-col gap-4 overflow-auto text-sm">
        <section>
          <h3 className="mb-1 font-semibold">Screens</h3>

          <div className="mb-2 flex flex-wrap gap-2">
            <button className="rounded border border-slate-300 px-3 py-1 dark:border-slate-700" onClick={() => void openOutput('audience')}>
              Audience window
            </button>
            <button className="rounded border border-slate-300 px-3 py-1 dark:border-slate-700" onClick={() => void openOutput('stage')}>
              Stage window
            </button>
            <button className="rounded border border-slate-300 px-3 py-1 dark:border-slate-700" onClick={startPairing}>
              Pair a device
            </button>
          </div>

          {audience?.hint != null && ! hintDismissed && (
            <p className="mb-2 text-xs text-slate-500">
              {audience.hint}{' '}
              <button
                className="underline"
                onClick={() => {
                  setHintDismissed(true);
                  try {
                    localStorage.setItem(HINT_DISMISSED, '1');
                  } catch {
                    // Without storage it comes back next time, which is the safe direction.
                  }
                }}
              >
                Got it
              </button>
            </p>
          )}

          {codeExpired && (
            <p className="mb-2 text-xs text-amber-700 dark:text-amber-400">
              That code has expired. Issue a new one to pair a device.
            </p>
          )}

          {code !== null && (
            <div className="mb-2 rounded border border-slate-200 p-2 dark:border-slate-800">
              <p className="text-xs text-slate-500">
                On the other device: open Aurum, choose Join session, and enter
              </p>
              <p className="my-1 font-mono text-2xl tracking-widest">{code.value}</p>
              <p className="text-xs text-slate-500">
                {pairing === 'failed'
                  ? 'The signalling relay could not be reached; a device on this network can still not be paired.'
                  : pairing === 'connected'
                    ? 'A device is connected.'
                    : `Waiting for a device. The code works for ${codeLife(code.expires, now).minutesLeft} more minutes.`}
              </p>
            </div>
          )}

          <ul className="space-y-1">
            {outputs.length === 0 && <li className="text-slate-500">No screens attached yet.</li>}
            {outputs.map((output) => (
              <li key={output.output_id} className="flex items-center gap-2">
                <span className={output.responding ? 'text-emerald-600' : 'text-amber-600'}>●</span>
                <span>{output.label}</span>
                <span className="text-xs text-slate-500">
                  {output.responding ? `revision ${output.last_ack_revision}` : 'not responding'}
                </span>

                {output.kind === 'paired-stage' && (
                  <label className="ml-auto flex items-center gap-1 text-xs">
                    <input
                      type="checkbox"
                      checked={output.can_advance}
                      onChange={(event) => transport.current?.grantAdvance(output.output_id, event.target.checked)}
                    />
                    may advance
                  </label>
                )}
              </li>
            ))}
          </ul>
        </section>

        <section className="min-h-0 flex-1">
          <h3 className="mb-1 font-semibold">Running order</h3>
          <ol className="space-y-1">
            {grouped.map((group) => (
              <li key={group.itemId}>
                <p className="mt-2 text-xs uppercase tracking-widest text-slate-500">{group.title}</p>
                <div className="flex flex-wrap gap-1">
                  {group.slides.map(({ slide, index }) => (
                    <button
                      key={slide.id}
                      className={`rounded border px-2 py-1 text-xs ${
                        index === state.index
                          ? 'border-sky-500 bg-sky-50 dark:bg-sky-950'
                          : 'border-slate-200 dark:border-slate-800'
                      }`}
                      onClick={() => void apply(jump(state, index))}
                    >
                      {slide.label ?? slide.kind}
                    </button>
                  ))}
                </div>
              </li>
            ))}
          </ol>
        </section>
      </aside>

      {themeOpen && (
        <ThemeDrawer
          theme={state.theme}
          onClose={() => setThemeOpen(false)}
          onChange={(theme) => void apply({ ...state, theme, revision: state.revision + 1 })}
        />
      )}

      {ending && (
        <div className="fixed inset-0 z-20 flex items-center justify-center bg-slate-900/50 p-6">
          <div className="w-80 rounded bg-white p-4 shadow-lg dark:bg-slate-900">
            <h2 className="mb-2 font-semibold">End this session?</h2>
            <p className="mb-4 text-sm text-slate-500">Every screen closes and the session is written to the log.</p>
            <div className="flex gap-2">
              <button className="rounded bg-slate-900 px-3 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900" onClick={() => void finish()}>
                End session
              </button>
              <button className="text-sm underline" onClick={() => setEnding(false)}>Keep going</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function Blank({ state, onBlank }: { state: SessionState; onBlank: (mode: BlankMode) => void }) {
  return (
    <span className="flex gap-1">
      {(['black', 'logo', 'freeze'] as BlankMode[]).map((mode) => (
        <button
          key={mode}
          className={`rounded border px-3 py-1 ${
            state.blank_mode === mode ? 'border-sky-500 bg-sky-50 dark:bg-sky-950' : 'border-slate-300 dark:border-slate-700'
          }`}
          onClick={() => onBlank(mode)}
        >
          {mode}
        </button>
      ))}
    </span>
  );
}

function groupByItem(slides: SessionState['slides']): { itemId: string; title: string; slides: { slide: SessionState['slides'][number]; index: number }[] }[] {
  const groups: { itemId: string; title: string; slides: { slide: SessionState['slides'][number]; index: number }[] }[] = [];

  slides.forEach((slide, index) => {
    const last = groups.at(-1);

    if (last !== undefined && last.itemId === slide.itemId) {
      last.slides.push({ slide, index });
      return;
    }

    groups.push({ itemId: slide.itemId, title: slide.songTitle, slides: [{ slide, index }] });
  });

  return groups;
}
