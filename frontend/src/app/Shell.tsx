import { useEffect, useState } from 'react';
import { Link, NavLink, Outlet, useNavigate } from 'react-router-dom';
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
  const { me, workspace, db, online, pending, setWorkspace } = useWorkspace();
  const navigate = useNavigate();
  const [resume, setResume] = useState<SessionState | null>(null);
  const [syncOpen, setSyncOpen] = useState(false);

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

        <Link to="/settings/about" className="text-sm underline">About</Link>
        <button className="text-sm underline" onClick={onSignOut}>Sign out</button>
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

      <InstallBanner />

      {syncOpen && <SyncPanel onClose={() => setSyncOpen(false)} />}

      <Outlet />
    </div>
  );
}
