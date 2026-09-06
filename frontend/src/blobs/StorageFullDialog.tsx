import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';
import { release } from './released';
import { heldPins, type HeldPin } from './pins';
import type { StorageFull } from './queue';

/**
 * What a full device says.
 *
 * Not "storage error". It names the file that would not fit, lists what is being kept on
 * purpose with what each one costs, and lets the user release one. Nothing pinned is ever
 * thrown away to make room — that decision belongs to the person who pinned it.
 */
export function StorageFullDialog({ full, onClose }: { full: StorageFull; onClose: () => void }) {
  const { db, me, files, workspace, releasedSpace } = useWorkspace();
  const [held, setHeld] = useState<HeldPin[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void heldPins(db, me.id).then(setHeld);
  }, [db, me.id, busy]);

  /**
   * Releasing is about this device only: the set stays pinned for everybody else, and for this
   * user on their phone. Nothing is deleted here — the files stop being protected, the eviction
   * pass reclaims what it needs, and the download that failed is tried again on the next pass.
   */
  const letGo = async (pin: HeldPin): Promise<void> => {
    setBusy(true);

    try {
      release(workspace.id, pin.kind, pin.id);
      await files.evict();
      releasedSpace();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-30 flex items-center justify-center bg-slate-900/50 p-6">
      <div className="max-h-full w-[32rem] overflow-auto rounded bg-white p-4 shadow-xl dark:bg-slate-900">
        <h2 className="mb-1 text-lg font-semibold">This device is out of room</h2>
        <p className="mb-3 text-sm text-slate-600 dark:text-slate-400">
          “{full.songTitle}” needs {megabytes(full.size)} and there is nowhere to put it. Nothing
          you pinned has been thrown away — releasing one below frees its files on this device
          only, and the download is tried again on the next pass.
        </p>

        <ul className="mb-3 divide-y divide-slate-200 text-sm dark:divide-slate-800">
          {held.map((pin) => (
            <li key={`${pin.kind}-${pin.id}`} className="flex items-baseline gap-2 py-2">
              <span>{pin.name}</span>
              <span className="text-xs text-slate-500">{pin.kind === 'set' ? 'set' : 'song'} · {megabytes(pin.bytes)}</span>

              <span className="ml-auto flex items-center gap-2">
                {pin.reason === 'coming-up' && <span className="text-xs text-slate-500">kept because it is coming up</span>}
                <button className="underline" disabled={busy} onClick={() => void letGo(pin)}>
                  Release
                </button>
              </span>
            </li>
          ))}

          {held.length === 0 && (
            <li className="py-2 text-slate-500">
              Nothing is pinned. The browser itself has no room left for this origin.
            </li>
          )}
        </ul>

        <div className="flex flex-wrap items-center gap-3 text-sm">
          <button
            className="rounded border border-slate-300 px-3 py-1 dark:border-slate-700"
            disabled={busy}
            onClick={() => void files.evict(0).then(() => releasedSpace())}
          >
            Clear files that were only opened
          </button>
          <Link className="underline" to="/settings/storage" onClick={onClose}>Offline storage</Link>
          <button className="ml-auto underline" onClick={onClose}>Not now</button>
        </div>
      </div>
    </div>
  );
}

function megabytes(bytes: number): string {
  return `${Math.max(1, Math.round(bytes / 1024 / 1024))} MB`;
}
