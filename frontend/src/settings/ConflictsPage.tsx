import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { api } from '../api/client';
import { useWorkspace } from '../app/workspace';
import type { SyncedTable } from '../db/schema';

/**
 * The conflict review.
 *
 * Last-writer-wins is how the sync resolves a field two people changed at once, but the value
 * that lost is never destroyed: the server keeps it, every device can see it, and it can be
 * written back as a new edit. That is the difference between a merge rule and data loss.
 */
interface ServerConflict {
  id: string;
  table_name: SyncedTable;
  record_id: string;
  field: string;
  losing_value: string | null;
  losing_user: string | null;
  at: string;
}

export function ConflictsPage() {
  const { workspace, engine, db, canEdit } = useWorkspace();

  const [conflicts, setConflicts] = useState<ServerConflict[] | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [current, setCurrent] = useState<Map<string, string | null>>(new Map());

  useEffect(() => {
    void api<{ conflicts: ServerConflict[] }>(`/workspaces/${workspace.id}/sync/conflicts`)
      .then(async (response) => {
        setConflicts(response.conflicts);

        // The value that won is whatever is in the local copy now, which is what the reviewer
        // is comparing against.
        const values = new Map<string, string | null>();

        for (const conflict of response.conflicts) {
          const row = await (db as unknown as Record<SyncedTable, { get: (id: string) => Promise<Record<string, unknown> | undefined> }>)[conflict.table_name]
            ?.get(conflict.record_id);

          values.set(conflict.id, row === undefined ? null : String(row[conflict.field] ?? ''));
        }

        setCurrent(values);
      })
      .catch(() => setProblem('The conflict list could not be fetched. It needs a connection.'));
  }, [workspace.id, db]);

  const restore = async (conflict: ServerConflict): Promise<void> => {
    await engine.record(conflict.table_name, conflict.record_id, 'upsert', {
      [conflict.field]: conflict.losing_value,
    });

    setConflicts((rows) => (rows ?? []).filter((row) => row.id !== conflict.id));
  };

  return (
    <div className="mx-auto max-w-3xl p-4">
      <Link className="text-sm underline" to="/library">← Library</Link>
      <h2 className="mb-1 mt-3 text-2xl font-semibold">Conflicts</h2>
      <p className="mb-4 text-sm text-slate-500">
        When two people change the same field before either has synced, the later edit wins and
        the earlier one is kept here. Nothing is deleted; the older value can be put back.
      </p>

      {problem !== null && <p className="text-sm text-amber-700 dark:text-amber-400">{problem}</p>}

      {conflicts === null ? (
        <p className="text-sm text-slate-500">Loading…</p>
      ) : conflicts.length === 0 ? (
        <p className="text-sm text-slate-500">Nothing has been overwritten.</p>
      ) : (
        <ul className="space-y-3">
          {conflicts.map((conflict) => (
            <li key={conflict.id} className="rounded border border-slate-200 p-3 text-sm dark:border-slate-800">
              <p className="font-medium">
                {conflict.table_name}.{conflict.field}
                <span className="ml-2 text-xs text-slate-500">{new Date(conflict.at).toLocaleString()}</span>
              </p>

              <div className="mt-2 grid gap-2 md:grid-cols-2">
                <Value label="Now" value={current.get(conflict.id) ?? '—'} />
                <Value label="Overwritten" value={conflict.losing_value ?? '—'} />
              </div>

              {canEdit && (
                <button className="mt-2 underline" onClick={() => void restore(conflict)}>
                  Put the overwritten value back
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Value({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <p className="text-xs uppercase tracking-widest text-slate-500">{label}</p>
      <pre className="max-h-40 overflow-auto whitespace-pre-wrap rounded bg-slate-50 p-2 text-xs dark:bg-slate-800">{value}</pre>
    </div>
  );
}
