import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { ApiError, invites as inviteApi, members as memberApi, type Invite, type Member } from '../api/client';
import { useWorkspace } from '../app/workspace';

/**
 * Who is in this workspace, and who has been asked.
 *
 * Roles are read from the membership row on the server; this page is a view of that, and for
 * anyone who is not an owner it is read-only — the controls are hidden rather than disabled,
 * because a disabled button is a promise the app cannot keep.
 */
export function MembersPage() {
  const { workspace, me } = useWorkspace();
  const owner = workspace.role === 'owner';

  const [people, setPeople] = useState<Member[] | null>(null);
  const [pending, setPending] = useState<Invite[]>([]);
  const [email, setEmail] = useState('');
  const [role, setRole] = useState<Invite['role']>('editor');
  const [link, setLink] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    setProblem(null);

    void memberApi.list(workspace.id)
      .then((response) => setPeople(response.members))
      .catch(() => setProblem('The member list needs a connection.'));

    if (owner) {
      void inviteApi.list(workspace.id).then((response) => setPending(response.invites)).catch(() => undefined);
    }
  }, [workspace.id, owner]);

  const act = async (work: Promise<unknown>): Promise<void> => {
    setProblem(null);

    try {
      await work;
    } catch (error) {
      setProblem(error instanceof ApiError ? error.message : 'That did not work.');
    }
  };

  return (
    <div className="mx-auto max-w-3xl p-4">
      <Link className="text-sm underline" to="/library">← Library</Link>
      <h2 className="mb-1 mt-3 text-2xl font-semibold">{workspace.name}</h2>
      <p className="mb-4 text-sm text-slate-500">
        Owners manage members. Editors change songs, charts and sets. Viewers read everything and
        keep their own keys, capos and notes.
      </p>

      {problem !== null && (
        <p className="mb-3 rounded border border-amber-300 bg-amber-50 p-2 text-sm text-amber-900">{problem}</p>
      )}

      <ul className="mb-6 divide-y divide-slate-200 dark:divide-slate-800">
        {(people ?? []).map((member) => (
          <li key={member.user_id} className="flex flex-wrap items-center gap-3 py-2 text-sm">
            <span className="font-medium">{member.display_name}</span>
            <span className="text-slate-500">{member.email}</span>
            {member.user_id === me.id && <span className="text-xs text-slate-400">you</span>}

            {owner ? (
              <span className="ml-auto flex items-center gap-3">
                <select
                  className="rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
                  value={member.role}
                  onChange={(event) => void act(
                    memberApi.setRole(workspace.id, member.user_id, event.target.value as Member['role'])
                      .then((response) => setPeople(response.members)),
                  )}
                >
                  <option value="owner">owner</option>
                  <option value="editor">editor</option>
                  <option value="viewer">viewer</option>
                </select>

                <button
                  className="underline text-red-700 dark:text-red-400"
                  onClick={() => void act(
                    memberApi.remove(workspace.id, member.user_id).then((response) => setPeople(response.members)),
                  )}
                >
                  Remove
                </button>
              </span>
            ) : (
              <span className="ml-auto text-slate-500">{member.role}</span>
            )}
          </li>
        ))}

        {people?.length === 0 && <li className="py-2 text-sm text-slate-500">Nobody else is here yet.</li>}
      </ul>

      {owner && (
        <section>
          <h3 className="mb-2 font-semibold">Invite someone</h3>

          <form
            className="mb-3 flex flex-wrap gap-2"
            onSubmit={(event) => {
              event.preventDefault();
              void act(
                inviteApi.create(workspace.id, email, role).then((response) => {
                  setPending(response.pending);
                  setLink(response.link);
                  setEmail('');
                }),
              );
            }}
          >
            <input
              className="min-w-56 flex-1 rounded border border-slate-300 px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-900"
              type="email"
              placeholder="their email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
            />

            <select
              className="rounded border border-slate-300 bg-transparent px-2 py-2 text-sm dark:border-slate-700"
              value={role}
              onChange={(event) => setRole(event.target.value as Invite['role'])}
            >
              <option value="editor">editor</option>
              <option value="viewer">viewer</option>
            </select>

            <button className="rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900">
              Send invitation
            </button>
          </form>

          <p className="mb-3 text-xs text-slate-500">
            Ownership is granted after someone has joined and has two-factor authentication on
            their account — that is why it is not in this list.
          </p>

          {link !== null && (
            <p className="mb-3 rounded border border-sky-300 bg-sky-50 p-2 text-sm text-sky-900">
              Invitation sent. You can also hand it over directly:
              <button className="ml-2 underline" onClick={() => void navigator.clipboard?.writeText(link)}>copy link</button>
            </p>
          )}

          {pending.length > 0 && (
            <ul className="divide-y divide-slate-200 text-sm dark:divide-slate-800">
              {pending.map((invite) => (
                <li key={invite.id} className="flex items-center gap-3 py-2">
                  <span>{invite.email}</span>
                  <span className="text-slate-500">{invite.role}</span>
                  <span className="text-xs text-slate-400">expires {invite.expires_at.slice(0, 10)}</span>
                  {invite.not_sent === true && (
                    <span className="text-xs text-amber-700 dark:text-amber-400" title="The mail server refused it five times">
                      not sent
                    </span>
                  )}
                  <button
                    className="ml-auto underline"
                    onClick={() => void act(inviteApi.revoke(workspace.id, invite.id).then((response) => setPending(response.invites)))}
                  >
                    Withdraw
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
    </div>
  );
}
