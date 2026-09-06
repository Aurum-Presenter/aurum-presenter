import { useEffect } from 'react';
import { Link, useParams } from 'react-router-dom';
import { ChartView } from '../chart/ChartView';
import { formatKey, parseKey } from '../chart/notes';
import { useDisplay } from '../prefs/display';
import { useResolvedSet } from './useResolvedSet';

/**
 * The printable pack: every item of the set, each chart at the key this member will actually
 * play it in.
 *
 * It is a print stylesheet rather than a generated file, which is what makes it work offline —
 * there is nothing to fetch and no service to render it. "Save as PDF" in the print dialog
 * produces the pack.
 */
export function PrintPage() {
  const { setId } = useParams();
  const { set, items } = useResolvedSet(setId);
  const [display] = useDisplay();

  useEffect(() => {
    document.title = set === null ? 'Set' : `${set.name} — Aurum`;
  }, [set]);

  if (set === null) {
    return <p className="p-6 text-sm text-slate-500">Loading…</p>;
  }

  return (
    <div className="mx-auto max-w-4xl p-6 print:max-w-none print:p-0">
      <style>{`
        @media print {
          .no-print { display: none !important; }
          .set-item { break-after: page; }
          .set-item:last-child { break-after: auto; }
          body { background: white; color: black; }
        }
      `}</style>

      <div className="no-print mb-4 flex items-center gap-3">
        <Link className="text-sm underline" to={`/sets/${set.id}`}>← {set.name}</Link>
        <button className="rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900" onClick={() => window.print()}>
          Print or save as PDF
        </button>
        <span className="text-xs text-slate-500">
          Each chart prints in the key this set gives you. Nothing is fetched, so this works offline.
        </span>
      </div>

      <h1 className="mb-1 text-2xl font-semibold">{set.name}</h1>
      <p className="mb-6 text-sm text-slate-500">
        {[set.scheduled_for, set.venue].filter((value) => value !== null && value !== '').join(' · ')}
      </p>

      {items.map((resolved, index) => {
        const written = resolved.written ?? parseKey('C')!;

        return (
          <section key={resolved.item.id} className="set-item mb-8">
            <h2 className="mb-1 text-lg font-semibold">
              <span className="mr-2 text-slate-400">{index + 1}</span>
              {resolved.title}
              {resolved.key !== null && <span className="ml-3 text-base font-normal text-slate-500">{formatKey(resolved.key)}</span>}
              {resolved.capo > 0 && <span className="ml-2 text-base font-normal text-slate-500">capo {resolved.capo}</span>}
            </h2>

            {resolved.item.note !== null && <p className="mb-2 text-sm italic">{resolved.item.note}</p>}
            {resolved.item.content !== null && <p className="whitespace-pre-wrap">{resolved.item.content}</p>}

            {resolved.missing && <p className="text-sm">This song is no longer in the library.</p>}

            {resolved.arrangement !== null && resolved.arrangement.body.trim() !== '' && (
              <ChartView
                cacheKey={`print:${resolved.arrangement.id}:${resolved.arrangement.updated_at}`}
                body={resolved.arrangement.body}
                source={written}
                target={resolved.key ?? written}
                capo={resolved.capo}
                display={display}
              />
            )}
          </section>
        );
      })}
    </div>
  );
}
