/**
 * What this device has been told not to keep.
 *
 * Releasing is a decision about *this* device — a laptop with a full disk is not a reason for
 * the band's phones to stop holding Sunday's set — so it is local and never synced. It beats
 * every reason a file would otherwise be kept: a pinned set, a set that is coming up, a pinned
 * song. Nothing is deleted by releasing; the files simply stop being protected and the eviction
 * pass may reclaim them.
 */
const KEY = 'aurum.released';

export interface Released {
  sets: string[];
  songs: string[];
}

const EMPTY: Released = { sets: [], songs: [] };

export function released(workspaceId: string): Released {
  try {
    const stored = localStorage.getItem(`${KEY}.${workspaceId}`);

    return stored === null ? EMPTY : { ...EMPTY, ...(JSON.parse(stored) as Partial<Released>) };
  } catch {
    return EMPTY;
  }
}

export function release(workspaceId: string, kind: 'set' | 'song', id: string): void {
  const current = released(workspaceId);

  write(workspaceId, kind === 'set'
    ? { ...current, sets: [...new Set([...current.sets, id])] }
    : { ...current, songs: [...new Set([...current.songs, id])] });
}

export function keepAgain(workspaceId: string, kind: 'set' | 'song', id: string): void {
  const current = released(workspaceId);

  write(workspaceId, kind === 'set'
    ? { ...current, sets: current.sets.filter((each) => each !== id) }
    : { ...current, songs: current.songs.filter((each) => each !== id) });
}

function write(workspaceId: string, value: Released): void {
  try {
    localStorage.setItem(`${KEY}.${workspaceId}`, JSON.stringify(value));
  } catch {
    // No storage means no release list; the pin policy simply keeps what it would have kept.
  }
}
