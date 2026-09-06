import { useCallback, useEffect, useState } from 'react';
import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';
import { account, auth, ApiError, type Account } from './api/client';
import { Shell } from './app/Shell';
import { WorkspaceProvider } from './app/workspace';
import { LibraryPage } from './library/LibraryPage';
import { TrashPage } from './library/TrashPage';
import { PrintPage } from './sets/PrintPage';
import { ReaderPage } from './sets/ReaderPage';
import { SetPage } from './sets/SetPage';
import { SetsPage } from './sets/SetsPage';
import { SheetViewerPage } from './sheets/SheetViewerPage';
import { SongPage } from './song/SongPage';

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

  return (
    <WorkspaceProvider me={me!}>
      <BrowserRouter>
        <Routes>
          <Route element={<Shell onSignOut={() => auth.logout().then(() => setPhase('signed-out'))} />}>
            <Route path="/library" element={<LibraryPage />} />
            <Route path="/library/folder/:folderId" element={<LibraryPage />} />
            <Route path="/library/trash" element={<TrashPage />} />
            <Route path="/song/:songId" element={<SongPage />} />
            <Route path="/song/:songId/edit" element={<SongPage edit />} />
            <Route path="/song/:songId/sheet/:sheetId" element={<SheetViewerPage />} />
            <Route path="/sets" element={<SetsPage />} />
            <Route path="/sets/:setId" element={<SetPage />} />
            <Route path="/sets/:setId/read/:index" element={<ReaderPage />} />
            <Route path="/sets/:setId/print" element={<PrintPage />} />
            <Route path="*" element={<Navigate to="/library" replace />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </WorkspaceProvider>
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
