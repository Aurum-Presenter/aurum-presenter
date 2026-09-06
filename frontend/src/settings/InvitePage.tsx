import { useEffect, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { ApiError, invites } from '../api/client';

/**
 * Accepting an invitation.
 *
 * The link says which workspace and which address it was sent to before anything happens, so
 * somebody signed in as the wrong account can see that before they wonder why it failed.
 */
export function InvitePage() {
  const { token } = useParams();
  const navigate = useNavigate();

  const [preview, setPreview] = useState<Awaited<ReturnType<typeof invites.preview>>['invite'] | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [joined, setJoined] = useState<string | null>(null);

  useEffect(() => {
    void invites.preview(token!)
      .then((response) => setPreview(response.invite))
      .catch((error: unknown) => setProblem(error instanceof ApiError ? error.message : 'That link is not valid.'));
  }, [token]);

  const accept = async (): Promise<void> => {
    setProblem(null);

    try {
      const result = await invites.accept(token!);
      setJoined(result.workspace.name);

      // The workspace list on the account is stale now; a reload is the honest way to refresh
      // it, and it happens once.
      setTimeout(() => { window.location.assign('/library'); }, 1200);
    } catch (error) {
      setProblem(error instanceof ApiError ? error.message : 'That did not work.');
    }
  };

  return (
    <div className="mx-auto max-w-md p-6">
      <h2 className="mb-3 text-xl font-semibold">Invitation</h2>

      {problem !== null && <p className="mb-3 rounded border border-red-300 bg-red-50 p-2 text-sm text-red-800">{problem}</p>}

      {joined !== null ? (
        <p className="text-sm">You have joined {joined}. Taking you to the library…</p>
      ) : preview === null ? (
        <p className="text-sm text-slate-500">Checking the link…</p>
      ) : (
        <div className="text-sm">
          <p className="mb-3">
            You have been invited to <strong>{preview.workspace_name}</strong> as {preview.role}, at{' '}
            <strong>{preview.email}</strong>.
          </p>

          {preview.used && <p className="mb-3 text-amber-700 dark:text-amber-400">This invitation has already been used.</p>}
          {preview.expired && <p className="mb-3 text-amber-700 dark:text-amber-400">This invitation has expired.</p>}

          <div className="flex gap-3">
            <button
              className="rounded bg-slate-900 px-4 py-2 text-white disabled:opacity-40 dark:bg-slate-100 dark:text-slate-900"
              disabled={preview.used || preview.expired}
              onClick={() => void accept()}
            >
              Accept
            </button>
            <button className="underline" onClick={() => navigate('/library')}>Not now</button>
          </div>
        </div>
      )}
    </div>
  );
}
