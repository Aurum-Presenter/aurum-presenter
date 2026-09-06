import type { WorkspaceDb } from '../db/schema';
import { uuidv7 } from '../db/uuid';
import type { SyncEngine } from '../sync/engine';

/**
 * Per-user, per-song reading preferences: preferred key, capo, and which arrangement to open.
 *
 * These sync — a musician who sets "always in G" on their laptop wants it on their phone at the
 * gig — but they are never shared: the row is keyed by user, and every member has their own.
 */
export interface SongPrefs {
  preferred_key: string | null;
  /** Null means "not chosen", which is not the same as 0 — the arranger's capo hint fills it. */
  capo: number | null;
  arrangement_id: string | null;
}

export const NO_PREFS: SongPrefs = { preferred_key: null, capo: null, arrangement_id: null };

const NAME = 'chart';

export async function readSongPrefs(db: WorkspaceDb, userId: string, songId: string): Promise<SongPrefs> {
  const row = await find(db, userId, songId);

  if (row?.value == null) {
    return NO_PREFS;
  }

  try {
    return { ...NO_PREFS, ...(JSON.parse(row.value) as Partial<SongPrefs>) };
  } catch {
    return NO_PREFS;
  }
}

/**
 * Writes through the outbox like every other change, so setting a key on a plane is no
 * different from setting one at home.
 */
export async function writeSongPrefs(
  db: WorkspaceDb,
  engine: SyncEngine,
  userId: string,
  songId: string,
  prefs: SongPrefs,
): Promise<void> {
  const existing = await find(db, userId, songId);

  await engine.record('preferences', existing?.id ?? uuidv7(), 'upsert', {
    user_id: userId,
    scope_type: 'song',
    scope_id: songId,
    name: NAME,
    value: JSON.stringify(prefs),
  });
}

function find(db: WorkspaceDb, userId: string, songId: string) {
  return db.preferences
    .where('[user_id+scope_type+scope_id+name]')
    .equals([userId, 'song', songId, NAME])
    .first();
}
