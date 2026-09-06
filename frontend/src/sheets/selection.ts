import { parseKey, pitchClass, type Key } from '../chart/notes';
import type { Sheet } from '../db/schema';

/**
 * Choosing which sheet to show (business rule 4).
 *
 * A musician opening a song at their key wants their part in that key, and if it does not exist
 * they want to be told what they are looking at instead — never a blank screen. So selection
 * always returns something when anything exists, and says how far it had to fall back.
 */

export type Part = 'lead' | 'piano' | 'vocal' | 'guitar' | 'bass' | 'lyrics' | 'other';

export const PARTS: Part[] = ['lead', 'piano', 'vocal', 'guitar', 'bass', 'lyrics', 'other'];

export type Fallback =
  | 'exact'          // the key and the part asked for
  | 'other-part'     // right key, a different part
  | 'any-key'        // a sheet tagged as fitting any key
  | 'nearest-key'    // the preferred part, in the closest key
  | 'first';         // nothing matched; the first sheet by position

export interface Selection {
  sheet: Sheet;
  fallback: Fallback;
}

/**
 * Distance around the circle of fifths, 0–6. Bb is one step from F and six from B, which is
 * what "nearest key" means to a musician — not the semitone distance.
 */
export function fifthsDistance(a: Key, b: Key): number {
  const steps = (((pitchClass(b.tonic) - pitchClass(a.tonic)) * 7) % 12 + 12) % 12;

  return Math.min(steps, 12 - steps);
}

/**
 * Business rule 5: capo never enters into this. A capo changes the shapes played, not the key
 * the band sounds in, so the sheet stays in the sounding key.
 */
export function selectSheet(sheets: Sheet[], key: Key | null, part: Part | null): Selection | null {
  const usable = sheets
    .filter((sheet) => sheet.deleted_at === null)
    .sort((a, b) => a.position - b.position);

  if (usable.length === 0) {
    return null;
  }

  const inKey = (sheet: Sheet): boolean => {
    if (key === null || sheet.sheet_key === null) {
      return false;
    }

    const sheetKey = parseKey(sheet.sheet_key);

    return sheetKey !== null && pitchClass(sheetKey.tonic) === pitchClass(key.tonic) && sheetKey.minor === key.minor;
  };

  const isPart = (sheet: Sheet): boolean => part === null || sheet.part === part;

  const exact = usable.find((sheet) => inKey(sheet) && isPart(sheet));
  if (exact !== undefined) {
    return { sheet: exact, fallback: 'exact' };
  }

  const sameKey = usable.find(inKey);
  if (sameKey !== undefined) {
    return { sheet: sameKey, fallback: 'other-part' };
  }

  // A sheet with no key is a lyrics sheet or a chart that suits any key; it beats showing a
  // sheet in the wrong key.
  const anyKey = usable.find((sheet) => sheet.sheet_key === null && isPart(sheet));
  if (anyKey !== undefined) {
    return { sheet: anyKey, fallback: 'any-key' };
  }

  if (key !== null) {
    const nearest = [...usable]
      .filter((sheet) => isPart(sheet) && sheet.sheet_key !== null && parseKey(sheet.sheet_key) !== null)
      .sort((a, b) => fifthsDistance(key, parseKey(a.sheet_key!)!) - fifthsDistance(key, parseKey(b.sheet_key!)!))[0];

    if (nearest !== undefined) {
      return { sheet: nearest, fallback: 'nearest-key' };
    }
  }

  return { sheet: usable[0]!, fallback: 'first' };
}

/** What the banner over the viewer says when the sheet is not the one that was asked for. */
export function explain(selection: Selection, key: Key | null, part: Part | null): string | null {
  const sheetKey = selection.sheet.sheet_key ?? 'any key';

  switch (selection.fallback) {
    case 'exact':
      return null;
    case 'other-part':
      return `No ${part ?? 'preferred'} sheet in this key — showing the ${selection.sheet.part ?? 'available'} sheet.`;
    case 'any-key':
      return 'This sheet is not tied to a key.';
    case 'nearest-key':
      return `No sheet in ${key === null ? 'that key' : sheetKeyName(key)} — showing ${sheetKey}, the nearest key available.`;
    case 'first':
      return `Showing the only sheet attached (${selection.sheet.part ?? 'other'}, ${sheetKey}).`;
  }
}

function sheetKeyName(key: Key): string {
  return `${['C', 'D', 'E', 'F', 'G', 'A', 'B'][key.tonic.letter]}${key.tonic.alter > 0 ? '#' : key.tonic.alter < 0 ? 'b' : ''}${key.minor ? 'm' : ''}`;
}
