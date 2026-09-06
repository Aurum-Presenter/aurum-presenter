import { createContext, useContext, useEffect, useMemo, useState } from 'react';
import type { Account, Workspace } from '../api/client';
import { BlobQueue } from '../blobs/queue';
import { BlobStore } from '../blobs/store';
import { computeWanted } from '../blobs/wanted';
import { WorkspaceDb } from '../db/schema';
import { Library } from '../library/repository';
import { readUserPrefs } from '../prefs/userPrefs';
import { SheetsRepository } from '../sheets/repository';
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
  /** True when there is no account yet: everything works, nothing leaves the device. */
  local: boolean;
  workspace: Workspace;
  db: WorkspaceDb;
  engine: SyncEngine;
  library: Library;
  sheets: SheetsRepository;
  blobs: BlobQueue;
  files: BlobStore;
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

export function WorkspaceProvider({ me, local = false, children }: { me: Account; local?: boolean; children: React.ReactNode }) {
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
  const files = useMemo(() => new BlobStore(db, workspace.id), [db, workspace.id]);
  const blobs = useMemo(() => new BlobQueue(db, files, workspace.id), [db, files, workspace.id]);
  const sheets = useMemo(() => new SheetsRepository(db, engine, files), [db, engine, files]);

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
    if (local) {
      // Nothing to sync with. Pretending otherwise would mean a failing request every thirty
      // seconds and an error chip that means nothing.
      return;
    }

    const tick = async (): Promise<void> => {
      await engine.sync().catch(() => undefined);
      setPending(await engine.pendingCount());

      // Files move after the metadata, and never in front of it: a large sheet must not delay a
      // key change reaching the rest of the band.
      const part = (await readUserPrefs(db, me.id)).part;
      await blobs.run(await computeWanted(db, me.id, part)).catch(() => undefined);
    };

    void tick();
    const timer = setInterval(() => void tick(), 30_000);

    return () => clearInterval(timer);
  }, [engine, blobs, db, me.id, local]);

  const value: WorkspaceContextValue = {
    me,
    local,
    workspace,
    db,
    engine,
    library,
    sheets,
    blobs,
    files,
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
