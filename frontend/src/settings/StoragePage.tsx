import { useLiveQuery } from 'dexie-react-hooks';
import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';
import { BlobStore } from '../blobs/store';
import { computeWanted } from '../blobs/wanted';
import { keepAgain, released } from '../blobs/released';
import { isAutoPinned } from '../sets/repository';
import { readUserPrefs } from '../prefs/userPrefs';

/**
 * What this device is keeping offline, and how much room it has left.
 *
 * The honest version: which sets are pinned and why, how much the files come to, whether the
 * browser has agreed to keep them, and one button to fetch everything rather than waiting for
 * the pin policy to get round to it.
 */
export function StoragePage() {
  const { db, me, blobs, files, workspace } = useWorkspace();

  const [estimate, setEstimate] = useState<{ usage: number; quota: number } | null>(null);
  const [persisted, setPersisted] = useState<boolean | null>(null);
  const [offlineReady, setOfflineReady] = useState<boolean | null>(null);
  const [letGo, setLetGo] = useState(() => released(workspace.id));
  const [fetching, setFetching] = useState(false);

  const cached = useLiveQuery(() => db.blobs.toArray(), [db], []);
  const sets = useLiveQuery(() => db.sets.filter((row) => row.deleted_at === null).toArray(), [db], []);
  const sheets = useLiveQuery(() => db.sheets.filter((row) => row.deleted_at === null).toArray(), [db], []);
  const songs = useLiveQuery(() => db.songs.filter((row) => row.deleted_at === null).toArray(), [db], []);

  useEffect(() => {
    void BlobStore.estimate().then(setEstimate);
    void navigator.storage?.persisted?.().then(setPersisted).catch(() => setPersisted(null));
  }, [cached]);

  // A private window will not register a service worker, and the app has to say so here rather
  // than let somebody find out on a stage with no signal (PWA acceptance criterion 8).
  useEffect(() => {
    if (! ('serviceWorker' in navigator)) {
      setOfflineReady(false);
      return;
    }

    void navigator.serviceWorker.getRegistration()
      .then((registration) => setOfflineReady(registration !== undefined))
      .catch(() => setOfflineReady(false));
  }, []);

  const pinnedBytes = cached.filter((row) => row.pin_reason === 'pinned').reduce((total, row) => total + row.size, 0);
  const opportunisticBytes = cached.filter((row) => row.pin_reason === 'opportunistic').reduce((total, row) => total + row.size, 0);
  const uncached = sheets.filter((sheet) => sheet.sha256 !== null && ! cached.some((row) => row.sheet_id === sheet.id));
  const everythingBytes = uncached.reduce((total, sheet) => total + (sheet.size ?? 0), 0);

  const downloadEverything = async (): Promise<void> => {
    setFetching(true);

    try {
      // Every sheet in the workspace, not just the pinned ones — asked for deliberately, with
      // the size shown first.
      await blobs.run(new Set(sheets.filter((sheet) => sheet.sha256 !== null).map((sheet) => sheet.id)));
    } finally {
      setFetching(false);
    }
  };

  const refreshPins = async (): Promise<void> => {
    const part = (await readUserPrefs(db, me.id)).part;
    await BlobStore.requestPersistence().then(setPersisted);
    await blobs.run(await computeWanted(db, me.id, part, released(workspace.id)));
  };

  return (
    <div className="mx-auto max-w-3xl p-4">
      <Link className="text-sm underline" to="/library">← Library</Link>
      <h2 className="mb-1 mt-3 text-2xl font-semibold">Offline storage</h2>
      <p className="mb-4 text-sm text-slate-500">
        Songs, charts and sets are always kept on this device — they are text, and small. Sheet
        PDFs are kept when they are pinned or coming up.
      </p>

      <dl className="mb-6 space-y-1 text-sm">
        <Row label="Workspace" value={workspace.name} />
        <Row label="Pinned files" value={`${cached.filter((row) => row.pin_reason === 'pinned').length} · ${megabytes(pinnedBytes)}`} />
        <Row label="Opened recently" value={`${cached.filter((row) => row.pin_reason === 'opportunistic').length} · ${megabytes(opportunisticBytes)}`} />
        <Row label="Not downloaded" value={`${uncached.length} · ${megabytes(everythingBytes)}`} />
        {estimate !== null && (
          <Row label="Browser storage" value={`${megabytes(estimate.usage)} of ${megabytes(estimate.quota)} used`} />
        )}
        <Row
          label="Offline use"
          value={offlineReady === false
            ? 'unavailable in this window — a private window cannot keep the app itself offline'
            : offlineReady === true
              ? 'ready — this device can open the app with no network'
              : 'checking…'}
        />
        <Row
          label="Kept under pressure"
          value={persisted === true
            ? 'yes — the browser has agreed to keep this data'
            : 'not yet. On iOS an origin that is never opened can be cleared after a week.'}
        />
      </dl>

      <div className="mb-6 flex flex-wrap gap-3">
        <button className="rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900" onClick={() => void refreshPins()}>
          Download what is pinned
        </button>
        <button
          className="rounded border border-slate-300 px-4 py-2 text-sm dark:border-slate-700"
          disabled={fetching || uncached.length === 0}
          onClick={() => void downloadEverything()}
        >
          {fetching ? 'Downloading…' : `Download everything (${megabytes(everythingBytes)})`}
        </button>
        <button className="text-sm underline" onClick={() => void files.evict(0)}>
          Clear files that were only opened
        </button>
      </div>

      <h3 className="mb-2 font-semibold">Sets kept offline</h3>
      <ul className="space-y-1 text-sm">
        {sets.filter((set) => isAutoPinned(set) && ! letGo.sets.includes(set.id)).map((set) => (
          <li key={set.id} className="flex gap-2">
            <Link className="underline" to={`/sets/${set.id}`}>{set.name}</Link>
            <span className="text-slate-500">
              {set.pinned === 1 ? 'pinned' : `coming up — ${set.scheduled_for}`}
            </span>
          </li>
        ))}
        {sets.filter((set) => isAutoPinned(set) && ! letGo.sets.includes(set.id)).length === 0 && (
          <li className="text-slate-500">Nothing is pinned and no set is within the next fortnight.</li>
        )}
      </ul>

      {letGo.sets.length + letGo.songs.length > 0 && (
        <>
          <h3 className="mb-2 mt-6 font-semibold">Released on this device</h3>
          <p className="mb-2 text-sm text-slate-500">
            Still pinned for everybody else — this device was simply out of room.
          </p>
          <ul className="space-y-1 text-sm">
            {[
              ...letGo.sets.map((id) => ({ kind: 'set' as const, id, name: sets.find((set) => set.id === id)?.name ?? 'A set' })),
              ...letGo.songs.map((id) => ({ kind: 'song' as const, id, name: songs.find((song) => song.id === id)?.title ?? 'A song' })),
            ].map((item) => (
              <li key={`${item.kind}-${item.id}`} className="flex gap-2">
                <span>{item.name}</span>
                <span className="text-xs text-slate-500">{item.kind}</span>
                <button
                  className="underline"
                  onClick={() => {
                    keepAgain(workspace.id, item.kind, item.id);
                    setLetGo(released(workspace.id));
                    void refreshPins();
                  }}
                >
                  Keep it again
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex gap-2">
      <dt className="w-44 text-slate-500">{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function megabytes(bytes: number): string {
  if (bytes < 1024 * 1024) {
    return `${Math.round(bytes / 1024)} KB`;
  }

  return `${Math.round(bytes / 1024 / 1024 * 10) / 10} MB`;
}
