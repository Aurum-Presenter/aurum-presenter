import { describe, expect, it } from 'vitest';
import { formatChord, nashville, parseChord, transposeChord } from './chord';
import { parseChart } from './chordpro';
import { effectiveKey } from './effectiveKey';
import { formatKey, parseKey, shortestInterval, spell, transposeKey } from './notes';
import { detectNotation, isChordLine, toChordPro } from './overLyrics';
import { inlineText, overLyricsRows, renderChart, shapeKeyOf } from './render';

const key = (text: string) => parseKey(text)!;

/** Renders every chord of a body at a target key, in reading order. */
function chordsOf(body: string, from: string, to: string, capo = 0, layout: 'inline' | 'over' | 'nashville' = 'inline') {
  const rendered = renderChart(parseChart(body), {
    source: key(from),
    target: key(to),
    capo,
    layout,
  });

  return rendered.sections
    .flatMap((section) => section.lines)
    .flatMap((line) => line.segments)
    .map((segment) => segment.chord)
    .filter((chord): chord is string => chord !== null);
}

describe('chord grammar', () => {
  it('reads root, quality and bass', () => {
    expect(formatChord(parseChord('Am7/G')!)).toBe('Am7/G');
    expect(formatChord(parseChord('Bbmaj9#11')!)).toBe('Bbmaj9#11');
    expect(formatChord(parseChord('F#sus4')!)).toBe('F#sus4');
    expect(formatChord(parseChord('C6/9')!)).toBe('C6/9');
  });

  it('rejects anything that is not a chord', () => {
    expect(parseChord('Hmm')).toBeNull();
    expect(parseChord('Be')).toBeNull();
    expect(parseChord('the')).toBeNull();
    expect(parseChord('')).toBeNull();
  });
});

describe('transposition', () => {
  // Acceptance criterion 2 — the enharmonic follows the target key signature, not a table.
  it('spells the fourth as Gb in Db and as F# in B', () => {
    expect(chordsOf('[F]Amazing [G]grace', 'C', 'Db')).toEqual(['Gb', 'Ab']);
    expect(chordsOf('[F]Amazing [G]grace', 'C', 'B')).toEqual(['E', 'F#']);
  });

  // Acceptance criterion 3.
  it('moves a slash bass by the same interval', () => {
    const result = transposeChord(parseChord('Am7/G')!, 5, key('Dm'));

    expect(formatChord(result.chord)).toBe('Dm7/C');
  });

  // Business rule 9 — a minor key spells from its relative major.
  it('transposes Am up three semitones into Cm spelled with the Eb signature', () => {
    expect(formatKey(transposeKey(key('Am'), 3))).toBe('Cm');
    expect(chordsOf('[Am] [F] [C] [G]', 'Am', 'Cm')).toEqual(['Cm', 'Ab', 'Eb', 'Bb']);
  });

  // Business rule 14 — no Fx, no Bbb; the chart reports that it was respelled.
  it('falls back to a simpler enharmonic instead of a double accidental', () => {
    const spelled = spell(2, key('C#'));

    expect(spelled.note.alter).toBeLessThanOrEqual(1);
    expect(spelled.respelled).toBe(true);

    // C# raised a semitone lands on a pitch the C#-major signature can only spell as C##.
    const rendered = renderChart(parseChart('[C#]'), { source: key('C'), target: key('C#'), capo: 0, layout: 'inline' });
    expect(rendered.respelled).toBe(true);
    expect(chordsOf('[C#]', 'C', 'C#')).toEqual(['D']);
  });

  // Business rule 13 — the same key is reachable both ways; the shorter move wins.
  it('normalises an interval into one octave and prefers the shorter direction', () => {
    expect(shortestInterval(key('C'), key('A'))).toBe(-3);
    expect(shortestInterval(key('C'), key('D'))).toBe(2);
  });

  it('leaves the stored body untouched whatever key it is read in', () => {
    const body = '[G]Nothing [C]here [D]changes';
    const chart = parseChart(body);

    renderChart(chart, { source: key('G'), target: key('Bb'), capo: 3, layout: 'over' });
    renderChart(chart, { source: key('G'), target: key('E'), capo: 0, layout: 'nashville' });

    expect(inlineText(renderChart(chart, { source: key('G'), target: key('G'), capo: 0, layout: 'inline' }).sections[0]!.lines[0]!))
      .toBe(body);
  });
});

describe('capo', () => {
  // Acceptance criterion 4 — capo 2 sounding in D means shapes in C.
  it('draws shapes below the sounding key without changing it', () => {
    expect(formatKey(shapeKeyOf(key('D'), 2))).toBe('C');
    expect(chordsOf('[D] [G] [A]', 'D', 'D', 2)).toEqual(['C', 'F', 'G']);
  });
});

describe('nashville', () => {
  // Acceptance criterion 7.
  it('numbers degrees relative to the key', () => {
    expect(chordsOf('[G] [C] [D] [Em]', 'G', 'G', 0, 'nashville')).toEqual(['1', '4', '5', '6m']);
  });

  it('numbers a slash chord on both sides', () => {
    expect(nashville(parseChord('C/E')!, key('G'))).toBe('4/6');
  });

  it('marks a borrowed chord with an accidental', () => {
    expect(nashville(parseChord('Bb')!, key('C'))).toBe('b7');
  });
});

describe('unreadable tokens', () => {
  // Acceptance criterion 6.
  it('renders verbatim and flags the line, without failing the parse', () => {
    const chart = parseChart('[Hmm]Something');

    expect(chart.warnings).toEqual([{ line: 1, token: 'Hmm' }]);
    expect(chordsOf('[Hmm]Something', 'C', 'D')).toEqual(['Hmm']);
  });

  it('does not flag a no-chord marker', () => {
    expect(parseChart('[N.C.]Spoken').warnings).toEqual([]);
  });
});

describe('chords over lyrics', () => {
  const pasted = [
    'Verse 1',
    'G           C',
    'Amazing grace how sweet',
    '       D        G',
    'the sound of it all',
  ].join('\n');

  it('detects the notation it was given', () => {
    expect(detectNotation(pasted)).toBe('over_lyrics');
    expect(detectNotation('{title: X}\n[G]Amazing')).toBe('chordpro');
    expect(detectNotation('just some words')).toBe('ambiguous');
    expect(isChordLine('G           C')).toBe(true);
    expect(isChordLine('Amazing grace')).toBe(false);
  });

  // Acceptance criterion 1 — every chord returns to the column it was pasted at.
  it('round-trips chord columns through ChordPro', () => {
    const chart = parseChart(toChordPro(pasted));
    const rendered = renderChart(chart, { source: key('G'), target: key('G'), capo: 0, layout: 'over' });
    const rows = rendered.sections.flatMap((s) => s.lines).map(overLyricsRows).filter((row) => row.chords !== '');

    expect(rows[0]!.chords).toBe('G           C');
    expect(rows[0]!.lyrics).toBe('Amazing grace how sweet');
    expect(rows[1]!.chords).toBe('       D        G');
    expect(rows[1]!.lyrics).toBe('the sound of it all');
  });

  it('turns a plain section heading into a directive', () => {
    expect(toChordPro(pasted).startsWith('{verse: 1}')).toBe(true);
    expect(parseChart(toChordPro(pasted)).sections[0]!.kind).toBe('verse');
  });
});

describe('effective key', () => {
  // Business rule 7.
  it('resolves in order and names the level that supplied the key', () => {
    expect(effectiveKey({ setOverride: 'A', preferred: 'C', songOriginal: 'G' }).source).toBe('set');
    expect(effectiveKey({ preferred: 'C', arrangementDefault: 'E', songOriginal: 'G' }).source).toBe('preference');
    expect(effectiveKey({ arrangementDefault: 'E', songOriginal: 'G' }).source).toBe('arrangement');
    expect(formatKey(effectiveKey({ songOriginal: 'G' }).key!)).toBe('G');
    expect(effectiveKey({}).source).toBe('none');
  });

  it('ignores a level that holds something which is not a key', () => {
    expect(formatKey(effectiveKey({ preferred: 'not a key', songOriginal: 'F#m' }).key!)).toBe('F#m');
  });
});
