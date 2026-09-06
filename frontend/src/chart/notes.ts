/**
 * Notes and keys, spelled the way a key signature spells them.
 *
 * The whole point of this module is that a pitch is not a name. Pitch class 6 is `F#` in B
 * major and `Gb` in Db major, and a transposer that keeps a fixed sharp table gets one of the
 * two wrong every time. So a note is a letter plus an alteration, never a semitone index, and
 * naming a pitch always happens *in a key*.
 */

/** 0 = C, 1 = D, … 6 = B. Letters, not semitones — the distinction is the module. */
export type Letter = 0 | 1 | 2 | 3 | 4 | 5 | 6;

export interface Note {
  letter: Letter;
  /** −2 … +2. Negative is flat, positive is sharp. */
  alter: number;
}

export interface Key {
  tonic: Note;
  minor: boolean;
}

const LETTER_NAMES = ['C', 'D', 'E', 'F', 'G', 'A', 'B'] as const;
const LETTER_PITCH = [0, 2, 4, 5, 7, 9, 11] as const;
const MAJOR_STEPS = [0, 2, 4, 5, 7, 9, 11] as const;
/** Natural minor, so a minor key can number its own degrees without borrowing the relative major's. */
const MINOR_STEPS = [0, 2, 3, 5, 7, 8, 10] as const;

const SHARP_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'] as const;
const FLAT_NAMES = ['C', 'Db', 'D', 'Eb', 'E', 'F', 'Gb', 'G', 'Ab', 'A', 'Bb', 'B'] as const;

export function pitchClass(note: Note): number {
  return (((LETTER_PITCH[note.letter] + note.alter) % 12) + 12) % 12;
}

export function formatNote(note: Note): string {
  const accidental = note.alter >= 0 ? '#'.repeat(note.alter) : 'b'.repeat(-note.alter);

  return LETTER_NAMES[note.letter] + accidental;
}

export function sameNote(a: Note, b: Note): boolean {
  return a.letter === b.letter && a.alter === b.alter;
}

/** Parses a bare note name. `♯`/`♭` are accepted because that is what a paste from the web contains. */
export function parseNote(text: string): Note | null {
  const match = /^([A-Ga-g])(##|bb|[#b♯♭x])?$/.exec(text.trim());

  if (match === null) {
    return null;
  }

  const letter = LETTER_NAMES.indexOf(match[1]!.toUpperCase() as (typeof LETTER_NAMES)[number]) as Letter;

  return { letter, alter: alterOf(match[2]) };
}

function alterOf(accidental: string | undefined): number {
  switch (accidental) {
    case '#':
    case '♯':
      return 1;
    case '##':
    case 'x':
      return 2;
    case 'b':
    case '♭':
      return -1;
    case 'bb':
      return -2;
    default:
      return 0;
  }
}

/** Accepts `C`, `Am`, `Bb`, `F#m`, `Ebmin`, `C major`, `a minor`. */
export function parseKey(text: string): Key | null {
  const trimmed = text.trim();
  const match = /^([A-Ga-g](?:##|bb|[#b♯♭])?)\s*(m|min|minor|maj|major)?$/.exec(trimmed);

  if (match === null) {
    return null;
  }

  const tonic = parseNote(match[1]!);

  if (tonic === null) {
    return null;
  }

  const mode = (match[2] ?? '').toLowerCase();

  return { tonic, minor: mode === 'm' || mode === 'min' || mode === 'minor' };
}

export function formatKey(key: Key): string {
  return formatNote(key.tonic) + (key.minor ? 'm' : '');
}

export function keysEqual(a: Key, b: Key): boolean {
  return a.minor === b.minor && sameNote(a.tonic, b.tonic);
}

/**
 * The seven notes of the key, spelled by its signature.
 *
 * A minor key is spelled from its relative major (business rule 9), so `Am` transposed up three
 * semitones lands on `Cm` spelled with the Eb-major signature rather than with D#.
 */
export function scaleOf(key: Key): Note[] {
  const steps = key.minor ? MINOR_STEPS : MAJOR_STEPS;
  const tonicPitch = pitchClass(key.tonic);

  return steps.map((step, degree) => {
    const letter = ((key.tonic.letter + degree) % 7) as Letter;
    const natural = LETTER_PITCH[letter];
    const wanted = (tonicPitch + step) % 12;

    return { letter, alter: normaliseAlter(wanted - natural) };
  });
}

/** Brings a raw semitone difference into −6…+5, so B→Cb reads as −1 rather than +11. */
function normaliseAlter(difference: number): number {
  return ((((difference + 6) % 12) + 12) % 12) - 6;
}

/**
 * True when the key signature leans sharp. Chromatic notes are then spelled as a raised lower
 * degree — which is why B major's raised fourth is `E#` and not `F`.
 */
export function isSharpKey(key: Key): boolean {
  const lean = scaleOf(key).reduce((total, note) => total + note.alter, 0);

  // C major and A minor have no signature at all; sharps are the reading convention there.
  return lean >= 0;
}

export interface Spelled {
  note: Note;
  /**
   * True when the key's own spelling would have needed a double accidental (`Fx`, `Bbb`) and a
   * simpler enharmonic was substituted — business rule 14. The chart header says "respelled".
   */
  respelled: boolean;
}

/** Names a pitch class in a key: diatonic spelling first, then the key's own accidental side. */
export function spell(pitch: number, key: Key): Spelled {
  const target = ((pitch % 12) + 12) % 12;
  const scale = scaleOf(key);

  const diatonic = scale.find((note) => pitchClass(note) === target);

  if (diatonic !== undefined) {
    return { note: diatonic, respelled: false };
  }

  const sharp = isSharpKey(key);
  const neighbour = scale.find(
    (note) => pitchClass(note) === (sharp ? (target + 11) % 12 : (target + 1) % 12),
  );

  if (neighbour !== undefined) {
    const candidate: Note = { letter: neighbour.letter, alter: neighbour.alter + (sharp ? 1 : -1) };

    if (Math.abs(candidate.alter) <= 1) {
      return { note: candidate, respelled: false };
    }
  }

  return { note: parseNote((sharp ? SHARP_NAMES : FLAT_NAMES)[target]!)!, respelled: true };
}

/** Semitones from `from` up to `to`, always 0…11. */
export function interval(from: Key, to: Key): number {
  return (((pitchClass(to.tonic) - pitchClass(from.tonic)) % 12) + 12) % 12;
}

/** Moves a key by semitones, keeping the mode and spelling the new tonic from the target itself. */
export function transposeKey(key: Key, semitones: number): Key {
  const pitch = (((pitchClass(key.tonic) + semitones) % 12) + 12) % 12;

  // The target key names itself: of the two candidate spellings, take the one whose own
  // signature is the simpler — Db over C#, but F# over Gb.
  const candidates = [SHARP_NAMES[pitch]!, FLAT_NAMES[pitch]!]
    .map((name) => ({ tonic: parseNote(name)!, minor: key.minor }))
    .filter((candidate, index, all) => index === all.findIndex((c) => sameNote(c.tonic, candidate.tonic)));

  return candidates.reduce((best, candidate) => (signatureSize(candidate) < signatureSize(best) ? candidate : best));
}

function signatureSize(key: Key): number {
  return scaleOf(key).reduce((total, note) => total + Math.abs(note.alter), 0);
}

/**
 * Business rule 13: the same target key is reachable up or down, and the smaller move wins.
 * Returns the signed interval in −5…+6.
 */
export function shortestInterval(from: Key, to: Key): number {
  const up = interval(from, to);

  return up > 6 ? up - 12 : up;
}

/**
 * Scale degree of a pitch in a key, as Nashville notation writes it: `1`, `4`, `b3`, `#4`.
 */
export function degreeOf(note: Note, key: Key): string {
  const scale = scaleOf(key);
  const index = (((note.letter - key.tonic.letter) % 7) + 7) % 7;
  const expected = pitchClass(scale[index]!);
  const actual = pitchClass(note);

  const difference = normaliseAlter(actual - expected);
  const accidental = difference > 0 ? '#'.repeat(difference) : 'b'.repeat(-difference);

  return accidental + String(index + 1);
}
