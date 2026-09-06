import { describe, expect, it } from 'vitest';
import { parseKey } from '../chart/notes';
import type { Sheet } from '../db/schema';
import { fifthsDistance, selectSheet } from './selection';

const key = (text: string) => parseKey(text)!;

function sheet(id: string, sheetKey: string | null, part: string | null, position = 0): Sheet {
  return {
    id, song_id: 'song', sheet_key: sheetKey, part, position,
    sha256: null, size: null, page_count: null, mime_type: 'application/pdf', uploaded_at: null,
    arrangement_id: null, label: null, filename: null, pages_changed_at: null,
    updated_at: '2026-09-06T00:00:00Z', change_seq: 1, deleted_at: null, updated_by: null,
  };
}

describe('sheet selection', () => {
  // Business rule 4, in order.
  it("prefers the exact key and the reader's part", () => {
    const sheets = [sheet('a', 'G', 'piano'), sheet('b', 'G', 'lead'), sheet('c', 'Bb', 'piano')];

    expect(selectSheet(sheets, key('G'), 'piano')).toMatchObject({ sheet: { id: 'a' }, fallback: 'exact' });
  });

  it('falls back to another part in the same key before another key', () => {
    const sheets = [sheet('b', 'G', 'lead'), sheet('c', 'Bb', 'piano')];

    expect(selectSheet(sheets, key('G'), 'piano')).toMatchObject({ sheet: { id: 'b' }, fallback: 'other-part' });
  });

  it('uses a keyless sheet before a sheet in the wrong key', () => {
    const sheets = [sheet('lyrics', null, 'piano'), sheet('c', 'Bb', 'piano')];

    expect(selectSheet(sheets, key('G'), 'piano')).toMatchObject({ sheet: { id: 'lyrics' }, fallback: 'any-key' });
  });

  it('falls back to the nearest key around the circle of fifths', () => {
    const sheets = [sheet('far', 'B', 'piano'), sheet('near', 'D', 'piano')];

    expect(selectSheet(sheets, key('G'), 'piano')).toMatchObject({ sheet: { id: 'near' }, fallback: 'nearest-key' });
  });

  it('shows something rather than nothing', () => {
    // Wrong key and wrong part: still shown, and the banner says so.
    expect(selectSheet([sheet('only', 'Eb', 'bass', 3)], key('G'), 'piano'))
      .toMatchObject({ sheet: { id: 'only' }, fallback: 'first' });
    expect(selectSheet([], key('G'), 'piano')).toBeNull();
  });

  it('ignores deleted sheets', () => {
    const gone = { ...sheet('gone', 'G', 'piano'), deleted_at: '2026-09-06T00:00:00Z' };

    expect(selectSheet([gone, sheet('b', 'Bb', 'piano')], key('G'), 'piano')?.sheet.id).toBe('b');
  });
});

describe('circle of fifths', () => {
  it('measures distance in fifths, not semitones', () => {
    expect(fifthsDistance(key('C'), key('G'))).toBe(1);
    expect(fifthsDistance(key('C'), key('F'))).toBe(1);
    expect(fifthsDistance(key('C'), key('D'))).toBe(2);
    expect(fifthsDistance(key('C'), key('C#'))).toBe(5);
    expect(fifthsDistance(key('C'), key('F#'))).toBe(6);
    expect(fifthsDistance(key('G'), key('G'))).toBe(0);
  });
});
