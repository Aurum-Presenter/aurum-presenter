import { describe, expect, it } from 'vitest';
import { buildIndex, lyricsOf, search, tokenise } from './search';

const songs = [
  { id: 'a', title: 'Amazing Grace', altTitles: [], artist: 'John Newton', tags: ['hymn'], lyrics: 'how sweet the sound' },
  { id: 'b', title: 'Great Is Thy Faithfulness', altTitles: ['Morning by Morning'], artist: null, tags: ['hymn', 'slow'], lyrics: 'morning by morning new mercies I see' },
  { id: 'c', title: 'Graceful Days', altTitles: [], artist: 'The Sound', tags: [], lyrics: 'nothing to do with grace at all' },
];

const index = buildIndex(songs);

describe('search', () => {
  it('ranks a title match above a lyric match', () => {
    const hits = search(index, 'grace');

    expect(hits[0]!.id).toBe('a');
    expect(hits[0]!.field).toBe('title');
    expect(hits.map((hit) => hit.id)).toContain('c');
  });

  it('prefers an exact word to a longer word it prefixes', () => {
    const [first, second] = search(index, 'grace');

    expect(first!.id).toBe('a');
    expect(second!.score).toBeLessThan(first!.score);
  });

  // Acceptance criterion 1: three letters is enough.
  it('matches on a prefix of three letters', () => {
    expect(search(index, 'ama').map((hit) => hit.id)).toEqual(['a']);
  });

  it('narrows rather than widens with a second word', () => {
    expect(search(index, 'morning mercies').map((hit) => hit.id)).toEqual(['b']);
    expect(search(index, 'morning nothing')).toEqual([]);
  });

  it('finds a song by alternate title, artist and tag', () => {
    expect(search(index, 'morning by')[0]!.id).toBe('b');
    expect(search(index, 'newton')[0]!.id).toBe('a');
    expect(search(index, 'slow')[0]!.id).toBe('b');
  });

  it('folds accents so a keyboard without them still finds the song', () => {
    const accented = buildIndex([{ id: 'x', title: 'Señor', altTitles: [], artist: null, tags: [], lyrics: '' }]);

    expect(search(accented, 'senor')[0]!.id).toBe('x');
    expect(tokenise('Señor, Ven')).toEqual(['senor', 'ven']);
  });

  it('returns nothing for an empty query rather than everything', () => {
    expect(search(index, '   ')).toEqual([]);
  });

  // Acceptance criterion 2: a thousand songs, ranked, well inside 100 ms.
  it('searches a thousand songs quickly', () => {
    const many = Array.from({ length: 1000 }, (_, number) => ({
      id: `song-${number}`,
      title: `Song number ${number} of the library`,
      altTitles: [`Alternate ${number}`],
      artist: 'Some Artist',
      tags: ['tag'],
      lyrics: 'verse one line one verse two line two chorus line '.repeat(8) + `unique${number}`,
    }));

    const large = buildIndex(many);
    const started = performance.now();
    const hits = search(large, 'unique512');

    expect(performance.now() - started).toBeLessThan(100);
    expect(hits[0]!.id).toBe('song-512');
  });
});

describe('lyrics extraction', () => {
  it('drops chords and directives', () => {
    expect(lyricsOf('{verse: 1}\n[G]Amazing [D/F#]grace').replace(/\s+/g, ' ').trim())
      .toBe('Amazing grace');
  });
});
