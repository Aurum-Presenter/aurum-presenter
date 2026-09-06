import { describe, expect, it } from 'vitest';
import { isAutoPinned, parseList } from './repository';

const now = new Date('2026-09-06T12:00:00Z');
const set = (scheduled_for: string | null, pinned = 0) => ({ scheduled_for, pinned });

describe('offline pinning', () => {
  // Business rule 5 and acceptance criterion 4.
  it('keeps a set inside the fourteen-day window without being asked', () => {
    expect(isAutoPinned(set('2026-09-06'), now)).toBe(true);
    expect(isAutoPinned(set('2026-09-19'), now)).toBe(true);
    expect(isAutoPinned(set('2026-09-25'), now)).toBe(false);
  });

  it("keeps last night's set until the band has got home", () => {
    expect(isAutoPinned(set('2026-09-05'), now)).toBe(true);
    expect(isAutoPinned(set('2026-08-30'), now)).toBe(false);
  });

  it('keeps an explicitly pinned set whatever its date', () => {
    expect(isAutoPinned(set('2020-01-01', 1), now)).toBe(true);
    expect(isAutoPinned(set(null, 1), now)).toBe(true);
  });

  it('does not pin a set with no date, which is why the editor warns about one', () => {
    expect(isAutoPinned(set(null), now)).toBe(false);
    expect(isAutoPinned(set('not a date'), now)).toBe(false);
  });
});

describe('assigned members', () => {
  it('reads a list and shrugs at anything else', () => {
    expect(parseList('["a","b"]')).toEqual(['a', 'b']);
    expect(parseList(null)).toEqual([]);
    expect(parseList('7')).toEqual([]);
  });
});
