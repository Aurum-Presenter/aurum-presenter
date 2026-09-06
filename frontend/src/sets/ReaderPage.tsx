import { useEffect, useRef, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { ChartView } from '../chart/ChartView';
import { formatKey, parseKey } from '../chart/notes';
import { useDisplay } from '../prefs/display';
import { useWakeLock } from '../pwa/wakeLock';
import { useResolvedSet, type ResolvedItem } from './useResolvedSet';

/**
 * Reader mode: one item per screen, for a musician holding a phone on a mic stand.
 *
 * Arrow keys on a desktop, swipe on a phone, and a wake lock for as long as the set is open —
 * a screen that sleeps between verses is the single most annoying thing a stage app can do.
 */
export function ReaderPage() {
  const { setId, index } = useParams();
  const navigate = useNavigate();
  const { set, items } = useResolvedSet(setId);
  const [display] = useDisplay();
  const [jumping, setJumping] = useState(false);

  const position = Math.min(Math.max(Number(index ?? 0), 0), Math.max(items.length - 1, 0));
  const current = items[position];

  const go = useRef<(delta: number) => void>(() => undefined);
  go.current = (delta: number): void => {
    const next = position + delta;

    if (next >= 0 && next < items.length) {
      navigate(`/sets/${setId}/read/${next}`);
    }
  };

  useEffect(() => {
    const key = (event: KeyboardEvent): void => {
      if (event.key === 'ArrowRight' || event.key === 'PageDown' || event.key === ' ') {
        event.preventDefault();
        go.current(1);
      }

      if (event.key === 'ArrowLeft' || event.key === 'PageUp') {
        event.preventDefault();
        go.current(-1);
      }

      if (event.key === 'Escape') {
        navigate(`/sets/${setId}`);
      }
    };

    window.addEventListener('keydown', key);

    return () => window.removeEventListener('keydown', key);
  }, [navigate, setId]);

  useWakeLock();

  const touch = useRef<number | null>(null);

  if (set === null || current === undefined) {
    return <p className="p-6 text-sm text-slate-500">Loading…</p>;
  }

  return (
    <div
      className="mx-auto max-w-4xl p-4"
      onTouchStart={(event) => { touch.current = event.touches[0]!.clientX; }}
      onTouchEnd={(event) => {
        if (touch.current === null) return;
        const travelled = event.changedTouches[0]!.clientX - touch.current;
        if (Math.abs(travelled) > 60) go.current(travelled < 0 ? 1 : -1);
        touch.current = null;
      }}
    >
      <header className="mb-3 flex items-center gap-3 border-b border-slate-200 pb-2 dark:border-slate-800">
        <Link className="text-sm underline" to={`/sets/${set.id}`}>← {set.name}</Link>
        <button className="text-sm underline" onClick={() => setJumping(true)}>
          {position + 1} / {items.length}
        </button>

        <span className="ml-auto flex gap-2 text-sm">
          <button className="rounded border border-slate-300 px-3 dark:border-slate-700" onClick={() => go.current(-1)} disabled={position === 0}>←</button>
          <button className="rounded border border-slate-300 px-3 dark:border-slate-700" onClick={() => go.current(1)} disabled={position === items.length - 1}>→</button>
        </span>
      </header>

      <ReaderItem resolved={current} display={display} />

      {jumping && (
        <div className="fixed inset-0 z-20 flex items-end bg-slate-900/50" onClick={() => setJumping(false)}>
          <ul className="max-h-[70vh] w-full overflow-auto rounded-t bg-white p-2 dark:bg-slate-900" onClick={(event) => event.stopPropagation()}>
            {items.map((item, itemIndex) => (
              <li key={item.item.id}>
                <button
                  className={`flex w-full gap-2 px-2 py-2 text-left ${itemIndex === position ? 'font-semibold' : ''}`}
                  onClick={() => { navigate(`/sets/${setId}/read/${itemIndex}`); setJumping(false); }}
                >
                  <span className="w-6 text-slate-400">{itemIndex + 1}</span>
                  <span>{item.title}</span>
                  {item.key !== null && <span className="ml-auto text-slate-500">{formatKey(item.key)}</span>}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

export function ReaderItem({ resolved, display }: { resolved: ResolvedItem; display: ReturnType<typeof useDisplay>[0] }) {
  const written = resolved.written ?? parseKey('C')!;

  return (
    <article>
      <h2 className="text-xl font-semibold">
        {resolved.title}
        {resolved.key !== null && <span className="ml-3 text-base font-normal text-slate-500">{formatKey(resolved.key)}</span>}
        {resolved.capo > 0 && <span className="ml-2 text-base font-normal text-slate-500">capo {resolved.capo}</span>}
      </h2>

      {resolved.item.note !== null && (
        <p className="mb-2 rounded bg-amber-50 px-2 py-1 text-sm text-amber-900">{resolved.item.note}</p>
      )}

      {resolved.missing && (
        <p className="mb-2 text-sm text-amber-700 dark:text-amber-400">
          This song has been deleted from the library. The set keeps its title so the running
          order still reads.
        </p>
      )}

      {resolved.item.content !== null && (
        <p className="whitespace-pre-wrap py-4 text-lg">{resolved.item.content}</p>
      )}

      {resolved.arrangement !== null && resolved.arrangement.body.trim() !== '' && (
        <ChartView
          cacheKey={`${resolved.arrangement.id}:${resolved.arrangement.updated_at}`}
          body={resolved.arrangement.body}
          source={written}
          target={resolved.key ?? written}
          capo={resolved.capo}
          display={display}
        />
      )}

      {resolved.arrangement !== null && resolved.arrangement.body.trim() === '' && (
        <p className="py-4 text-sm text-slate-500">No chart for this song yet.</p>
      )}
    </article>
  );
}
