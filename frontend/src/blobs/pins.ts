import type { WorkspaceDb } from '../db/schema';
import { isAutoPinned } from '../sets/repository';
import type { SongPrefs } from '../prefs/songPrefs';

/**
 * What this device is keeping on purpose, and what each one costs.
 *
 * Read when a pinned download will not fit: the app never decides which of a musician's pins
 * matters least, so it shows them, largest first, and lets them choose (business rule 10).
 */
export interface HeldPin {
  kind: 'set' | 'song';
  id: string;
  name: string;
  bytes: number;
  /** Why it is here: asked for by hand, or kept because the date is close. */
  reason: 'pinned' | 'coming-up';
}

export interface PinSources {
  sets: { id: string; name: string; pinned: number; scheduled_for: string | null }[];
  items: { set_id: string; song_id: string | null }[];
  sheets: { id: string; song_id: string }[];
  songs: { id: string; title: string }[];
  cached: { sheet_id: string; size: number }[];
  preferences: { scope_id: string | null; value: string | null }[];
}

export async function heldPins(db: WorkspaceDb, userId: string): Promise<HeldPin[]> {
  const [sets, items, sheets, songs, cached, preferences] = await Promise.all([
    db.sets.filter((row) => row.deleted_at === null).toArray(),
    db.set_items.filter((row) => row.deleted_at === null).toArray(),
    db.sheets.filter((row) => row.deleted_at === null).toArray(),
    db.songs.filter((row) => row.deleted_at === null).toArray(),
    db.blobs.toArray(),
    db.preferences.filter((row) => row.user_id === userId && row.name === 'chart' && row.deleted_at === null).toArray(),
  ]);

  return pinsFrom({ sets, items, sheets, songs, cached, preferences });
}

/** The same, over plain rows: what is held, what it costs, and what can be let go. */
export function pinsFrom(
  { sets, items, sheets, songs, cached, preferences }: PinSources,
  now = new Date(),
): HeldPin[] {
  const bytesBySong = new Map<string, number>();

  for (const record of cached) {
    const sheet = sheets.find((row) => row.id === record.sheet_id);

    if (sheet !== undefined) {
      bytesBySong.set(sheet.song_id, (bytesBySong.get(sheet.song_id) ?? 0) + record.size);
    }
  }

  const held: HeldPin[] = [];

  for (const set of sets) {
    if (! isAutoPinned(set, now)) {
      continue;
    }

    const songIds = new Set(items.filter((item) => item.set_id === set.id && item.song_id !== null).map((item) => item.song_id!));
    const bytes = [...songIds].reduce((total, songId) => total + (bytesBySong.get(songId) ?? 0), 0);

    held.push({ kind: 'set', id: set.id, name: set.name, bytes, reason: set.pinned === 1 ? 'pinned' : 'coming-up' });
  }

  for (const row of preferences) {
    if (row.scope_id === null || row.value === null) {
      continue;
    }

    try {
      if ((JSON.parse(row.value) as SongPrefs).pinned !== true) {
        continue;
      }
    } catch {
      continue;
    }

    held.push({
      kind: 'song',
      id: row.scope_id,
      name: songs.find((song) => song.id === row.scope_id)?.title ?? 'A song',
      bytes: bytesBySong.get(row.scope_id) ?? 0,
      reason: 'pinned',
    });
  }

  return held.sort((a, b) => b.bytes - a.bytes);
}
