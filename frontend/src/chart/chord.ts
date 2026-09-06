import {
  degreeOf,
  formatNote,
  parseNote,
  pitchClass,
  spell,
  type Key,
  type Note,
} from './notes';

/**
 * The chord grammar of business rule 3: root, optional accidental, optional quality, optional
 * bass. Anything that does not match is not a chord — it is left verbatim in chord position and
 * flagged in the editor gutter, which is why parsing returns `null` rather than throwing.
 */
export interface Chord {
  root: Note;
  /** The suffix exactly as written: `m7`, `sus4`, `maj9#11`. Transposition never touches it. */
  quality: string;
  bass: Note | null;
}

/** A token in chord position: either a chord, or text we could not read and must not lose. */
export interface ChordToken {
  text: string;
  chord: Chord | null;
  /** `N.C.` and friends: not a chord, but not a mistake either — no gutter warning. */
  marker: boolean;
}

const MARKERS = ['n.c.', 'nc', 'x', '%', '/'];

/**
 * Quality atoms, longest first. A quality is valid when it is a concatenation of these — which
 * accepts `m7b5`, `maj9#11`, `sus4add9` and `6/9`, and rejects `mm`, `zz` and `Hmm`.
 */
const ATOMS = [
  'maj', 'major', 'min', 'minor', 'sus2', 'sus4', 'sus', 'add', 'dim', 'aug', 'alt', 'no',
  'm', 'M', 'Δ', '°', 'ø', '+', '-', '(', ')', ',', ' ', '/', '#', 'b', '♯', '♭',
];

const NUMBERS = ['13', '11', '9', '7', '6', '5', '4', '3', '2'];

export function parseChord(text: string): Chord | null {
  const token = text.trim();

  if (token === '') {
    return null;
  }

  const [head, bass] = splitBass(token);
  const rootMatch = /^([A-G])(##|bb|[#b♯♭])?/.exec(head);

  if (rootMatch === null) {
    return null;
  }

  const root = parseNote(rootMatch[0]);
  const quality = head.slice(rootMatch[0].length);

  if (root === null || ! isQuality(quality)) {
    return null;
  }

  return { root, quality, bass };
}

/** Splits `Am7/G` into `Am7` and the bass note. A trailing `6/9` is a quality, not a bass. */
function splitBass(token: string): [string, Note | null] {
  const slash = token.lastIndexOf('/');

  if (slash <= 0) {
    return [token, null];
  }

  const bass = parseNote(token.slice(slash + 1));

  return bass === null ? [token, null] : [token.slice(0, slash), bass];
}

function isQuality(quality: string): boolean {
  let rest = quality;

  outer: while (rest !== '') {
    for (const number of NUMBERS) {
      if (rest.startsWith(number)) {
        rest = rest.slice(number.length);
        continue outer;
      }
    }

    for (const atom of ATOMS) {
      if (rest.startsWith(atom)) {
        rest = rest.slice(atom.length);
        continue outer;
      }
    }

    return false;
  }

  return true;
}

/** Reads a token in chord position, keeping unreadable text rather than discarding it. */
export function readToken(text: string): ChordToken {
  const chord = parseChord(text);

  if (chord !== null) {
    return { text, chord, marker: false };
  }

  return { text, chord: null, marker: MARKERS.includes(text.trim().toLowerCase()) };
}

export function formatChord(chord: Chord): string {
  return formatNote(chord.root) + chord.quality + (chord.bass === null ? '' : '/' + formatNote(chord.bass));
}

export interface TransposedChord {
  chord: Chord;
  /** True when the key's spelling would have needed a double accidental — business rule 14. */
  respelled: boolean;
}

/**
 * Transposes a chord by an interval and spells the result in the target key. The bass moves by
 * the same interval and obeys the same spelling rules (business rule 10).
 */
export function transposeChord(chord: Chord, semitones: number, target: Key): TransposedChord {
  const root = spell(pitchClass(chord.root) + semitones, target);
  const bass = chord.bass === null ? null : spell(pitchClass(chord.bass) + semitones, target);

  return {
    chord: { root: root.note, quality: chord.quality, bass: bass?.note ?? null },
    respelled: root.respelled || (bass?.respelled ?? false),
  };
}

/** Nashville numbers: `G C D Em` in G becomes `1 4 5 6m`, slash chords becoming `4/6`. */
export function nashville(chord: Chord, key: Key): string {
  const bass = chord.bass === null ? '' : '/' + degreeOf(chord.bass, key);

  return degreeOf(chord.root, key) + chord.quality + bass;
}
