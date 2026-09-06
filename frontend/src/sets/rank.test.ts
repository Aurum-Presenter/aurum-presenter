import { describe, expect, it } from 'vitest';
import { byRank, initialRanks, rankBetween, rankForMove } from './rank';

describe('fractional ranks', () => {
  it('produces a rank between its neighbours', () => {
    const a = rankBetween(null, null);
    const b = rankBetween(a, null);
    const middle = rankBetween(a, b);

    expect(a < middle).toBe(true);
    expect(middle < b).toBe(true);
  });

  it('never ends in the lowest digit, so there is always room to insert before it', () => {
    let rank = rankBetween(null, null);

    for (let index = 0; index < 200; index++) {
      rank = rankBetween(null, rank);
      expect(rank.endsWith('0')).toBe(false);
      expect(rank.length).toBeLessThan(210);
    }
  });

  it('keeps splitting the same gap without collapsing', () => {
    let low = rankBetween(null, null);
    const high = rankBetween(low, null);

    for (let index = 0; index < 200; index++) {
      const next = rankBetween(low, high);
      expect(low < next && next < high).toBe(true);
      low = next;
    }
  });

  it('refuses neighbours given in the wrong order', () => {
    expect(() => rankBetween('b', 'a')).toThrow();
  });

  // Acceptance criterion 1: dragging the last item to the top, on two devices, merges by
  // string order rather than by a renumbering that has to win.
  it('moves an item to the top without touching the others', () => {
    const ranks = initialRanks(5);
    const moved = rankForMove(ranks, 4, 0)!;

    expect(moved < ranks[0]!).toBe(true);
    expect(rankForMove(ranks, 2, 2)).toBeNull();
  });

  it('moves an item into the middle', () => {
    const ranks = initialRanks(5);
    const moved = rankForMove(ranks, 0, 3)!;
    const order = byRank([
      ...ranks.slice(1).map((rank, index) => ({ rank, id: index + 1 })),
      { rank: moved, id: 0 },
    ]);

    expect(order.map((row) => row.id)).toEqual([1, 2, 3, 0, 4]);
  });

  it('sorts by plain string order', () => {
    expect(byRank([{ rank: 'b' }, { rank: 'a' }, { rank: 'aV' }]).map((row) => row.rank))
      .toEqual(['a', 'aV', 'b']);
  });
});
