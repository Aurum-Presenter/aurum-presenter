import type { WorkspaceDb } from '../db/schema';
import { uuidv7 } from '../db/uuid';
import type { Part } from '../sheets/selection';
import type { SyncEngine } from '../sync/engine';

/**
 * Preferences that belong to a member across the whole workspace rather than to one song.
 *
 * The preferred part is the obvious one: a pianist wants the piano sheet every time, without
 * saying so once per song. It syncs, because it is a fact about the person, not the device.
 */
const NAME = 'workspace';

export interface UserPrefs {
  part: Part | null;
}

export const NO_USER_PREFS: UserPrefs = { part: null };

export async function readUserPrefs(db: WorkspaceDb, userId: string): Promise<UserPrefs> {
  const row = await find(db, userId);

  if (row?.value == null) {
    return NO_USER_PREFS;
  }

  try {
    return { ...NO_USER_PREFS, ...(JSON.parse(row.value) as Partial<UserPrefs>) };
  } catch {
    return NO_USER_PREFS;
  }
}

export async function writeUserPrefs(
  db: WorkspaceDb,
  engine: SyncEngine,
  userId: string,
  prefs: UserPrefs,
): Promise<void> {
  const existing = await find(db, userId);

  await engine.record('preferences', existing?.id ?? uuidv7(), 'upsert', {
    user_id: userId,
    scope_type: 'workspace',
    scope_id: null,
    name: NAME,
    value: JSON.stringify(prefs),
  });
}

function find(db: WorkspaceDb, userId: string) {
  return db.preferences
    .filter((row) => row.user_id === userId && row.scope_type === 'workspace' && row.name === NAME)
    .first();
}
