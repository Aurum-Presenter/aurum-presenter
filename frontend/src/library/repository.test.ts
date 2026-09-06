import { describe, expect, it } from 'vitest';
import { formatDuration, listOf, parseDuration, songsInFolder, validateSong, wouldCycle } from './repository';

const folders = [
  { id: 'root', parent_id: null },
  { id: 'child', parent_id: 'root' },
  { id: 'grandchild', parent_id: 'child' },
  { id: 'other', parent_id: null },
];

describe('folder moves', () => {
  // Acceptance criterion 3: dropping a folder into its own subtree is refused.
  it('refuses a move into its own descendant, or onto itself', () => {
    expect(wouldCycle(folders, 'root', 'grandchild')).toBe(true);
    expect(wouldCycle(folders, 'root', 'child')).toBe(true);
    expect(wouldCycle(folders, 'root', 'root')).toBe(true);
  });

  it('allows every move that does not close a loop', () => {
    expect(wouldCycle(folders, 'grandchild', 'other')).toBe(false);
    expect(wouldCycle(folders, 'child', null)).toBe(false);
  });

  /** A tree that already holds a cycle — two devices re-parenting at once — must not hang. */
  it('terminates on an already-looped tree', () => {
    const looped = [
      { id: 'a', parent_id: 'b' },
      { id: 'b', parent_id: 'a' },
      { id: 'c', parent_id: null },
    ];

    expect(wouldCycle(looped, 'c', 'a')).toBe(false);
  });
});

describe('song validation', () => {
  it('requires a title and sensible numbers', () => {
    expect(validateSong({ title: '  ' })).toContain('A title is required.');
    expect(validateSong({ title: 'x'.repeat(201) })).toContain('A title is at most 200 characters.');
    expect(validateSong({ title: 'Song', tempo: 12 })).toContain('Tempo is between 20 and 300 bpm.');
    expect(validateSong({ title: 'Song', ccli_number: 'ABC' })).toContain('A CCLI number is digits only.');
    expect(validateSong({ title: 'Song', tempo: 120, ccli_number: '123456' })).toEqual([]);
  });

  it('reads and writes a length as mm:ss', () => {
    expect(parseDuration('4:05')).toBe(245);
    expect(parseDuration('0:59')).toBe(59);
    expect(parseDuration('4:60')).toBeNull();
    expect(parseDuration('nonsense')).toBeNull();
    expect(formatDuration(245)).toBe('4:05');
    expect(formatDuration(null)).toBe('');
  });

  it('survives a tags column that is not an array', () => {
    expect(listOf('["hymn","slow"]')).toEqual(['hymn', 'slow']);
    expect(listOf('not json')).toEqual([]);
    expect(listOf(null)).toEqual([]);
    expect(listOf('{"a":1}')).toEqual([]);
  });
});

describe('folder contents', () => {
  const songs = [
    { id: 'one', folder_id: 'hymns' },
    { id: 'two', folder_id: null },
    { id: 'three', folder_id: 'modern' },
  ] as Parameters<typeof songsInFolder>[0];

  it('includes songs placed in a second folder as well as those filed there', () => {
    const placements = [
      { id: 'p', song_id: 'three', folder_id: 'hymns', deleted_at: null },
    ] as Parameters<typeof songsInFolder>[1];

    expect(songsInFolder(songs, placements, 'hymns').map((song) => song.id)).toEqual(['one', 'three']);
    expect(songsInFolder(songs, placements, null).map((song) => song.id)).toEqual(['two']);
  });

  it('ignores a placement that has been removed', () => {
    const placements = [
      { id: 'p', song_id: 'three', folder_id: 'hymns', deleted_at: '2026-09-06T00:00:00Z' },
    ] as Parameters<typeof songsInFolder>[1];

    expect(songsInFolder(songs, placements, 'hymns').map((song) => song.id)).toEqual(['one']);
  });
});
