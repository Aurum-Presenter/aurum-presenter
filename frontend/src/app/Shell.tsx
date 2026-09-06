import { Link, NavLink, Outlet } from 'react-router-dom';
import { useWorkspace } from './workspace';

/**
 * The app frame: which workspace, what state the sync is in, and a way out.
 *
 * The sync chip is the only place the network is ever mentioned. Nothing else in the app waits
 * for it, so nothing else needs to talk about it.
 */
export function Shell({ onSignOut }: { onSignOut: () => void }) {
  const { me, workspace, online, pending, syncNow, setWorkspace } = useWorkspace();

  return (
    <div className="min-h-dvh bg-white text-slate-900 dark:bg-slate-950 dark:text-slate-100">
      <header className="flex flex-wrap items-center gap-3 border-b border-slate-200 px-4 py-3 dark:border-slate-800">
        <Link to="/library" className="text-lg font-semibold">Aurum</Link>

        <nav className="flex gap-3 text-sm">
          <NavLink to="/library" className={({ isActive }) => (isActive ? 'font-medium' : 'text-slate-500')}>
            Library
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
          onClick={syncNow}
        >
          {online ? 'synced' : 'offline'}{pending > 0 ? ` · ${pending} pending` : ''}
        </button>

        <button className="text-sm underline" onClick={onSignOut}>Sign out</button>
      </header>

      <Outlet />
    </div>
  );
}
