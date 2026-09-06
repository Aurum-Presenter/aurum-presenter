import { createContext, useContext, useEffect, useMemo, useState } from 'react';
import type { Account, Workspace } from '../api/client';
import { WorkspaceDb } from '../db/schema';
import { Library } from '../library/repository';
import { SyncEngine } from '../sync/engine';

/**
 * Everything a page needs to read and write the current workspace.
 *
 * The database, the outbox and the library helpers are per workspace, and switching workspace
 * means opening a different Dexie database rather than filtering a shared one — the same shape
 * the server has, where a workspace *is* a file.
 */
export interface WorkspaceContextValue {
  me: Account;
  workspace: Workspace;
  db: WorkspaceDb;
  engine: SyncEngine;
  library: Library;
  canEdit: boolean;
  online: boolean;
  pending: number;
  syncNow: () => void;
  setWorkspace: (id: string) => void;
}

const Context = createContext<WorkspaceContextValue | null>(null);

export function useWorkspace(): WorkspaceContextValue {
  const value = useContext(Context);

  if (value === null) {
    throw new Error('useWorkspace outside a WorkspaceProvider');
  }

  return value;
}

export function WorkspaceProvider({ me, children }: { me: Account; children: React.ReactNode }) {
  const [workspaceId, setWorkspaceId] = useState(() => {
    const remembered = localStorage.getItem('aurum.workspace');

    return me.workspaces.some((w) => w.id === remembered) ? remembered! : me.workspaces[0]?.id ?? '';
  });

  const workspace = me.workspaces.find((w) => w.id === workspaceId) ?? me.workspaces[0]!;
  const [online, setOnline] = useState(navigator.onLine);
  const [pending, setPending] = useState(0);

  const db = useMemo(() => new WorkspaceDb(workspace.id), [workspace.id]);
  const engine = useMemo(() => new SyncEngine(db, workspace.id), [db, workspace.id]);
  const library = useMemo(() => new Library(db, engine), [db, engine]);

  useEffect(() => () => db.close(), [db]);

  useEffect(() => {
    const goOnline = (): void => setOnline(true);
    const goOffline = (): void => setOnline(false);
    window.addEventListener('online', goOnline);
    window.addEventListener('offline', goOffline);

    return () => {
      window.removeEventListener('online', goOnline);
      window.removeEventListener('offline', goOffline);
    };
  }, []);

  useEffect(() => {
    const tick = (): void => {
      void engine.sync().catch(() => undefined).then(() => engine.pendingCount().then(setPending));
    };

    tick();
    const timer = setInterval(tick, 30_000);

    return () => clearInterval(timer);
  }, [engine]);

  const value: WorkspaceContextValue = {
    me,
    workspace,
    db,
    engine,
    library,
    canEdit: workspace.role !== 'viewer',
    online,
    pending,
    syncNow: () => void engine.sync().catch(() => undefined).then(() => engine.pendingCount().then(setPending)),
    setWorkspace: (id) => {
      localStorage.setItem('aurum.workspace', id);
      setWorkspaceId(id);
    },
  };

  return <Context.Provider value={value}>{children}</Context.Provider>;
}
