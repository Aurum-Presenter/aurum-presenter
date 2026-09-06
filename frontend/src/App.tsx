import { useCallback, useEffect, useMemo, useState } from 'react';
import { account, auth, ApiError, type Account, type Workspace } from './api/client';
import { WorkspaceDb, type Song } from './db/schema';
import { uuidv7 } from './db/uuid';
import { SongPage } from './song/SongPage';
import { SyncEngine } from './sync/engine';

type Phase = 'loading' | 'signed-out' | 'totp' | 'ready';

export function App() {
  const [phase, setPhase] = useState<Phase>('loading');
  const [me, setMe] = useState<Account | null>(null);
  const [challengeId, setChallengeId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadAccount = useCallback(async () => {
    const data = await account.me();
    setMe(data);
    setPhase('ready');
  }, []);

  useEffect(() => {
    // The refresh cookie is httpOnly, so the only way to know whether a session survives a
    // reload is to ask. This is also the one network call the app makes before it will render
    // offline content — and it is allowed to fail.
    auth
      .restore()
      .then((ok) => (ok ? loadAccount() : setPhase('signed-out')))
      .catch(() => setPhase('signed-out'));
  }, [loadAccount]);

  const signIn = async (email: string, password: string) => {
    setError(null);
    try {
      const result = await auth.login(email, password);

      if (result.totp_required && result.challenge_id) {
        setChallengeId(result.challenge_id);
        setPhase('totp');
        return;
      }

      await loadAccount();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : 'Could not sign in.');
    }
  };

  const submitCode = async (code: string) => {
    setError(null);
    try {
      await auth.completeTotp(challengeId!, code);
      setChallengeId(null);
      await loadAccount();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : 'Could not verify that code.');
    }
  };

  if (phase === 'loading') {
    return <Centered>Starting…</Centered>;
  }

  if (phase === 'signed-out') {
    return <SignIn onSubmit={signIn} error={error} />;
  }

  if (phase === 'totp') {
    return <TotpPrompt onSubmit={submitCode} error={error} />;
  }

  return <Library me={me!} onSignOut={() => auth.logout().then(() => setPhase('signed-out'))} />;
}

function Library({ me, onSignOut }: { me: Account; onSignOut: () => void }) {
  const [workspace, setWorkspace] = useState<Workspace>(me.workspaces[0]!);
  const [songs, setSongs] = useState<Song[]>([]);
  const [pending, setPending] = useState(0);
  const [online, setOnline] = useState(navigator.onLine);
  const [title, setTitle] = useState('');
  const [openSongId, setOpenSongId] = useState<string | null>(null);

  const db = useMemo(() => new WorkspaceDb(workspace.id), [workspace.id]);
  const engine = useMemo(() => new SyncEngine(db, workspace.id), [db, workspace.id]);
  const canEdit = workspace.role !== 'viewer';

  const refresh = useCallback(async () => {
    const rows = await db.songs.filter((s) => s.deleted_at === null).toArray();
    setSongs(rows.sort((a, b) => a.title.localeCompare(b.title)));
    setPending(await engine.pendingCount());
  }, [db, engine]);

  useEffect(() => {
    const goOnline = () => setOnline(true);
    const goOffline = () => setOnline(false);
    window.addEventListener('online', goOnline);
    window.addEventListener('offline', goOffline);

    setOpenSongId(null);
    void engine.sync().catch(() => undefined).then(refresh);
    const timer = setInterval(() => void engine.sync().catch(() => undefined).then(refresh), 30_000);

    return () => {
      clearInterval(timer);
      window.removeEventListener('online', goOnline);
      window.removeEventListener('offline', goOffline);
      db.close();
    };
  }, [db, engine, refresh]);

  const openSong = songs.find((song) => song.id === openSongId);

  const addSong = async (event: React.FormEvent) => {
    event.preventDefault();
    if (title.trim() === '') return;

    // Writes land locally and the UI updates immediately; the network is a background concern.
    await engine.record('songs', uuidv7(), 'upsert', { title: title.trim() });
    setTitle('');
    await refresh();
    void engine.sync().catch(() => undefined).then(refresh);
  };

  return (
    <div className="min-h-dvh bg-white text-slate-900 dark:bg-slate-950 dark:text-slate-100">
      <header className="flex flex-wrap items-center gap-3 border-b border-slate-200 px-4 py-3 dark:border-slate-800">
        <h1 className="text-lg font-semibold">Aurum Presenter</h1>

        <select
          className="rounded border border-slate-300 bg-transparent px-2 py-1 text-sm dark:border-slate-700"
          value={workspace.id}
          onChange={(e) => setWorkspace(me.workspaces.find((w) => w.id === e.target.value)!)}
        >
          {me.workspaces.map((w) => (
            <option key={w.id} value={w.id}>{w.name} · {w.role}</option>
          ))}
        </select>

        <span
          className={`ml-auto rounded-full px-3 py-1 text-xs font-medium ${
            online ? 'bg-emerald-100 text-emerald-900' : 'bg-amber-100 text-amber-900'
          }`}
          title="Nothing is blocked while offline; the outbox drains when a connection returns."
        >
          {online ? 'synced' : 'offline'}{pending > 0 ? ` · ${pending} pending` : ''}
        </span>

        <button className="text-sm underline" onClick={onSignOut}>Sign out</button>
      </header>

      {openSong !== undefined ? (
        <SongPage
          db={db}
          engine={engine}
          song={openSong}
          userId={me.id}
          canEdit={canEdit}
          onBack={() => setOpenSongId(null)}
          onChanged={() => { void refresh(); void engine.sync().catch(() => undefined).then(refresh); }}
        />
      ) : (
        <main className="mx-auto max-w-2xl p-4">
          {me.totp.required && !me.totp.enrolled && (
            <p className="mb-4 rounded border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900">
              You own a band workspace, so two-factor authentication is required on this account.
            </p>
          )}

          {canEdit && (
            <form onSubmit={addSong} className="mb-6 flex gap-2">
              <input
                className="flex-1 rounded border border-slate-300 px-3 py-2 dark:border-slate-700 dark:bg-slate-900"
                placeholder="Add a song…"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
              />
              <button className="rounded bg-slate-900 px-4 py-2 text-white dark:bg-slate-100 dark:text-slate-900">
                Add
              </button>
            </form>
          )}

          {songs.length === 0 ? (
            <p className="text-sm text-slate-500">No songs yet. Add one — it works offline.</p>
          ) : (
            <ul className="divide-y divide-slate-200 dark:divide-slate-800">
              {songs.map((song) => (
                <li key={song.id}>
                  <button className="w-full py-2 text-left" onClick={() => setOpenSongId(song.id)}>
                    <span className="font-medium">{song.title}</span>
                    {song.original_key && (
                      <span className="ml-2 text-sm text-slate-500">key of {song.original_key}</span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </main>
      )}
    </div>
  );
}

function SignIn({ onSubmit, error }: { onSubmit: (e: string, p: string) => void; error: string | null }) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');

  return (
    <Centered>
      <form
        className="w-80 space-y-3"
        onSubmit={(e) => { e.preventDefault(); onSubmit(email, password); }}
      >
        <h1 className="text-xl font-semibold">Sign in</h1>
        {error && <p className="text-sm text-red-600">{error}</p>}
        <input
          className="w-full rounded border border-slate-300 px-3 py-2"
          type="email" placeholder="Email" value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
        <input
          className="w-full rounded border border-slate-300 px-3 py-2"
          type="password" placeholder="Password" value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <button className="w-full rounded bg-slate-900 py-2 text-white">Continue</button>
      </form>
    </Centered>
  );
}

function TotpPrompt({ onSubmit, error }: { onSubmit: (code: string) => void; error: string | null }) {
  const [code, setCode] = useState('');

  return (
    <Centered>
      <form className="w-80 space-y-3" onSubmit={(e) => { e.preventDefault(); onSubmit(code); }}>
        <h1 className="text-xl font-semibold">Two-factor code</h1>
        <p className="text-sm text-slate-500">
          Enter the six-digit code from your authenticator, or one of your recovery codes.
        </p>
        {error && <p className="text-sm text-red-600">{error}</p>}
        <input
          className="w-full rounded border border-slate-300 px-3 py-2 tracking-widest"
          inputMode="numeric" autoComplete="one-time-code" placeholder="000000"
          value={code} onChange={(e) => setCode(e.target.value)}
        />
        <button className="w-full rounded bg-slate-900 py-2 text-white">Verify</button>
      </form>
    </Centered>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex min-h-dvh items-center justify-center bg-white p-6 text-slate-900">
      {children}
    </div>
  );
}
