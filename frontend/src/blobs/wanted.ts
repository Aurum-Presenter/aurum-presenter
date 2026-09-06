import { effectiveKey } from '../chart/effectiveKey';
import type { WorkspaceDb } from '../db/schema';
import type { SongPrefs } from '../prefs/songPrefs';
import type { Part } from '../sheets/selection';
import { isAutoPinned } from '../sets/repository';
import { wantedSheets, type SheetWant } from './policy';
import type { Released } from './released';

/**
 * The sheets this device should be holding, read out of the local database.
 *
 * Two sources: the sets that are coming up (or pinned), each in the key it will be played in,
 * and the songs the user has pinned by hand — less anything this device has been told to let
 * go of, which beats both.
 */
export async function computeWanted(
  db: WorkspaceDb,
  userId: string,
  part: Part | null,
  released: Released = { sets: [], songs: [] },
): Promise<Set<string>> {
  const [sets, items, sheets, songs, arrangements, preferences] = await Promise.all([
    db.sets.filter((row) => row.deleted_at === null).toArray(),
    db.set_items.filter((row) => row.deleted_at === null).toArray(),
    db.sheets.filter((row) => row.deleted_at === null).toArray(),
    db.songs.filter((row) => row.deleted_at === null).toArray(),
    db.arrangements.filter((row) => row.deleted_at === null).toArray(),
    db.preferences.filter((row) => row.user_id === userId && row.name === 'chart' && row.deleted_at === null).toArray(),
  ]);

  const prefs = new Map<string, SongPrefs>();

  for (const row of preferences) {
    if (row.scope_id !== null && row.value !== null) {
      try {
        prefs.set(row.scope_id, JSON.parse(row.value) as SongPrefs);
      } catch {
        // Unreadable preference, no preference.
      }
    }
  }

  const keyFor = (songId: string, override: string | null): SheetWant => {
    const song = songs.find((row) => row.id === songId);
    const preference = prefs.get(songId);
    const forSong = arrangements.filter((row) => row.song_id === songId);
    const arrangement = forSong.find((row) => row.id === preference?.arrangement_id)
      ?? forSong.find((row) => row.is_default === 1)
      ?? forSong[0];

    return {
      songId,
      key: effectiveKey({
        setOverride: override,
        preferred: preference?.preferred_key ?? null,
        arrangementDefault: arrangement?.default_key ?? null,
        songOriginal: song?.original_key ?? null,
      }).key,
    };
  };

  const wants: SheetWant[] = [];
  const letGo = { sets: new Set(released.sets), songs: new Set(released.songs) };
  const pinnedSets = new Set(
    sets.filter((set) => isAutoPinned(set) && ! letGo.sets.has(set.id)).map((set) => set.id),
  );

  for (const item of items) {
    if (item.song_id !== null && pinnedSets.has(item.set_id) && ! letGo.songs.has(item.song_id)) {
      wants.push(keyFor(item.song_id, item.key_override));
    }
  }

  for (const [songId, preference] of prefs) {
    if (preference.pinned === true && ! letGo.songs.has(songId)) {
      wants.push(keyFor(songId, null));
    }
  }

  return wantedSheets(sheets, wants, part);
}
