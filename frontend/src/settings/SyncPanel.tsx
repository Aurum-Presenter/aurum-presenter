import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';
import type { OutboxOp } from '../db/schema';

/**
 * What the sync is doing, for the one moment a musician cares: when something has not gone
 * through.
 *
 * Nothing here blocks anything. The panel exists so that "pending" has an answer, and so that a
 * parked operation — one the server refused — can be retried or thrown away deliberately rather
 * than sitting in a queue forever.
 */
export function SyncPanel({ onClose }: { onClose: () => void }) {
  const { engine, db, online, syncNow } = useWorkspace();

  const [status, setStatus] = useState({ pending: 0, parked: 0, lastPull: null as string | null, lastPush: null as string | null });
  const [parked, setParked] = useState<OutboxOp[]>([]);
  const [uploads, setUploads] = useState(0);

  const refresh = async (): Promise<void> => {
    setStatus(await engine.status());
    setParked(await engine.parked());
    setUploads(await db.uploads.count());
  };

  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);

    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engine, db]);

  return (
    <div className="fixed inset-0 z-30 flex justify-end bg-slate-900/40" onClick={onClose}>
      <div className="h-full w-96 overflow-auto bg-white p-4 text-sm dark:bg-slate-900" onClick={(event) => event.stopPropagation()}>
        <h2 className="mb-3 text-lg font-semibold">Sync</h2>

        <dl className="mb-4 space-y-1">
          <Row label="Connection" value={online ? 'online' : 'offline — everything still works'} />
          <Row label="Waiting to send" value={`${status.pending} change${status.pending === 1 ? '' : 's'}`} />
          <Row label="Files waiting" value={`${uploads}`} />
          <Row label="Last received" value={ago(status.lastPull)} />
          <Row label="Last sent" value={ago(status.lastPush)} />
        </dl>

        <div className="mb-4 flex gap-3">
          <button className="rounded bg-slate-900 px-3 py-2 text-white dark:bg-slate-100 dark:text-slate-900" onClick={syncNow}>
            Sync now
          </button>
          <Link className="self-center underline" to="/settings/sync/conflicts" onClick={onClose}>Conflicts</Link>
          <Link className="self-center underline" to="/settings/storage" onClick={onClose}>Storage</Link>
        </div>

        {parked.length > 0 && (
          <section>
            <h3 className="mb-1 font-semibold text-amber-700 dark:text-amber-400">
              {parked.length} change{parked.length === 1 ? '' : 's'} the server refused
            </h3>
            <p className="mb-2 text-xs text-slate-500">
              These are kept, not lost. Fix what caused them — a permission, a record someone
              deleted — and retry, or discard the change if it is no longer wanted.
            </p>

            <ul className="space-y-2">
              {parked.map((op) => (
                <li key={op.seq} className="rounded border border-slate-200 p-2 dark:border-slate-800">
                  <p className="font-mono text-xs">{op.op} {op.table}</p>
                  <p className="text-xs text-slate-500">{op.last_error ?? 'refused'}</p>
                  <p className="mt-1 flex gap-3">
                    <button className="underline" onClick={() => void engine.retry(op.seq!).then(refresh)}>Retry</button>
                    <button className="underline text-red-700 dark:text-red-400" onClick={() => void engine.discard(op.seq!).then(refresh)}>
                      Discard
                    </button>
                  </p>
                </li>
              ))}
            </ul>
          </section>
        )}

        <button className="mt-4 underline" onClick={onClose}>Close</button>
      </div>
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex gap-2">
      <dt className="w-36 text-slate-500">{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function ago(at: string | null): string {
  if (at === null) {
    return 'never';
  }

  const seconds = Math.round((Date.now() - Date.parse(at)) / 1000);

  if (seconds < 60) {
    return 'just now';
  }

  if (seconds < 3600) {
    return `${Math.round(seconds / 60)} minutes ago`;
  }

  return `${Math.round(seconds / 3600)} hours ago`;
}
