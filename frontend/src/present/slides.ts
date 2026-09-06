import { parseChart, type ChartLine, type Section, type SectionKind } from '../chart/chordpro';

/**
 * Turning a set into slides.
 *
 * The rules are deterministic and run once, at session start (business rule 6): a slide list
 * that could change while the operator is on slide 14 is a slide list that will change while
 * the operator is on slide 14.
 *
 * Slides keep the *parsed* lines rather than rendered text, because the audience and the stage
 * want different things from them — the audience gets lyrics with the chords stripped, and the
 * stage gets chords at its own reader's key.
 */

export type SlideKind = 'lyrics' | 'text' | 'blank' | 'sheet' | 'title';

export interface Slide {
  id: string;
  /** The set item this came from, so the control surface can group slides by song. */
  itemId: string;
  kind: SlideKind;
  songTitle: string;
  /** "Chorus", "Verse 2" — shown on stage always, on the audience only if the theme says so. */
  label: string | null;
  /** Parsed lines: chords and lyrics, untransposed. Empty for blank and sheet slides. */
  lines: ChartLine[];
  /** Plain text for a non-song item. */
  text: string | null;
  sheetId: string | null;
  page: number | null;
  /** The key the lines are written in, so a stage view can transpose them to its own. */
  writtenKey: string | null;
  /** The key the band agreed for this set, if any; it wins over a reader's preference. */
  setKey: string | null;
  capo: number;
}

/** One set item, frozen at session start. Nothing here is read from the database again. */
export interface SnapshotItem {
  itemId: string;
  songId: string | null;
  title: string;
  /** ChordPro, exactly as it was when the session started. */
  body: string | null;
  writtenKey: string | null;
  setKey: string | null;
  capo: number;
  itemType: string | null;
  content: string | null;
  note: string | null;
  sheetId: string | null;
  sheetPages: number | null;
}

export interface Snapshot {
  setId: string | null;
  setName: string;
  items: SnapshotItem[];
  takenAt: string;
}

const LABELS: Partial<Record<SectionKind, string>> = {
  verse: 'Verse', chorus: 'Chorus', bridge: 'Bridge', prechorus: 'Pre-chorus',
  tag: 'Tag', intro: 'Intro', outro: 'Outro',
};

/**
 * How many lines fit on a slide at this theme's size. Derived rather than measured so that two
 * devices building the same session get the same slides — an audience window and a stage device
 * that disagree about slide count cannot follow the same index.
 */
export function linesPerSlide(fontSizeVh: number, safeAreaPct: number): number {
  const usable = 100 - safeAreaPct * 2;

  return Math.max(2, Math.floor(usable / (fontSizeVh * 1.35)));
}

export function buildSlides(snapshot: Snapshot, fontSizeVh = 8, safeAreaPct = 5): Slide[] {
  const perSlide = linesPerSlide(fontSizeVh, safeAreaPct);
  const slides: Slide[] = [];

  snapshot.items.forEach((item, itemIndex) => {
    const base = {
      itemId: item.itemId,
      songTitle: item.title,
      writtenKey: item.writtenKey,
      setKey: item.setKey,
      capo: item.capo,
      text: null,
      sheetId: null,
      page: null,
    };

    // Business rule 4: a non-song item is its own slide, and a blank item is a black one.
    if (item.body === null || item.body.trim() === '') {
      if (item.itemType === 'blank') {
        slides.push({ ...base, id: `${itemIndex}-blank`, kind: 'blank', label: null, lines: [] });
        return;
      }

      if (item.content !== null && item.content.trim() !== '') {
        splitText(item.content, perSlide).forEach((text, part) => {
          slides.push({ ...base, id: `${itemIndex}-text-${part}`, kind: 'text', label: null, lines: [], text });
        });
        return;
      }

      slides.push({ ...base, id: `${itemIndex}-title`, kind: 'title', label: null, lines: [] });
      return;
    }

    // Business rule 5: a sheet item becomes one slide per page.
    if (item.sheetId !== null) {
      const pages = item.sheetPages ?? 1;

      for (let page = 1; page <= pages; page++) {
        slides.push({
          ...base,
          id: `${itemIndex}-sheet-${page}`,
          kind: 'sheet',
          label: null,
          lines: [],
          sheetId: item.sheetId,
          page,
        });
      }

      return;
    }

    const chart = parseChart(item.body);
    const sections = expandRepeats(chart.sections);

    sections.forEach((section, sectionIndex) => {
      const lines = section.lines.filter((line) => ! isEmpty(line));

      if (lines.length === 0) {
        return;
      }

      splitLines(lines, perSlide).forEach((group, part) => {
        slides.push({
          ...base,
          id: `${itemIndex}-${sectionIndex}-${part}`,
          kind: 'lyrics',
          label: labelOf(section),
          lines: group,
        });
      });
    });
  });

  return slides;
}

function labelOf(section: Section): string | null {
  const name = LABELS[section.kind];

  if (name === undefined) {
    return null;
  }

  return section.label === null ? name : `${name} ${section.label}`;
}

function isEmpty(line: ChartLine): boolean {
  return line.comment === null && line.segments.every((segment) => segment.lyric.trim() === '' && segment.chord === null);
}

/**
 * Business rule 3: a bare `{chorus}` after the chorus has been written once is a repeat, and it
 * gets its own slides rather than being silently dropped.
 */
function expandRepeats(sections: Section[]): Section[] {
  const seen = new Map<string, Section>();

  return sections.map((section) => {
    const key = `${section.kind}:${section.label ?? ''}`;
    const hasContent = section.lines.some((line) => ! isEmpty(line));

    if (hasContent) {
      seen.set(key, section);
      return section;
    }

    const earlier = seen.get(key) ?? seen.get(`${section.kind}:`);

    return earlier === undefined ? section : { ...section, lines: earlier.lines };
  });
}

/**
 * Business rule 2: split at a blank line if there is one, then at a sentence end, then at a
 * line end. Never mid-line — a lyric cut in half mid-phrase is worse than a smaller font.
 */
export function splitLines(lines: ChartLine[], perSlide: number): ChartLine[][] {
  if (lines.length <= perSlide) {
    return [lines];
  }

  const groups: ChartLine[][] = [];
  let rest = lines;

  while (rest.length > perSlide) {
    const window = rest.slice(0, perSlide);
    let cut = window.length;

    for (let index = window.length - 1; index >= Math.ceil(perSlide / 2); index--) {
      if (endsSentence(window[index]!)) {
        cut = index + 1;
        break;
      }
    }

    groups.push(rest.slice(0, cut));
    rest = rest.slice(cut);
  }

  if (rest.length > 0) {
    groups.push(rest);
  }

  return groups;
}

function endsSentence(line: ChartLine): boolean {
  return /[.!?]"?\s*$/.test(textOf(line));
}

export function textOf(line: ChartLine): string {
  return line.comment ?? line.segments.map((segment) => segment.lyric).join('');
}

/** The same splitting, for the plain text of an announcement or a reading. */
export function splitText(content: string, perSlide: number): string[] {
  const paragraphs = content.split(/\n\s*\n/).flatMap((paragraph) => {
    const lines = paragraph.split('\n');
    const chunks: string[] = [];

    for (let index = 0; index < lines.length; index += perSlide) {
      chunks.push(lines.slice(index, index + perSlide).join('\n'));
    }

    return chunks;
  });

  return paragraphs.filter((paragraph) => paragraph.trim() !== '');
}
