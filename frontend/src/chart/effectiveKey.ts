import { parseKey, type Key } from './notes';

/**
 * Business rule 7: set override → preferred key → arrangement default → song original key.
 * The first non-null wins, and the UI names the level that supplied it — a musician who sees
 * the wrong key needs to know which of four places to go and change it.
 */
export type KeySource = 'set' | 'preference' | 'arrangement' | 'song' | 'none';

export interface KeyInputs {
  setOverride?: string | null;
  preferred?: string | null;
  arrangementDefault?: string | null;
  songOriginal?: string | null;
}

export interface EffectiveKey {
  key: Key | null;
  source: KeySource;
}

const LEVELS: [KeySource, keyof KeyInputs][] = [
  ['set', 'setOverride'],
  ['preference', 'preferred'],
  ['arrangement', 'arrangementDefault'],
  ['song', 'songOriginal'],
];

export function effectiveKey(inputs: KeyInputs): EffectiveKey {
  for (const [source, field] of LEVELS) {
    const value = inputs[field];

    if (value === null || value === undefined || value === '') {
      continue;
    }

    const key = parseKey(value);

    if (key !== null) {
      return { key, source };
    }
  }

  return { key: null, source: 'none' };
}

/**
 * The key the chart is *written* in, which transposition measures from: the arrangement's own
 * default if it has one, else the song's original key. Never the reader's preference.
 */
export function sourceKey(inputs: Pick<KeyInputs, 'arrangementDefault' | 'songOriginal'>): Key | null {
  return effectiveKey({ arrangementDefault: inputs.arrangementDefault, songOriginal: inputs.songOriginal }).key;
}
