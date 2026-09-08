import { parseChart, type Chart } from '../chart/chordpro';
import { formatNote } from '../chart/notes';
import { detectNotation, toChordPro } from '../chart/overLyrics';

/**
 * Import of the files a band already owns: ChordPro exports and plain chords-over-lyrics text.
 *
 * A partial import is never rolled back (business rule 6). Twenty-eight songs in and two error
 * messages is a good afternoon; twenty-eight songs thrown away because two files were odd is
 * not.
 */

export interface ImportedSong {
  title: string;
  artist: string | null;
  original_key: string | null;
  tempo: number | null;
  time_signature: string | null;
  body: string;
  sourceNotation: 'chordpro' | 'over_lyrics';
  sourceText: string | null;
}

export interface ImportResult {
  filename: string;
  song: ImportedSong | null;
  error: string | null;
}

/** Control characters no chart contains, but a PDF or an image dropped in with them does. */
const BINARY = /[\u0000-\u0008\u000B\u000C\u000E-\u001F]/;

export function importFile(filename: string, text: string): ImportResult {
  const fail = (error: string): ImportResult => ({ filename, song: null, error });

  if (text.trim() === '') {
    return fail('The file is empty.');
  }

  if (BINARY.test(text.slice(0, 2000))) {
    return fail('This is not a text file.');
  }

  const notation = detectNotation(text);
  const body = notation === 'chordpro' ? text : toChordPro(text);
  const chart = parseChart(body);

  if (chart.error !== null) {
    return fail(`Line ${chart.error.line}: ${chart.error.message}`);
  }

  const title = chart.meta.title ?? titleFromFilename(filename);

  if (title.trim() === '') {
    return fail('No title in the file, and none could be taken from its name.');
  }

  return {
    filename,
    error: null,
    song: {
      title: title.trim(),
      artist: chart.meta.artist,
      original_key: chart.meta.key ?? firstChordKey(chart),
      tempo: numberOrNull(chart.meta.tempo),
      time_signature: chart.meta.time,
      body,
      sourceNotation: notation === 'chordpro' ? 'chordpro' : 'over_lyrics',
      sourceText: notation === 'chordpro' ? null : text,
    },
  };
}

/** "01 - Amazing Grace.chopro" becomes "Amazing Grace". */
export function titleFromFilename(filename: string): string {
  return filename
    .replace(/\.[a-z0-9]{1,8}$/i, '')
    .replace(/^\d+[\s._-]+/, '')
    .replace(/_+/g, ' ')
    .trim();
}

/**
 * With no `{key}` directive, the first chord is the best guess a chart can offer — and it is
 * right far more often than it is wrong, because charts start on the one.
 */
function firstChordKey(chart: Chart): string | null {
  for (const section of chart.sections) {
    for (const line of section.lines) {
      for (const segment of line.segments) {
        const chord = segment.chord?.chord;

        if (chord != null) {
          // From the parsed root, not from the raw text: a chart pasted from the web writes
          // its accidentals as ♯ and ♭, and a regex over the text reads E♯ as E.
          const minor = /^(?:m|min)(?![a-z])/.test(chord.quality);

          return formatNote(chord.root) + (minor ? 'm' : '');
        }
      }
    }
  }

  return null;
}

function numberOrNull(text: string | null): number | null {
  if (text === null) {
    return null;
  }

  const value = Number.parseInt(text, 10);

  return Number.isFinite(value) ? value : null;
}
