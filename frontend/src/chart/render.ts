import { formatChord, nashville, transposeChord, type ChordToken } from './chord';
import type { Chart, ChartLine, Section } from './chordpro';
import { pitchClass, transposeKey, type Key } from './notes';

/**
 * Turning the parsed model into text on screen.
 *
 * Transposition lives here rather than in the store because it is a display transform and
 * nothing else (business rule 6): the ChordPro body is byte-identical before and after any
 * number of key changes, which is what lets two members read the same chart in two keys.
 */

export type Layout = 'inline' | 'over' | 'nashville';

export interface RenderOptions {
  /** The key the stored body is written in. */
  source: Key;
  /** The sounding key the reader wants. */
  target: Key;
  /** 0–11. Shapes are drawn `capo` semitones below the sounding key (business rule 11). */
  capo: number;
  layout: Layout;
}

export interface RenderedSegment {
  chord: string | null;
  lyric: string;
}

export interface RenderedLine {
  segments: RenderedSegment[];
  comment: string | null;
}

export interface RenderedSection {
  kind: Section['kind'];
  label: string | null;
  lines: RenderedLine[];
}

export interface RenderedChart {
  sections: RenderedSection[];
  /** The key the chord shapes are in — the sounding key, unless a capo is fitted. */
  shapeKey: Key;
  /** True when a spelling needed a double accidental and a simpler enharmonic was used. */
  respelled: boolean;
}

/** The key the shapes are written in: sounding key minus the capo position. */
export function shapeKeyOf(target: Key, capo: number): Key {
  return capo === 0 ? target : transposeKey(target, -capo);
}

export function renderChart(chart: Chart, options: RenderOptions): RenderedChart {
  const shapeKey = shapeKeyOf(options.target, options.capo);
  const semitones = (((pitchClass(shapeKey.tonic) - pitchClass(options.source.tonic)) % 12) + 12) % 12;

  let respelled = false;

  const renderToken = (token: ChordToken | null): string | null => {
    if (token === null) {
      return null;
    }

    if (token.chord === null) {
      // Business rule 3: unreadable tokens stay exactly as written, in chord position.
      return token.text;
    }

    if (options.layout === 'nashville') {
      // Degrees are relative to the key, so they are the same before and after transposition —
      // and a capo cannot change them either.
      return nashville(token.chord, options.source);
    }

    const transposed = transposeChord(token.chord, semitones, shapeKey);
    respelled = respelled || transposed.respelled;

    return formatChord(transposed.chord);
  };

  const sections = chart.sections.map((section) => ({
    kind: section.kind,
    label: section.label,
    lines: section.lines.map((line: ChartLine): RenderedLine => ({
      comment: line.comment,
      segments: line.segments.map((segment) => ({
        chord: renderToken(segment.chord),
        lyric: segment.lyric,
      })),
    })),
  }));

  return { sections, shapeKey, respelled };
}

/**
 * The two rows of a chords-over-lyrics line: chords padded to the column their lyric starts at.
 *
 * This is the inverse of the paste converter, and acceptance criterion 1 depends on the pair
 * round-tripping — a chord pasted at column 12 is stored at offset 12 and lands back at 12.
 */
export function overLyricsRows(line: RenderedLine): { chords: string; lyrics: string } {
  let chords = '';
  let lyrics = '';

  for (const segment of line.segments) {
    if (segment.chord !== null) {
      // A chord whose column is already covered by the previous chord's text is nudged right,
      // so two chords never run together into one unreadable token.
      if (chords.length > lyrics.length) {
        lyrics = lyrics.padEnd(chords.length + 1, ' ');
      }

      chords = chords.padEnd(lyrics.length, ' ') + segment.chord;
    }

    lyrics += segment.lyric;
  }

  return { chords: chords.trimEnd(), lyrics };
}

/** Inline layout: `[G]` brackets, as the body is stored. */
export function inlineText(line: RenderedLine): string {
  return line.segments
    .map((segment) => (segment.chord === null ? '' : `[${segment.chord}]`) + segment.lyric)
    .join('');
}
