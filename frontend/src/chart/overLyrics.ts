import { parseChord } from './chord';

/**
 * Converting chords-over-lyrics text to ChordPro on entry (business rules 1 and 2).
 *
 * Everything a band already owns is in this format — SongbookPro, OnSong, Ultimate Guitar, a
 * printout typed into a text file. Storing it as-is would mean two chart formats forever, so it
 * is converted once, on paste, and the original is kept in `source_text` for one undo.
 */

export type Notation = 'chordpro' | 'over_lyrics' | 'ambiguous';

const SECTION_HEADING =
  /^\s*\[?\s*(intro|verse|pre-?chorus|chorus|bridge|tag|outro|ending|interlude|instrumental)\s*([0-9]{1,2}|[a-z])?\s*\]?\s*:?\s*$/i;

/** A line is a chord line when it has at least one token and every token is a chord. */
export function isChordLine(line: string): boolean {
  const tokens = line.trim().split(/\s+/).filter((token) => token !== '');

  return tokens.length > 0 && tokens.every((token) => parseChord(token) !== null);
}

export function detectNotation(text: string): Notation {
  if (/\{[a-z_]+\s*[:}]/i.test(text) || /\[[A-Ga-g][^\]\n]{0,12}\]/.test(text)) {
    return 'chordpro';
  }

  const lines = text.replace(/\r\n?/g, '\n').split('\n');

  return lines.some((line) => isChordLine(line)) ? 'over_lyrics' : 'ambiguous';
}

/**
 * Converts chords-over-lyrics to ChordPro, preserving each chord's character column.
 *
 * The column is the whole contract: acceptance criterion 1 says a converted chart re-rendered
 * over its lyrics must put every chord back in the column it was pasted at. So a chord binds to
 * the character offset it sits above in the next non-blank line, and a chord past the end of
 * that line keeps its column as trailing spaces.
 */
export function toChordPro(text: string): string {
  const lines = text.replace(/\r\n?/g, '\n').split('\n');
  const output: string[] = [];

  for (let index = 0; index < lines.length; index++) {
    const line = lines[index]!;

    const heading = SECTION_HEADING.exec(line);
    if (heading !== null) {
      const label = heading[2];
      output.push(`{${heading[1]!.toLowerCase().replace('-', '')}${label === undefined ? '' : ': ' + label}}`);
      continue;
    }

    if (! isChordLine(line)) {
      output.push(line.trimEnd());
      continue;
    }

    const chords = positionsOf(line);
    const next = lines[index + 1];

    if (next === undefined || next.trim() === '' || isChordLine(next) || SECTION_HEADING.test(next)) {
      output.push(merge('', chords));
      continue;
    }

    output.push(merge(next.trimEnd(), chords));
    index++;
  }

  return output.join('\n').replace(/\n{3,}/g, '\n\n').trim() + '\n';
}

interface Placement {
  column: number;
  chord: string;
}

function positionsOf(line: string): Placement[] {
  const placements: Placement[] = [];
  const pattern = /\S+/g;

  let match: RegExpExecArray | null;
  while ((match = pattern.exec(line)) !== null) {
    placements.push({ column: match.index, chord: match[0] });
  }

  return placements;
}

/** Inserts `[chord]` into a lyric line at each chord's column, right to left so offsets hold. */
function merge(lyric: string, chords: Placement[]): string {
  let result = lyric;

  for (const placement of [...chords].reverse()) {
    if (placement.column > result.length) {
      result = result.padEnd(placement.column, ' ');
    }

    result = result.slice(0, placement.column) + `[${placement.chord}]` + result.slice(placement.column);
  }

  return result;
}
