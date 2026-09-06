import { useEffect, useState } from 'react';
import { Link, NavLink, Outlet, useNavigate } from 'react-router-dom';
import { StorageFullDialog } from '../blobs/StorageFullDialog';
import { InstallBanner } from '../pwa/install';
import { SyncPanel } from '../settings/SyncPanel';
import { resumable } from '../present/store';
import type { SessionState } from '../present/session';
import { useWorkspace } from './workspace';

/**
 * The app frame: which workspace, what state the sync is in, and a way out.
 *
 * The sync chip is the only place the network is ever mentioned. Nothing else in the app waits
 * for it, so nothing else needs to talk about it.
 */
export function Shell({ onSignOut }: { onSignOut: () => void }) {
  const { me, workspace, db, online, pending, local, storageFull, setWorkspace } = useWorkspace();
  const navigate = useNavigate();
  const [resume, setResume] = useState<SessionState | null>(null);
  const [syncOpen, setSyncOpen] = useState(false);
  const [signingOut, setSigningOut] = useState(false);
  // Dismissal is per file: a different pinned file that will not fit is worth saying again.
  const [dismissedSheet, setDismissedSheet] = useState<string | null>(null);

  // A control window closed by accident is offered back for a minute (acceptance criterion 5).
  useEffect(() => {
    void resumable(db).then(setResume);
  }, [db]);

  return (
    <div className="min-h-dvh bg-white text-slate-900 dark:bg-slate-950 dark:text-slate-100">
      <header className="flex flex-wrap items-center gap-3 border-b border-slate-200 px-4 py-3 dark:border-slate-800">
        <Link to="/library" className="text-lg font-semibold">Aurum</Link>

        <nav className="flex gap-3 text-sm">
          <NavLink to="/library" className={({ isActive }) => (isActive ? 'font-medium' : 'text-slate-500')}>
            Library
          </NavLink>
          <NavLink to="/sets" className={({ isActive }) => (isActive ? 'font-medium' : 'text-slate-500')}>
            Sets
          </NavLink>
          <NavLink to="/join" className={({ isActive }) => (isActive ? 'font-medium' : 'text-slate-500')}>
            Join session
          </NavLink>
        </nav>

        <select
          className="rounded border border-slate-300 bg-transparent px-2 py-1 text-sm dark:border-slate-700"
          value={workspace.id}
          onChange={(event) => setWorkspace(event.target.value)}
        >
          {me.workspaces.map((option) => (
            <option key={option.id} value={option.id}>{option.name} · {option.role}</option>
          ))}
        </select>

        <button
          className={`ml-auto rounded-full px-3 py-1 text-xs font-medium ${
            online ? 'bg-emerald-100 text-emerald-900' : 'bg-amber-100 text-amber-900'
          }`}
          title="Nothing is blocked while offline; the outbox drains when a connection returns."
          onClick={() => setSyncOpen(true)}
        >
          {online ? 'synced' : 'offline'}{pending > 0 ? ` · ${pending} pending` : ''}
        </button>

        <Link to="/settings/account" className="text-sm underline">Account</Link>
        <button className="text-sm underline" onClick={() => setSigningOut(true)}>
          {local ? 'Sign in' : 'Sign out'}
        </button>
      </header>

      {resume !== null && (
        <div className="flex flex-wrap items-center gap-3 border-b border-amber-300 bg-amber-50 px-4 py-2 text-sm text-amber-900">
          <span>
            A session of “{resume.set_snapshot.setName}” was running, at slide {resume.index + 1}.
          </span>
          <button
            className="underline"
            onClick={() => { setResume(null); navigate(`/present/${resume.session_id}`); }}
          >
            Resume it
          </button>
          <button className="underline" onClick={() => setResume(null)}>Dismiss</button>
        </div>
      )}

      {local && (
        <div className="flex flex-wrap items-center gap-3 border-b border-slate-200 bg-slate-50 px-4 py-2 text-sm text-slate-700 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-300">
          <span>
            This device is working on its own. Everything is saved here; nothing is shared or
            backed up until you sign in.
          </span>
          <button className="underline" onClick={() => setSigningOut(true)}>Sign in and keep it</button>
        </div>
      )}

      <InstallBanner />

      {signingOut && (
        <SignOutDialog
          local={local}
          onCancel={() => setSigningOut(false)}
          onConfirm={async (removeContent) => {
            if (removeContent) {
              // "Also remove downloaded songs from this device" — the whole local database,
              // which is exactly what somebody handing a laptop back means by it.
              await db.delete();
            }

            onSignOut();
          }}
        />
      )}

      {syncOpen && <SyncPanel onClose={() => setSyncOpen(false)} />}

      {storageFull !== null && dismissedSheet !== storageFull.sheetId && (
        <StorageFullDialog full={storageFull} onClose={() => setDismissedSheet(storageFull.sheetId)} />
      )}

      <Outlet />
    </div>
  );
}

/**
 * Signing out is two decisions, not one: end the session, and decide whether the songs stay on
 * this device. A shared laptop wants them gone; a personal phone does not.
 */
function SignOutDialog({
  local,
  onCancel,
  onConfirm,
}: {
  local: boolean;
  onCancel: () => void;
  onConfirm: (removeContent: boolean) => Promise<void>;
}) {
  const [remove, setRemove] = useState(false);

  return (
    <div className="fixed inset-0 z-30 flex items-center justify-center bg-slate-900/50 p-6" onClick={onCancel}>
      <div className="w-96 rounded bg-white p-4 text-sm dark:bg-slate-900" onClick={(event) => event.stopPropagation()}>
        <h2 className="mb-2 text-lg font-semibold">{local ? 'Sign in' : 'Sign out'}</h2>

        <p className="mb-3 text-slate-500">
          {local
            ? 'Signing in keeps everything on this device and starts syncing it to your account.'
            : 'Your songs stay on this device unless you say otherwise.'}
        </p>

        {! local && (
          <label className="mb-4 flex items-center gap-2">
            <input type="checkbox" checked={remove} onChange={(event) => setRemove(event.target.checked)} />
            Also remove downloaded songs and sheets from this device
          </label>
        )}

        <div className="flex gap-3">
          <button
            className="rounded bg-slate-900 px-3 py-2 text-white dark:bg-slate-100 dark:text-slate-900"
            onClick={() => void onConfirm(remove)}
          >
            {local ? 'Go to sign in' : 'Sign out'}
          </button>
          <button className="underline" onClick={onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
