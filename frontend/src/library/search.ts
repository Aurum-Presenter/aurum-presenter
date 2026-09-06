/**
 * The local search index.
 *
 * Search is client-side and offline by rule: a musician looking for a song on a dark stage
 * cannot wait for a round trip, and often has no signal at all. The index is an inverted map
 * from term to song, built from the fields a band actually searches by, and rebuilt
 * incrementally as songs change.
 */

export interface IndexedSong {
  id: string;
  title: string;
  altTitles: string[];
  artist: string | null;
  tags: string[];
  /** Plain lyrics extracted from the chart — chords and directives already stripped. */
  lyrics: string;
}

export interface SearchHit {
  id: string;
  score: number;
  /** Which field carried the best match, so the list can say why a song is in the results. */
  field: Field;
}

type Field = 'title' | 'alt' | 'artist' | 'tag' | 'lyrics';

/** A title match must beat a lyric match by more than a lyric match can accumulate. */
const WEIGHTS: Record<Field, number> = { title: 100, alt: 60, artist: 40, tag: 30, lyrics: 1 };

interface Posting {
  song: number;
  field: Field;
}

export interface SearchIndex {
  ids: string[];
  /** Sorted for binary search: a query term matches every index term that starts with it. */
  terms: string[];
  postings: Posting[][];
}

export const EMPTY_INDEX: SearchIndex = { ids: [], terms: [], postings: [] };

/** Lower-cased word tokens. Accents are folded so "Señor" is found by typing "senor". */
export function tokenise(text: string): string[] {
  return text
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .split(/[^a-z0-9']+/)
    .filter((token) => token !== '');
}

export function buildIndex(songs: IndexedSong[]): SearchIndex {
  const byTerm = new Map<string, Posting[]>();

  songs.forEach((song, index) => {
    const add = (text: string, field: Field): void => {
      for (const term of tokenise(text)) {
        const postings = byTerm.get(term);

        if (postings === undefined) {
          byTerm.set(term, [{ song: index, field }]);
          continue;
        }

        // One posting per (song, field): repeating a word in a chorus must not outrank a title.
        if (! postings.some((posting) => posting.song === index && posting.field === field)) {
          postings.push({ song: index, field });
        }
      }
    };

    add(song.title, 'title');
    song.altTitles.forEach((alt) => add(alt, 'alt'));
    add(song.artist ?? '', 'artist');
    song.tags.forEach((tag) => add(tag, 'tag'));
    add(song.lyrics, 'lyrics');
  });

  const terms = [...byTerm.keys()].sort();

  return {
    ids: songs.map((song) => song.id),
    terms,
    postings: terms.map((term) => byTerm.get(term)!),
  };
}

/**
 * Ranked results for a query. Every query term must match something (AND), which is what makes
 * a two-word query narrow rather than widen — the behaviour a person expects from a search box.
 */
export function search(index: SearchIndex, query: string, limit = 50): SearchHit[] {
  const terms = tokenise(query);

  if (terms.length === 0) {
    return [];
  }

  let running: Map<number, { score: number; field: Field }> | null = null;

  for (const term of terms) {
    const found = new Map<number, { score: number; field: Field }>();

    for (const position of prefixRange(index.terms, term)) {
      const term1 = index.terms[position]!;
      // An exact word scores above a prefix of a longer word: "grace" over "graceful".
      const closeness = term.length / term1.length;

      for (const posting of index.postings[position]!) {
        const score = WEIGHTS[posting.field] * closeness;
        const previous = found.get(posting.song);

        if (previous === undefined || score > previous.score) {
          found.set(posting.song, { score, field: posting.field });
        }
      }
    }

    if (running === null) {
      running = found;
      continue;
    }

    for (const [song, current] of running) {
      const next = found.get(song);

      if (next === undefined) {
        running.delete(song);
      } else {
        running.set(song, {
          score: current.score + next.score,
          field: next.score > current.score ? next.field : current.field,
        });
      }
    }
  }

  return [...(running ?? new Map())]
    .map(([song, hit]) => ({ id: index.ids[song]!, score: hit.score, field: hit.field }))
    .sort((a, b) => b.score - a.score)
    .slice(0, limit);
}

/** Indices of every term starting with the prefix, found by binary search. */
function prefixRange(terms: string[], prefix: string): number[] {
  let low = 0;
  let high = terms.length;

  while (low < high) {
    const middle = (low + high) >> 1;

    if (terms[middle]! < prefix) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }

  const positions: number[] = [];

  for (let index = low; index < terms.length && terms[index]!.startsWith(prefix); index++) {
    positions.push(index);
  }

  return positions;
}

/**
 * Lyrics as a searcher thinks of them: no chords, no directives, no bracket noise. Cheap enough
 * to run over a whole library, which is what the index build does.
 */
export function lyricsOf(chordPro: string): string {
  return chordPro
    .replace(/\{[^}]*\}/g, ' ')
    .replace(/\[[^\]]*\]/g, '')
    .replace(/^#.*$/gm, ' ');
}
