import { useLiveQuery } from 'dexie-react-hooks';
import { Link } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';

/**
 * Trash. A deleted song is a tombstone, not a hole: it replicates to every device, and for
 * thirty days it can come back on any of them (business rule 3).
 */
const KEEP_DAYS = 30;

export function TrashPage() {
  const { db, library, canEdit } = useWorkspace();

  const deleted = useLiveQuery(
    () => db.songs.filter((song) => song.deleted_at !== null).toArray(),
    [db],
    [],
  );

  const rows = [...deleted].sort((a, b) => (b.deleted_at ?? '').localeCompare(a.deleted_at ?? ''));

  return (
    <div className="mx-auto max-w-3xl p-4">
      <Link className="text-sm underline" to="/library">← Library</Link>
      <h2 className="mb-1 mt-3 text-2xl font-semibold">Trash</h2>
      <p className="mb-4 text-sm text-slate-500">
        Deleted songs are kept for {KEEP_DAYS} days and can be restored on any device.
      </p>

      {rows.length === 0 ? (
        <p className="text-sm text-slate-500">Nothing has been deleted.</p>
      ) : (
        <ul className="divide-y divide-slate-200 dark:divide-slate-800">
          {rows.map((song) => (
            <li key={song.id} className="flex items-center gap-3 py-2">
              <span className="font-medium">{song.title}</span>
              <span className="text-xs text-slate-500">{daysLeft(song.deleted_at)}</span>
              {canEdit && (
                <button className="ml-auto text-sm underline" onClick={() => void library.restoreSong(song.id)}>
                  Restore
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function daysLeft(deletedAt: string | null): string {
  if (deletedAt === null) {
    return '';
  }

  const elapsed = (Date.now() - Date.parse(deletedAt)) / 86_400_000;
  const left = Math.max(0, Math.ceil(KEEP_DAYS - elapsed));

  return left === 0 ? 'due to be purged' : `${left} day${left === 1 ? '' : 's'} left`;
}
