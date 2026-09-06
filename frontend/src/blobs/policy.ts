import type { Key } from '../chart/notes';
import type { Sheet } from '../db/schema';
import { selectSheet, type Part } from '../sheets/selection';

/**
 * Which sheet files this device keeps.
 *
 * The rule the features agree on: what is coming up, and what the user asked to keep. A set
 * inside its window brings the sheets for the keys that set will actually be played in — not
 * every sheet of every song in it, which on a phone is the difference between a few megabytes
 * and a few hundred.
 */
export interface SheetWant {
  songId: string;
  key: Key | null;
}

export function wantedSheets(sheets: Sheet[], wants: SheetWant[], part: Part | null): Set<string> {
  const wanted = new Set<string>();

  for (const want of wants) {
    const forSong = sheets.filter((sheet) => sheet.song_id === want.songId && sheet.deleted_at === null);
    const chosen = selectSheet(forSong, want.key, part);

    if (chosen !== null && chosen.sheet.sha256 !== null) {
      wanted.add(chosen.sheet.id);
    }

    // A song pinned deliberately keeps every part in that key, because the person who pinned it
    // does not necessarily know which part they will need on the night.
    for (const sheet of forSong) {
      if (sheet.sha256 !== null && sheet.sheet_key !== null && chosen?.fallback === 'exact'
        && sheet.sheet_key === chosen.sheet.sheet_key) {
        wanted.add(sheet.id);
      }
    }
  }

  return wanted;
}
