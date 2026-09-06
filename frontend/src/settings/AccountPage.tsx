import { useState } from 'react';
import { Link } from 'react-router-dom';
import { account, ApiError, type Account } from '../api/client';
import { useWorkspace } from '../app/workspace';

/**
 * The account: who you are, and the second factor.
 *
 * An owner of a band workspace must hold a second factor, so this page is where that invariant
 * is satisfied — and where it refuses to be undone by anyone still holding an ownership.
 */
export function AccountPage() {
  const { me } = useWorkspace();
  const [state, setState] = useState<Account['totp']>(me.totp);

  const [enrolling, setEnrolling] = useState<{ uri: string; secret: string } | null>(null);
  const [code, setCode] = useState('');
  const [password, setPassword] = useState('');
  const [codes, setCodes] = useState<string[] | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const run = async (work: () => Promise<void>): Promise<void> => {
    setProblem(null);

    try {
      await work();
    } catch (error) {
      setProblem(error instanceof ApiError ? error.message : 'That did not work.');
    }
  };

  return (
    <div className="mx-auto max-w-2xl p-4">
      <Link className="text-sm underline" to="/library">← Library</Link>
      <h2 className="mb-1 mt-3 text-2xl font-semibold">Account</h2>
      <p className="mb-4 text-sm text-slate-500">{me.display_name} · {me.email}</p>

      {problem !== null && <p className="mb-3 rounded border border-red-300 bg-red-50 p-2 text-sm text-red-800">{problem}</p>}
      {note !== null && <p className="mb-3 rounded border border-sky-300 bg-sky-50 p-2 text-sm text-sky-900">{note}</p>}

      <section className="mb-6">
        <h3 className="mb-1 font-semibold">Two-factor authentication</h3>

        {state.required && ! state.enrolled && (
          <p className="mb-2 rounded border border-amber-300 bg-amber-50 p-2 text-sm text-amber-900">
            You own a band workspace, so this account needs a second factor.
          </p>
        )}

        {state.enrolled ? (
          <div className="text-sm">
            <p className="mb-2 text-slate-500">
              Enabled. {state.recovery_codes_remaining} recovery code(s) left.
            </p>

            <div className="mb-2 flex flex-wrap gap-2">
              <input
                className="rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-900"
                type="password"
                placeholder="your password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
              <input
                className="w-28 rounded border border-slate-300 px-2 py-1 tracking-widest dark:border-slate-700 dark:bg-slate-900"
                inputMode="numeric"
                placeholder="000000"
                value={code}
                onChange={(event) => setCode(event.target.value)}
              />
              <button
                className="rounded border border-slate-300 px-3 dark:border-slate-700"
                onClick={() => void run(async () => {
                  const result = await account.totp.newRecoveryCodes(password, code);
                  setCodes(result.recovery_codes);
                  setCode('');
                  setPassword('');
                })}
              >
                New recovery codes
              </button>
              <button
                className="rounded border border-red-300 px-3 text-red-700 dark:text-red-400"
                onClick={() => void run(async () => {
                  await account.totp.disable(password, code);
                  setState({ ...state, enrolled: false });
                  setCode('');
                  setPassword('');
                  setNote('Two-factor authentication is off.');
                })}
              >
                Turn off
              </button>
            </div>

            <p className="text-xs text-slate-500">
              Turning it off is refused while you own a band workspace — hand ownership over
              first, or it would leave the band without one.
            </p>
          </div>
        ) : enrolling === null ? (
          <button
            className="rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900"
            onClick={() => void run(async () => {
              const result = await account.totp.enrol();
              setEnrolling({ uri: result.provisioning_uri, secret: result.secret });
            })}
          >
            Set up two-factor authentication
          </button>
        ) : (
          <div className="text-sm">
            <p className="mb-2 text-slate-500">
              Add this to your authenticator, then type the six digits it shows.
            </p>
            <p className="mb-2 break-all rounded bg-slate-100 p-2 font-mono text-xs dark:bg-slate-800">{enrolling.secret}</p>
            <p className="mb-2 break-all text-xs text-slate-400">{enrolling.uri}</p>

            <div className="flex gap-2">
              <input
                className="w-28 rounded border border-slate-300 px-2 py-1 tracking-widest dark:border-slate-700 dark:bg-slate-900"
                inputMode="numeric"
                placeholder="000000"
                value={code}
                onChange={(event) => setCode(event.target.value)}
              />
              <button
                className="rounded bg-slate-900 px-3 text-white dark:bg-slate-100 dark:text-slate-900"
                onClick={() => void run(async () => {
                  const result = await account.totp.confirm(code);
                  setCodes(result.recovery_codes);
                  setState({ ...state, enrolled: true, recovery_codes_remaining: result.recovery_codes.length });
                  setEnrolling(null);
                  setCode('');
                })}
              >
                Confirm
              </button>
            </div>
          </div>
        )}
      </section>

      {codes !== null && (
        <section className="mb-6 rounded border border-slate-300 p-3 dark:border-slate-700">
          <h3 className="mb-1 font-semibold">Recovery codes</h3>
          <p className="mb-2 text-sm text-slate-500">
            Shown once. Each works one time, in place of a code from your authenticator. Keep
            them somewhere that is not this device.
          </p>
          <ul className="mb-2 grid grid-cols-2 gap-1 font-mono text-sm">
            {codes.map((value) => <li key={value}>{value}</li>)}
          </ul>
          <button className="text-sm underline" onClick={() => void navigator.clipboard?.writeText(codes.join('\n'))}>
            Copy them
          </button>
        </section>
      )}

      <ul className="space-y-1 text-sm">
        <li><Link className="underline" to="/settings/members">Members of this workspace</Link></li>
        <li><Link className="underline" to="/settings/storage">Offline storage</Link></li>
        <li><Link className="underline" to="/settings/about">About</Link></li>
      </ul>
    </div>
  );
}
