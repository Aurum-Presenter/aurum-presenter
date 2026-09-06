import type { Account, Workspace } from '../api/client';
import { workspaces as workspaceApi } from '../api/client';
import { uuidv7 } from '../db/uuid';

/**
 * Using the app without an account.
 *
 * Everything works: songs, charts, sets, presentation. What is missing is other people. The
 * workspace id is generated on the device, so when the user does sign in, the same id is
 * claimed server-side and every record they already made keeps the identity it was born with —
 * nothing is re-created and nothing is re-downloaded.
 */
const KEY = 'aurum.local';

export interface LocalMode {
  workspaceId: string;
  name: string;
  startedAt: string;
}

export function localMode(): LocalMode | null {
  try {
    const stored = localStorage.getItem(KEY);

    return stored === null ? null : (JSON.parse(stored) as LocalMode);
  } catch {
    return null;
  }
}

export function startLocalMode(name = 'My songs'): LocalMode {
  const mode: LocalMode = { workspaceId: uuidv7(), name, startedAt: new Date().toISOString() };

  localStorage.setItem(KEY, JSON.stringify(mode));

  return mode;
}

export function endLocalMode(): void {
  localStorage.removeItem(KEY);
}

/**
 * The account object the app runs on before there is an account. It is a real workspace with a
 * real id — the same one the server will be given when it is claimed.
 */
export function localAccount(mode: LocalMode): Account {
  const workspace: Workspace = {
    id: mode.workspaceId,
    name: mode.name,
    kind: 'personal',
    role: 'owner',
    created_at: mode.startedAt,
    updated_at: mode.startedAt,
  };

  return {
    id: 'local-device',
    email: '',
    display_name: 'This device',
    totp: { enrolled: false, required: false, recovery_codes_remaining: 0 },
    workspaces: [workspace],
  };
}

/**
 * Hands the local workspace to the server under the id it already has. Called once, straight
 * after a first sign-in; from then on the outbox pushes everything that was made offline.
 */
export async function claimLocalWorkspace(): Promise<Workspace | null> {
  const mode = localMode();

  if (mode === null) {
    return null;
  }

  try {
    const { workspace } = await workspaceApi.create(mode.name, mode.workspaceId);
    endLocalMode();

    // Select it, too. Everything this person has made is in here, and the account they have
    // just created also has an empty personal workspace that would otherwise win by being
    // first in the list.
    localStorage.setItem('aurum.workspace', workspace.id);

    return workspace;
  } catch {
    // Already claimed, or no connection. Either way the local data stays where it is and the
    // claim can be retried on the next sign-in.
    return null;
  }
}
