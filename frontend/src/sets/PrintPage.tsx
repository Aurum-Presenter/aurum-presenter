import { useLiveQuery } from 'dexie-react-hooks';
import { useEffect, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';
import { ChartView } from '../chart/ChartView';
import { formatKey, parseKey } from '../chart/notes';
import type { Sheet } from '../db/schema';
import { useDisplay } from '../prefs/display';
import { NO_USER_PREFS, readUserPrefs } from '../prefs/userPrefs';
import { renderPdfToImages } from '../sheets/render';
import { selectSheet } from '../sheets/selection';
import { useResolvedSet, type ResolvedItem } from './useResolvedSet';

/**
 * The printable pack: every item of the set, each chart at the key this member will actually
 * play it in, and the sheet pages for the songs that have one.
 *
 * It is a print stylesheet rather than a generated file, which is what makes it work offline —
 * nothing is fetched and no service renders it. What it cannot do offline is print a sheet the
 * device never downloaded, so it says which ones those are before it starts rather than after.
 */
export function PrintPage() {
  const { setId } = useParams();
  const { db, me } = useWorkspace();
  const { set, items } = useResolvedSet(setId);
  const [display] = useDisplay();

  const [useSheets, setUseSheets] = useState(true);
  const [started, setStarted] = useState(false);

  const part = useLiveQuery(async () => (await readUserPrefs(db, me.id)).part, [db, me.id], NO_USER_PREFS.part);
  const sheets = useLiveQuery(() => db.sheets.filter((row) => row.deleted_at === null).toArray(), [db], []);
  const cached = useLiveQuery(async () => new Set((await db.blobs.toArray()).map((blob) => blob.sheet_id)), [db], new Set<string>());

  useEffect(() => {
    document.title = set === null ? 'Set' : `${set.name} — Aurum`;
  }, [set]);

  if (set === null) {
    return <p className="p-6 text-sm text-slate-500">Loading…</p>;
  }

  const chosen = new Map<string, Sheet>();
  const missing: string[] = [];

  for (const resolved of items) {
    if (resolved.song === null) {
      continue;
    }

    const forSong = sheets.filter((sheet) => sheet.song_id === resolved.song!.id);
    const selection = selectSheet(forSong, resolved.key, part);

    if (selection === null) {
      continue;
    }

    if (cached.has(selection.sheet.id)) {
      chosen.set(resolved.item.id, selection.sheet);
    } else if (selection.sheet.sha256 !== null) {
      missing.push(resolved.title);
    }
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

      <div className="no-print mb-4">
        <div className="flex flex-wrap items-center gap-3">
          <Link className="text-sm underline" to={`/sets/${set.id}`}>← {set.name}</Link>

          <label className="flex items-center gap-1 text-sm text-slate-500">
            <input type="checkbox" checked={useSheets} onChange={(event) => setUseSheets(event.target.checked)} />
            Use sheet PDFs where there is one
          </label>

          <button
            className="rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900"
            onClick={() => { setStarted(true); setTimeout(() => window.print(), 400); }}
          >
            Print or save as PDF
          </button>
        </div>

        {useSheets && missing.length > 0 && (
          <p className="mt-2 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-900">
            {missing.length} sheet{missing.length === 1 ? '' : 's'} ({missing.join(', ')}) have not been
            downloaded to this device. Printing now uses the chart for those songs instead.
          </p>
        )}

        {! started && (
          <p className="mt-2 text-xs text-slate-500">
            Everything below is rendered here on the device. Nothing is fetched, so this works offline.
          </p>
        )}
      </div>

      <h1 className="mb-1 text-2xl font-semibold">{set.name}</h1>
      <p className="mb-6 text-sm text-slate-500">
        {[set.scheduled_for, set.venue].filter((value) => value !== null && value !== '').join(' · ')}
      </p>

      {items.map((resolved, index) => (
        <PrintItem
          key={resolved.item.id}
          index={index}
          resolved={resolved}
          sheet={useSheets ? chosen.get(resolved.item.id) ?? null : null}
          display={display}
        />
      ))}
    </div>
  );
}

function PrintItem(props: {
  index: number;
  resolved: ResolvedItem;
  sheet: Sheet | null;
  display: ReturnType<typeof useDisplay>[0];
}) {
  const { files } = useWorkspace();
  const { resolved } = props;
  const [pages, setPages] = useState<string[] | null>(null);
  const written = resolved.written ?? parseKey('C')!;

  useEffect(() => {
    let cancelled = false;

    if (props.sheet === null) {
      setPages(null);
      return;
    }

    void files.get(props.sheet.id)
      .then((file) => (file === null ? null : renderPdfToImages(file)))
      .then((images) => { if (! cancelled) setPages(images); })
      .catch(() => { if (! cancelled) setPages(null); });

    return () => { cancelled = true; };
  }, [files, props.sheet]);

  return (
    <section className="set-item mb-8">
      <h2 className="mb-1 text-lg font-semibold">
        <span className="mr-2 text-slate-400">{props.index + 1}</span>
        {resolved.title}
        {resolved.key !== null && <span className="ml-3 text-base font-normal text-slate-500">{formatKey(resolved.key)}</span>}
        {resolved.capo > 0 && <span className="ml-2 text-base font-normal text-slate-500">capo {resolved.capo}</span>}
      </h2>

      {resolved.item.note !== null && <p className="mb-2 text-sm italic">{resolved.item.note}</p>}
      {resolved.item.content !== null && <p className="whitespace-pre-wrap">{resolved.item.content}</p>}
      {resolved.missing && <p className="text-sm">This song is no longer in the library.</p>}

      {pages !== null ? (
        pages.map((source, page) => <img key={page} src={source} alt="" className="mb-2 w-full" />)
      ) : (
        resolved.arrangement !== null && resolved.arrangement.body.trim() !== '' && (
          <ChartView
            cacheKey={`print:${resolved.arrangement.id}:${resolved.arrangement.updated_at}`}
            body={resolved.arrangement.body}
            source={written}
            target={resolved.key ?? written}
            capo={resolved.capo}
            display={props.display}
          />
        )
      )}
    </section>
  );
}
