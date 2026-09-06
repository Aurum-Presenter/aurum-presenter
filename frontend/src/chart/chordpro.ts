import { readToken, type ChordToken } from './chord';

/**
 * ChordPro is the canonical storage, and this turns it into the section model everything else
 * reads: rendering, transposition and — later — presenter slides. Business rule 5: a chart is
 * parsed once and transposed on the model, never by string replacement on the raw text.
 */

export type SectionKind =
  | 'verse' | 'chorus' | 'bridge' | 'prechorus' | 'tag' | 'intro' | 'outro' | 'none';

/** A chord (or none) and the lyric that runs from it to the next chord. */
export interface Segment {
  chord: ChordToken | null;
  lyric: string;
}

export interface ChartLine {
  segments: Segment[];
  /** `{comment: ...}` — a performance note, not a lyric. */
  comment: string | null;
}

export interface Section {
  kind: SectionKind;
  /** `{verse: 2}` → "2". Presenter slides use this to label the slide. */
  label: string | null;
  lines: ChartLine[];
}

export interface ChartWarning {
  /** 1-based, so the editor gutter can point at the source line. */
  line: number;
  token: string;
}

export interface ChartMeta {
  title: string | null;
  subtitle: string | null;
  artist: string | null;
  key: string | null;
  tempo: string | null;
  time: string | null;
  capo: string | null;
}

export interface Chart {
  meta: ChartMeta;
  sections: Section[];
  /** Tokens in chord position that are not chords. They render verbatim and still save. */
  warnings: ChartWarning[];
  /** Set when the source could not be read at all; the view falls back to plain text. */
  error: { line: number; message: string } | null;
}

const SECTION_ALIASES: Record<string, SectionKind> = {
  verse: 'verse', sov: 'verse', start_of_verse: 'verse',
  chorus: 'chorus', soc: 'chorus', start_of_chorus: 'chorus',
  bridge: 'bridge', sob: 'bridge', start_of_bridge: 'bridge',
  prechorus: 'prechorus', pre_chorus: 'prechorus',
  tag: 'tag', intro: 'intro', outro: 'outro', ending: 'outro',
};

const SECTION_ENDS = ['eov', 'eoc', 'eob', 'end_of_verse', 'end_of_chorus', 'end_of_bridge'];

const META_ALIASES: Record<string, keyof ChartMeta> = {
  title: 'title', t: 'title',
  subtitle: 'subtitle', st: 'subtitle',
  artist: 'artist', composer: 'artist',
  key: 'key', tempo: 'tempo', bpm: 'tempo',
  time: 'time', capo: 'capo',
};

/**
 * Parsing runs on the main thread, deliberately, at every length.
 *
 * The feature document asks for charts over 500 lines to be parsed in the search worker. On
 * this parser a 500-line chart takes under 2 ms and a 10,000-line one about 24 ms, so posting
 * the text across and structured-cloning the model back would cost more than the parse and
 * would put a frame of empty screen in front of a musician who is reading. If a future change
 * makes parsing genuinely expensive — a layout pass, say — the worker is there and this is the
 * one place that would have to move.
 */
export function parseChart(body: string): Chart {
  const meta: ChartMeta = {
    title: null, subtitle: null, artist: null, key: null, tempo: null, time: null, capo: null,
  };
  const warnings: ChartWarning[] = [];
  const sections: Section[] = [];

  let current: Section = { kind: 'none', label: null, lines: [] };
  let error: Chart['error'] = null;

  const flush = (): void => {
    if (current.lines.length > 0 || current.kind !== 'none') {
      sections.push(current);
    }
  };

  const lines = body.replace(/\r\n?/g, '\n').split('\n');

  lines.forEach((raw, index) => {
    const number = index + 1;
    const text = raw.trimEnd();
    const directive = /^\s*\{(.*)\}\s*$/.exec(text);

    if (directive !== null) {
      const [name, value] = splitDirective(directive[1]!);

      if (name in META_ALIASES) {
        meta[META_ALIASES[name]!] = value;
        return;
      }

      if (name === 'comment' || name === 'c' || name === 'ci') {
        current.lines.push({ segments: [], comment: value ?? '' });
        return;
      }

      if (name in SECTION_ALIASES) {
        flush();
        current = { kind: SECTION_ALIASES[name]!, label: value, lines: [] };
        return;
      }

      if (SECTION_ENDS.includes(name)) {
        flush();
        current = { kind: 'none', label: null, lines: [] };
        return;
      }

      // An unknown directive is not an error. Charts travel between apps and carry each
      // other's extensions; dropping one must never cost the user their lyrics.
      return;
    }

    if (text.startsWith('#')) {
      return;
    }

    if (error === null && unbalanced(text)) {
      error = { line: number, message: 'Unclosed [ in chord position.' };
    }

    current.lines.push(parseLine(text, number, warnings));
  });

  flush();

  return { meta, sections, warnings, error };
}

function splitDirective(inner: string): [string, string | null] {
  const colon = inner.indexOf(':');
  const name = (colon === -1 ? inner : inner.slice(0, colon)).trim().toLowerCase().replace(/[\s-]+/g, '_');
  const value = colon === -1 ? null : inner.slice(colon + 1).trim();

  return [name, value === '' ? null : value];
}

function unbalanced(text: string): boolean {
  return text.split('[').length !== text.split(']').length;
}

function parseLine(text: string, number: number, warnings: ChartWarning[]): ChartLine {
  const segments: Segment[] = [];
  let lyric = '';
  let pending: ChordToken | null = null;
  let index = 0;

  while (index < text.length) {
    const character = text[index]!;

    if (character !== '[') {
      lyric += character;
      index++;
      continue;
    }

    const close = text.indexOf(']', index);

    if (close === -1) {
      lyric += text.slice(index);
      break;
    }

    if (lyric !== '' || pending !== null) {
      segments.push({ chord: pending, lyric });
    }

    pending = readToken(text.slice(index + 1, close));

    if (pending.chord === null && ! pending.marker && pending.text.trim() !== '') {
      warnings.push({ line: number, token: pending.text });
    }

    lyric = '';
    index = close + 1;
  }

  if (lyric !== '' || pending !== null || segments.length === 0) {
    segments.push({ chord: pending, lyric });
  }

  return { segments, comment: null };
}

/** True when the line carries no lyric text at all — a chord-only line, or a blank one. */
export function isChordOnly(line: ChartLine): boolean {
  return line.comment === null && line.segments.every((segment) => segment.lyric.trim() === '');
}

export function isBlank(line: ChartLine): boolean {
  return line.comment === null && line.segments.every((s) => s.chord === null && s.lyric.trim() === '');
}

/**
 * Parsing is memoised per `(arrangement id, updated_at)` — business rule 5. The key changes on
 * every save, so a stale model cannot outlive the text it came from.
 */
const cache = new Map<string, Chart>();

export function parseCached(key: string, body: string): Chart {
  const cached = cache.get(key);

  if (cached !== undefined) {
    return cached;
  }

  const chart = parseChart(body);

  // A band's library is thousands of songs; the cache is a working set, not a store.
  if (cache.size > 64) {
    cache.clear();
  }

  cache.set(key, chart);

  return chart;
}
