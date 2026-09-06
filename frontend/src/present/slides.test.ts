import { describe, expect, it } from 'vitest';
import { buildSlides, linesPerSlide, splitText, textOf, type Snapshot, type SnapshotItem } from './slides';

function item(overrides: Partial<SnapshotItem> = {}): SnapshotItem {
  return {
    itemId: 'item-1',
    songId: 'song-1',
    title: 'Amazing Grace',
    body: null,
    writtenKey: 'G',
    setKey: null,
    capo: 0,
    itemType: null,
    content: null,
    note: null,
    sheetId: null,
    sheetPages: null,
    ...overrides,
  };
}

const snapshot = (items: SnapshotItem[]): Snapshot => ({
  setId: 'set-1', setName: 'Sunday', items, takenAt: '2026-09-06T09:00:00Z',
});

describe('slide building', () => {
  it('makes one slide per section, labelled', () => {
    const body = '{verse: 1}\n[G]Amazing grace\nhow sweet the sound\n\n{chorus}\n[C]My chains are gone';
    const slides = buildSlides(snapshot([item({ body })]));

    expect(slides.map((slide) => slide.label)).toEqual(['Verse 1', 'Chorus']);
    expect(slides[0]!.lines.map(textOf)).toEqual(['Amazing grace', 'how sweet the sound']);
  });

  it('keeps chords on the slide so the stage can transpose them itself', () => {
    const slides = buildSlides(snapshot([item({ body: '{verse}\n[G]Amazing [D]grace' })]));

    expect(slides[0]!.lines[0]!.segments[0]!.chord?.text).toBe('G');
    expect(slides[0]!.writtenKey).toBe('G');
  });

  // Business rule 3.
  it('expands a repeated section into its own slides', () => {
    const body = '{chorus}\nMy chains are gone\n\n{verse: 2}\nThe Lord has promised\n\n{chorus}';
    const slides = buildSlides(snapshot([item({ body })]));

    expect(slides.map((slide) => slide.label)).toEqual(['Chorus', 'Verse 2', 'Chorus']);
    expect(slides[2]!.lines.map(textOf)).toEqual(['My chains are gone']);
  });

  // Business rule 2: split at a sentence end rather than mid-thought, and never mid-line.
  it('splits a long section at a sentence end', () => {
    const body = [
      '{verse}',
      'Line one goes here.',
      'Line two continues',
      'and line three ends it.',
      'Line four starts again',
      'line five',
      'line six',
    ].join('\n');

    const slides = buildSlides(snapshot([item({ body })]), 20, 5);

    expect(slides.length).toBeGreaterThan(1);
    expect(textOf(slides[0]!.lines.at(-1)!)).toBe('and line three ends it.');
    expect(slides.flatMap((slide) => slide.lines.map(textOf))).toHaveLength(6);
  });

  it('never splits a line in half', () => {
    const body = '{verse}\n' + Array.from({ length: 20 }, (_, index) => `line number ${index}`).join('\n');
    const slides = buildSlides(snapshot([item({ body })]), 12, 5);

    for (const slide of slides) {
      for (const line of slide.lines) {
        expect(textOf(line)).toMatch(/^line number \d+$/);
      }
    }
  });

  // Business rule 4.
  it('turns a non-song item into text slides, and a blank item into a black one', () => {
    const slides = buildSlides(snapshot([
      item({ itemId: 'a', songId: null, title: 'Notices', itemType: 'announcement', content: 'Coffee is in the hall' }),
      item({ itemId: 'b', songId: null, title: 'Blank', itemType: 'blank' }),
    ]));

    expect(slides[0]).toMatchObject({ kind: 'text', text: 'Coffee is in the hall' });
    expect(slides[1]).toMatchObject({ kind: 'blank' });
  });

  // Business rule 5.
  it('makes one slide per page of a sheet item', () => {
    const slides = buildSlides(snapshot([item({ body: '{verse}\nSomething', sheetId: 'sheet-1', sheetPages: 3 })]));

    expect(slides).toHaveLength(3);
    expect(slides.map((slide) => slide.page)).toEqual([1, 2, 3]);
    expect(slides[0]!.kind).toBe('sheet');
  });

  it('carries the set key so every output agrees on it', () => {
    const slides = buildSlides(snapshot([item({ body: '{verse}\nA line', setKey: 'A', capo: 2 })]));

    expect(slides[0]).toMatchObject({ setKey: 'A', capo: 2 });
  });

  it('is deterministic, so two devices build the same list', () => {
    const input = snapshot([item({ body: '{verse}\nOne\nTwo\nThree\n\n{chorus}\nFour' })]);

    expect(buildSlides(input, 8, 5).map((slide) => slide.id))
      .toEqual(buildSlides(input, 8, 5).map((slide) => slide.id));
  });
});

describe('fitting', () => {
  it('fits fewer lines as the font grows', () => {
    expect(linesPerSlide(4, 5)).toBeGreaterThan(linesPerSlide(10, 5));
    expect(linesPerSlide(40, 5)).toBeGreaterThanOrEqual(2);
  });

  it('splits plain text on its paragraphs', () => {
    expect(splitText('One\n\nTwo\n\n', 4)).toEqual(['One', 'Two']);
    expect(splitText('a\nb\nc\nd', 2)).toEqual(['a\nb', 'c\nd']);
  });
});
