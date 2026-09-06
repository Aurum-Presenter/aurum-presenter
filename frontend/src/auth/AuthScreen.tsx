import { useState } from 'react';
import { ApiError, auth } from '../api/client';
import { startLocalMode, type LocalMode } from './local';

/**
 * Signing in, signing up, and getting back in.
 *
 * This is the one screen that has to work before the router exists, because it is what decides
 * whether there is anything to route to. A password-reset link is picked out of the address bar
 * directly for the same reason.
 */
type Mode = 'sign-in' | 'register' | 'forgot' | 'reset' | 'totp';

export function AuthScreen({
  onSignedIn,
  onLocalMode,
}: {
  onSignedIn: () => Promise<void>;
  onLocalMode: (mode: LocalMode) => void;
}) {
  const resetToken = /^\/auth\/reset\/(.+)$/.exec(window.location.pathname)?.[1] ?? null;

  const [mode, setMode] = useState<Mode>(resetToken === null ? 'sign-in' : 'reset');
  const [email, setEmail] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [password, setPassword] = useState('');
  const [code, setCode] = useState('');
  const [challengeId, setChallengeId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const invited = /^\/invite\/(.+)$/.test(window.location.pathname);

  const run = async (work: () => Promise<void>): Promise<void> => {
    setError(null);
    setBusy(true);

    try {
      await work();
    } catch (problem) {
      setError(problem instanceof ApiError ? problem.message : 'That did not work.');
    } finally {
      setBusy(false);
    }
  };

  const submit = (event: React.FormEvent): void => {
    event.preventDefault();

    void run(async () => {
      switch (mode) {
        case 'sign-in': {
          const result = await auth.login(email, password);

          if (result.totp_required === true && result.challenge_id !== undefined) {
            setChallengeId(result.challenge_id);
            setMode('totp');
            return;
          }

          await onSignedIn();
          return;
        }

        case 'register':
          await auth.register(email, displayName, password);
          await onSignedIn();
          return;

        case 'totp':
          await auth.completeTotp(challengeId!, code);
          await onSignedIn();
          return;

        case 'forgot':
          await auth.forgotPassword(email);
          // Deliberately the same message whether or not the address is registered.
          setNote('If that address has an account, a reset link is on its way.');
          setMode('sign-in');
          return;

        case 'reset':
          await auth.resetPassword(resetToken!, password);
          setNote('Your password is set. Sign in with it.');
          setMode('sign-in');
          window.history.replaceState(null, '', '/');
          return;
      }
    });
  };

  return (
    <div className="flex min-h-dvh items-center justify-center bg-white p-6 text-slate-900">
      <form className="w-80 space-y-3" onSubmit={submit}>
        <h1 className="text-xl font-semibold">
          {{
            'sign-in': 'Sign in',
            register: 'Create an account',
            forgot: 'Reset your password',
            reset: 'Choose a new password',
            totp: 'Two-factor code',
          }[mode]}
        </h1>

        {invited && mode === 'sign-in' && (
          <p className="rounded border border-sky-300 bg-sky-50 p-2 text-sm text-sky-900">
            Sign in with the address the invitation was sent to, and it will be waiting.
          </p>
        )}

        {note !== null && <p className="text-sm text-sky-700">{note}</p>}
        {error !== null && <p className="text-sm text-red-600">{error}</p>}

        {mode === 'totp' ? (
          <>
            <p className="text-sm text-slate-500">
              Enter the six-digit code from your authenticator, or one of your recovery codes.
            </p>
            <input
              className="w-full rounded border border-slate-300 px-3 py-2 tracking-widest"
              inputMode="numeric"
              autoComplete="one-time-code"
              placeholder="000000"
              value={code}
              onChange={(event) => setCode(event.target.value)}
            />
          </>
        ) : (
          <>
            {mode !== 'reset' && (
              <input
                className="w-full rounded border border-slate-300 px-3 py-2"
                type="email"
                autoComplete="email"
                placeholder="Email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
              />
            )}

            {mode === 'register' && (
              <input
                className="w-full rounded border border-slate-300 px-3 py-2"
                placeholder="Your name"
                value={displayName}
                onChange={(event) => setDisplayName(event.target.value)}
              />
            )}

            {mode !== 'forgot' && (
              <input
                className="w-full rounded border border-slate-300 px-3 py-2"
                type="password"
                autoComplete={mode === 'sign-in' ? 'current-password' : 'new-password'}
                placeholder={mode === 'reset' ? 'New password' : 'Password'}
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
            )}
          </>
        )}

        <button className="w-full rounded bg-slate-900 py-2 text-white disabled:opacity-50" disabled={busy}>
          {busy ? 'Just a moment…' : 'Continue'}
        </button>

        {mode === 'sign-in' && (
          <p className="border-t border-slate-200 pt-3 text-center text-sm text-slate-500">
            Or{' '}
            <button type="button" className="underline" onClick={() => onLocalMode(startLocalMode())}>
              use it on this device without an account
            </button>
            . Everything works; you can sign in later and keep what you made.
          </p>
        )}

        <div className="flex justify-between text-sm text-slate-500">
          {mode === 'sign-in' && (
            <>
              <button type="button" className="underline" onClick={() => setMode('register')}>Create an account</button>
              <button type="button" className="underline" onClick={() => setMode('forgot')}>Forgotten password</button>
            </>
          )}

          {(mode === 'register' || mode === 'forgot') && (
            <button type="button" className="underline" onClick={() => setMode('sign-in')}>Back to sign in</button>
          )}
        </div>
      </form>
    </div>
  );
}
