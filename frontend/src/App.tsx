import { useCallback, useEffect, useState } from 'react';
import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';
import { account, auth, type Account } from './api/client';
import { AuthScreen } from './auth/AuthScreen';
import { claimLocalWorkspace, localAccount, localMode } from './auth/local';
import { Shell } from './app/Shell';
import { WorkspaceProvider } from './app/workspace';
import { LibraryPage } from './library/LibraryPage';
import { TrashPage } from './library/TrashPage';
import { AudiencePage } from './present/AudiencePage';
import { ControlPage } from './present/ControlPage';
import { JoinPage } from './present/JoinPage';
import { StagePage } from './present/StagePage';
import { SharePage } from './pwa/SharePage';
import { UpdateToast } from './pwa/update';
import { PrintPage } from './sets/PrintPage';
import { ReaderPage } from './sets/ReaderPage';
import { SetPage } from './sets/SetPage';
import { SetsPage } from './sets/SetsPage';
import { AboutPage } from './settings/AboutPage';
import { AccountPage } from './settings/AccountPage';
import { ConflictsPage } from './settings/ConflictsPage';
import { InvitePage } from './settings/InvitePage';
import { MembersPage } from './settings/MembersPage';
import { StoragePage } from './settings/StoragePage';
import { SheetViewerPage } from './sheets/SheetViewerPage';
import { SongPage } from './song/SongPage';

type Phase = 'loading' | 'signed-out' | 'ready';

/**
 * The app's two states: signed out, where the only thing that exists is the auth screen, and
 * signed in, where every route reads from the device and the network is a background concern.
 */
export function App() {
  const [phase, setPhase] = useState<Phase>('loading');
  const [me, setMe] = useState<Account | null>(null);

  const [local, setLocal] = useState(false);

  const loadAccount = useCallback(async () => {
    // A workspace started before signing in is handed to the server under the id it already
    // has, so nothing made offline has to be re-created.
    await claimLocalWorkspace();

    const data = await account.me();
    setMe(data);
    setLocal(false);
    setPhase('ready');
  }, []);

  useEffect(() => {
    const started = localMode();

    if (started !== null) {
      // The device is already working without an account; a sign-in screen would be a wall in
      // front of songs that are right here.
      setMe(localAccount(started));
      setLocal(true);
      setPhase('ready');
    }

    // The refresh cookie is httpOnly, so the only way to know whether a session survives a
    // reload is to ask. This is also the one network call the app makes before it will render
    // offline content — and it is allowed to fail.
    // A local-mode device stays where it is if there is no session: it has songs on it, and a
    // sign-in screen in front of them would be a wall, not a door.
    const noSession = (): void => setPhase(started === null ? 'signed-out' : 'ready');

    auth
      .restore()
      .then((ok) => (ok ? loadAccount() : noSession()))
      .catch(noSession);
  }, [loadAccount]);

  if (phase === 'loading') {
    return <div className="flex min-h-dvh items-center justify-center bg-white p-6 text-slate-900">Starting…</div>;
  }

  if (phase === 'signed-out') {
    return (
      <AuthScreen
        onSignedIn={loadAccount}
        onLocalMode={(mode) => { setMe(localAccount(mode)); setLocal(true); setPhase('ready'); }}
      />
    );
  }

  return (
    <WorkspaceProvider me={me!} local={local}>
      <BrowserRouter>
        <Routes>
          {/* Output windows carry no app chrome: they are screens, not pages. */}
          <Route path="/output/audience" element={<AudiencePage />} />
          <Route path="/output/stage" element={<StagePage />} />

          <Route
            element={(
              <Shell
                onSignOut={() => {
                  // In local mode there is no session to end, and a failed logout must not
                  // leave the user stuck inside the app.
                  void auth.logout().catch(() => undefined).then(() => setPhase('signed-out'));
                }}
              />
            )}
          >
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
            <Route path="/present/:sessionId" element={<ControlPage />} />
            <Route path="/join" element={<JoinPage />} />
            <Route path="/invite/:token" element={<InvitePage />} />
            <Route path="/share" element={<SharePage />} />
            <Route path="/settings/account" element={<AccountPage />} />
            <Route path="/settings/members" element={<MembersPage />} />
            <Route path="/settings/sync/conflicts" element={<ConflictsPage />} />
            <Route path="/settings/storage" element={<StoragePage />} />
            <Route path="/settings/trash" element={<TrashPage />} />
            <Route path="/settings/about" element={<AboutPage />} />
            <Route path="*" element={<Navigate to="/library" replace />} />
          </Route>
        </Routes>

        <UpdateToast />
      </BrowserRouter>
    </WorkspaceProvider>
  );
}
