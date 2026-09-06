/**
 * Fractional ranks for set items (business rule 1).
 *
 * An integer position means that inserting one item renumbers every item after it, and two
 * people reordering offline produce two conflicting renumberings of the same rows. A rank is a
 * string between its neighbours instead: inserting touches exactly one row, and a merge is
 * decided by string order rather than by who pushed last.
 *
 * The one invariant that makes this total: a rank never ends in the lowest digit, so there is
 * always room to insert before it.
 */

const DIGITS = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';
const BASE = DIGITS.length;

/** `null` on either side means "before everything" or "after everything". */
export function rankBetween(before: string | null, after: string | null): string {
  const low = before ?? '';
  const high = after ?? '';

  if (low !== '' && high !== '' && low >= high) {
    throw new Error(`Ranks out of order: ${low} is not before ${high}`);
  }

  let rank = '';

  // While the rank still matches `high` digit for digit, the upper bound is `high`'s next
  // digit. The moment it drops below one, every later digit is free — "0z" is below "1"
  // whatever follows it.
  let bounded = high !== '';

  for (let index = 0; ; index++) {
    const from = index < low.length ? digit(low[index]!) : 0;

    if (bounded && index >= high.length) {
      throw new Error(`No rank fits between ${low} and ${high}`);
    }

    const to = bounded ? digit(high[index]!) : BASE;

    if (from + 1 < to) {
      // A midpoint exists here. It is never the lowest digit, which is what keeps the next
      // insertion before it possible.
      return rank + DIGITS[Math.floor((from + to) / 2)]!;
    }

    rank += DIGITS[from]!;
    bounded = bounded && from === to;
  }
}

/** Ranks for a fresh list, evenly spread so the first few inserts stay short. */
export function initialRanks(count: number): string[] {
  const ranks: string[] = [];
  let previous: string | null = null;

  for (let index = 0; index < count; index++) {
    previous = rankBetween(previous, null);
    ranks.push(previous);
  }

  return ranks;
}

/**
 * The rank an item needs to land at `target` in the current order, having been removed from
 * wherever it was. Returns null when the move is a no-op.
 */
export function rankForMove(ranks: string[], from: number, to: number): string | null {
  if (from === to) {
    return null;
  }

  const without = ranks.filter((_, index) => index !== from);
  const before = to === 0 ? null : without[to - 1] ?? null;
  const after = without[to] ?? null;

  return rankBetween(before, after);
}

function digit(character: string): number {
  const index = DIGITS.indexOf(character);

  if (index === -1) {
    throw new Error(`"${character}" is not a rank digit`);
  }

  return index;
}

/** Sorts by rank, which is plain string order — that is the whole point of the scheme. */
export function byRank<T extends { rank: string }>(rows: T[]): T[] {
  return [...rows].sort((a, b) => (a.rank < b.rank ? -1 : a.rank > b.rank ? 1 : 0));
}
