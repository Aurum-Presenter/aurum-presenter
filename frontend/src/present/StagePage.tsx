import { useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { parseKey } from '../chart/notes';
import { nextSlide, stageSlide, type SessionMessage, type SessionState } from './session';
import { StageSlide } from './SlideView';
import { OutputTransport } from './transport';
import { useWakeLock } from '../pwa/wakeLock';

/**
 * The stage view.
 *
 * It is a subscriber to the same state as the audience, rendered by different rules: chords
 * stay, the next slide is visible, and blanking the audience never blanks the stage — the band
 * still needs the words while the congregation is looking at a logo.
 *
 * When the connection drops it keeps the last slide on screen with a stale badge. A stage view
 * that goes blank in front of a congregation is worse than one that is a few seconds behind.
 */

interface StagePrefs {
  fontVh: number;
  chords: boolean;
  preview: boolean;
  clock: boolean;
  brightness: number;
}

const DEFAULT_PREFS: StagePrefs = { fontVh: 4, chords: true, preview: true, clock: true, brightness: 1 };

export function StagePage({ external }: { external?: { state: SessionState | null; stale: boolean; send?: (message: SessionMessage) => void } }) {
  const [params] = useSearchParams();
  const sessionId = params.get('session') ?? '';

  const [local, setLocal] = useState<SessionState | null>(null);
  const [transport, setTransport] = useState<OutputTransport | null>(null);
  const [prefs, setPrefs] = useState<StagePrefs>(() => {
    try {
      return { ...DEFAULT_PREFS, ...(JSON.parse(localStorage.getItem('aurum.stage') ?? '{}') as Partial<StagePrefs>) };
    } catch {
      return DEFAULT_PREFS;
    }
  });
  const [settings, setSettings] = useState(false);
  const [now, setNow] = useState(Date.now());

  // A tablet on a music stand must stay lit through a long song.
  useWakeLock();

  // A paired device is handed its state from outside; a window on the control device subscribes
  // to the same BroadcastChannel as every other output.
  useEffect(() => {
    if (external !== undefined) {
      return;
    }

    const output = new OutputTransport(sessionId, 'stage', 'Stage window', setLocal);
    setTransport(output);

    return () => output.close();
  }, [sessionId, external]);

  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);

    return () => clearInterval(timer);
  }, []);

  const state = external?.state ?? local;
  const stale = external?.stale ?? false;

  const update = (changes: Partial<StagePrefs>): void => {
    const next = { ...prefs, ...changes };
    setPrefs(next);
    localStorage.setItem('aurum.stage', JSON.stringify(next));
  };

  const key = useMemo(() => {
    const preferred = localStorage.getItem('aurum.stage.key');

    return preferred === null ? null : parseKey(preferred);
  }, []);

  const current = state === null ? null : stageSlide(state);
  const next = state === null ? null : nextSlide(state);
  const elapsed = state === null ? 0 : Math.floor((now - Date.parse(state.started_at)) / 1000);

  return (
    <div
      className={`flex h-dvh w-dvw flex-col bg-black text-slate-100 ${stale ? 'ring-4 ring-amber-500' : ''}`}
      style={{ filter: `brightness(${prefs.brightness})` }}
    >
      <header className="flex items-center gap-3 border-b border-slate-800 px-3 py-2 text-sm">
        <span className="font-medium">{current?.songTitle ?? state?.set_snapshot.setName ?? 'Stage'}</span>
        {current?.label != null && <span className="opacity-70">{current.label}</span>}

        {stale && <span className="rounded-full bg-amber-500 px-2 text-xs text-black">reconnecting</span>}

        <span className="ml-auto flex items-center gap-3 opacity-70">
          {state !== null && <span>{state.index + 1} / {state.slides.length}</span>}
          {prefs.clock && <span>{format(elapsed)}</span>}
          <button className="underline" onClick={() => setSettings((value) => ! value)}>layout</button>
        </span>
      </header>

      {state?.stage_message != null && (
        <p className="bg-amber-500 px-3 py-2 text-lg font-medium text-black">{state.stage_message}</p>
      )}

      <main className="grid flex-1 gap-4 overflow-hidden p-4 md:grid-cols-[3fr_2fr]">
        <section className="overflow-auto">
          <StageSlide slide={current} targetKey={key} showChords={prefs.chords} scale={prefs.fontVh} />
        </section>

        {prefs.preview && (
          <section className="overflow-auto border-l border-slate-800 pl-4 opacity-60">
            <p className="mb-2 text-xs uppercase tracking-widest">Next</p>
            <StageSlide
              slide={next}
              targetKey={key}
              showChords={prefs.chords}
              scale={prefs.fontVh * 0.7}
              empty="End of the set."
            />
          </section>
        )}
      </main>

      <footer className="flex items-center gap-3 border-t border-slate-800 px-3 py-2 text-sm opacity-70">
        <span>{state?.set_snapshot.setName}</span>
        <span className="ml-auto">{next?.songTitle !== current?.songTitle && next != null ? `Next: ${next.songTitle}` : ''}</span>

        {(external?.send !== undefined || transport !== null) && (
          <span className="flex gap-2">
            <button
              className="rounded border border-slate-700 px-2"
              onClick={() => (external?.send !== undefined
                ? external.send({ type: 'advance', output_id: 'stage', delta: -1 })
                : transport?.requestAdvance(-1))}
            >
              ←
            </button>
            <button
              className="rounded border border-slate-700 px-2"
              onClick={() => (external?.send !== undefined
                ? external.send({ type: 'advance', output_id: 'stage', delta: 1 })
                : transport?.requestAdvance(1))}
            >
              →
            </button>
          </span>
        )}
      </footer>

      {settings && (
        <div className="absolute inset-x-0 bottom-0 border-t border-slate-700 bg-slate-900 p-4 text-sm">
          <div className="flex flex-wrap items-center gap-4">
            <label className="flex items-center gap-2">
              Size
              <input type="range" min={2} max={10} step={0.5} value={prefs.fontVh} onChange={(event) => update({ fontVh: Number(event.target.value) })} />
            </label>
            <label className="flex items-center gap-2">
              Brightness
              <input type="range" min={0.3} max={1} step={0.05} value={prefs.brightness} onChange={(event) => update({ brightness: Number(event.target.value) })} />
            </label>
            <label className="flex items-center gap-2">
              <input type="checkbox" checked={prefs.chords} onChange={(event) => update({ chords: event.target.checked })} />
              Chords
            </label>
            <label className="flex items-center gap-2">
              <input type="checkbox" checked={prefs.preview} onChange={(event) => update({ preview: event.target.checked })} />
              Next preview
            </label>
            <label className="flex items-center gap-2">
              <input type="checkbox" checked={prefs.clock} onChange={(event) => update({ clock: event.target.checked })} />
              Clock
            </label>
            <button className="ml-auto underline" onClick={() => setSettings(false)}>done</button>
          </div>
        </div>
      )}
    </div>
  );
}

function format(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}
