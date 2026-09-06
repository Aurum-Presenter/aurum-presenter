import { useLiveQuery } from 'dexie-react-hooks';
import { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';
import { AUTO_PIN_DAYS, isAutoPinned, Sets } from './repository';

/** The sets list: what is coming up, what has been, and what is kept on this device. */
export function SetsPage() {
  const { db, engine, canEdit } = useWorkspace();
  const navigate = useNavigate();
  const sets = new Sets(db, engine);

  const [name, setName] = useState('');
  const [date, setDate] = useState('');

  const rows = useLiveQuery(
    async () => (await db.sets.filter((row) => row.deleted_at === null).toArray())
      .sort((a, b) => (b.scheduled_for ?? '').localeCompare(a.scheduled_for ?? '') || a.name.localeCompare(b.name)),
    [db],
    [],
  );

  const create = async (event: React.FormEvent): Promise<void> => {
    event.preventDefault();

    if (name.trim() === '') {
      return;
    }

    const id = await sets.create({ name, scheduled_for: date === '' ? null : date });
    setName('');
    setDate('');
    navigate(`/sets/${id}`);
  };

  return (
    <div className="mx-auto max-w-3xl p-4">
      <h2 className="mb-3 text-2xl font-semibold">Sets</h2>

      {canEdit && (
        <form className="mb-5 flex flex-wrap gap-2" onSubmit={(event) => void create(event)}>
          <input
            className="min-w-48 flex-1 rounded border border-slate-300 px-3 py-2 dark:border-slate-700 dark:bg-slate-900"
            placeholder="New set — Sunday morning, the Anchor, …"
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
          <input
            className="rounded border border-slate-300 px-3 py-2 dark:border-slate-700 dark:bg-slate-900"
            type="date"
            value={date}
            onChange={(event) => setDate(event.target.value)}
          />
          <button className="rounded bg-slate-900 px-4 py-2 text-white dark:bg-slate-100 dark:text-slate-900">Create</button>
        </form>
      )}

      {name.trim() !== '' && date === '' && (
        <p className="mb-3 text-xs text-amber-700 dark:text-amber-400">
          Without a date this set is not kept offline automatically — you can pin it instead.
        </p>
      )}

      {rows.length === 0 ? (
        <p className="rounded border border-dashed border-slate-300 py-10 text-center text-sm text-slate-500 dark:border-slate-700">
          No sets yet. Create one, or duplicate a past set once you have one.
        </p>
      ) : (
        <ul className="divide-y divide-slate-200 dark:divide-slate-800">
          {rows.map((set) => (
            <li key={set.id} className="flex items-baseline gap-3 py-2">
              <Link className="font-medium" to={`/sets/${set.id}`}>{set.name}</Link>
              {set.scheduled_for !== null && <span className="text-sm text-slate-500">{set.scheduled_for}</span>}
              {set.venue !== null && <span className="text-sm text-slate-500">{set.venue}</span>}

              {isAutoPinned(set) && (
                <span
                  className="rounded-full bg-sky-100 px-2 text-xs text-sky-900"
                  title={set.pinned === 1 ? 'Pinned for offline' : `Within ${AUTO_PIN_DAYS} days, so it is kept offline`}
                >
                  offline
                </span>
              )}

              {canEdit && (
                <span className="ml-auto flex gap-3 text-sm">
                  <button className="underline" onClick={() => void sets.duplicate(set.id).then((id) => id !== null && navigate(`/sets/${id}`))}>
                    Duplicate
                  </button>
                  <button className="underline text-red-700 dark:text-red-400" onClick={() => void sets.remove(set.id)}>
                    Delete
                  </button>
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
