/**
 * The corpus both implementations are run over.
 *
 * Deterministic: the same seed produces the same cases, so a divergence found in CI can be
 * reproduced exactly. Random cases go where a hand-written test cannot reach — every chord
 * spelling in every key, ranks split two hundred deep, charts assembled out of the shapes that
 * break parsers — and the fixtures go alongside them because real songs are stranger than
 * anything a generator invents.
 */
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

/** mulberry32: small, fast, and identical run to run. */
function random(seed) {
  let state = seed >>> 0;

  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;

    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const LETTERS = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];
const ACCIDENTALS = ['', '#', 'b', '##', 'bb', '♯', '♭'];
const QUALITIES = ['', 'm', 'maj7', 'm7', '7', 'sus4', 'sus2', 'add9', 'dim', 'aug', 'm7b5', '6/9', 'maj9#11', '13', 'alt'];
const KEYS = ['C', 'G', 'D', 'A', 'E', 'B', 'F#', 'Db', 'Ab', 'Eb', 'Bb', 'F', 'Am', 'Em', 'Bm', 'F#m', 'C#m', 'Dm', 'Gm', 'Cm', 'Fm', 'Bbm'];
const LAYOUTS = ['inline', 'over', 'nashville'];
const WORDS = ['amazing', 'grace', 'how', 'sweet', 'the', 'sound', 'that', 'saved', 'a', 'wretch', 'like', 'me', 'señor', 'grüßen', "don't"];
const DIRECTIVES = ['{verse: 1}', '{chorus}', '{bridge}', '{comment: build}', '{title: A Song}', '{artist: Someone}', '{key: G}', '{tempo: 72}', '{x_unknown: 3}', '{eoc}'];
const NOISE = ['[', ']', '[]', '[N.C.]', '[Hmm]', '#a comment', '', '   ', '[C', 'x]'];
const PARTS = ['lead', 'piano', 'vocal', 'guitar', 'bass', 'lyrics', 'other', null];

const pick = (rng, list) => list[Math.floor(rng() * list.length)];
const times = (rng, max, make) => Array.from({ length: Math.floor(rng() * max) }, make);

const chordToken = (rng) =>
  pick(rng, LETTERS) + pick(rng, ACCIDENTALS) + pick(rng, QUALITIES) +
  (rng() < 0.2 ? '/' + pick(rng, LETTERS) + pick(rng, ACCIDENTALS) : '');

function body(rng) {
  const lines = times(rng, 12, () => {
    const roll = rng();

    if (roll < 0.15) {
      return pick(rng, DIRECTIVES);
    }

    if (roll < 0.25) {
      return pick(rng, NOISE);
    }

    return times(rng, 6, () =>
      (rng() < 0.6 ? `[${chordToken(rng)}]` : '') + pick(rng, WORDS)).join(' ');
  });

  return lines.join('\n');
}

function overLyricsText(rng) {
  return times(rng, 8, () => {
    const roll = rng();

    if (roll < 0.2) {
      return pick(rng, ['Verse 1', '[Chorus]', 'Pre-Chorus:', 'Bridge b', 'Instrumental']);
    }

    if (roll < 0.55) {
      // A chord line: tokens at arbitrary columns, which is the thing that has to round-trip.
      let line = '';

      for (const chord of times(rng, 5, () => chordToken(rng))) {
        line = line.padEnd(line.length + Math.floor(rng() * 9), ' ') + chord;
      }

      return line;
    }

    return times(rng, 8, () => pick(rng, WORDS)).join(' ');
  }).join('\n');
}

function corpus(rng) {
  return times(rng, 6, (_, index) => ({
    id: `song-${index}`,
    title: times(rng, 4, () => pick(rng, WORDS)).join(' '),
    alt_titles: times(rng, 2, () => pick(rng, WORDS)),
    artist: rng() < 0.7 ? pick(rng, WORDS) : null,
    tags: times(rng, 3, () => pick(rng, WORDS)),
    lyrics: times(rng, 20, () => pick(rng, WORDS)).join(' '),
  })).map((song, index) => ({ ...song, id: `song-${index}` }));
}

function sheets(rng) {
  return times(rng, 6, (_, index) => ({
    id: `sheet-${index}`,
    sheet_key: rng() < 0.8 ? pick(rng, KEYS) : null,
    part: pick(rng, PARTS),
    position: Math.floor(rng() * 10),
    deleted: rng() < 0.15,
  })).map((sheet, index) => ({ ...sheet, id: `sheet-${index}` }));
}

/** Ranks that are actually in order, produced the way the app produces them. */
function rankPair(rng, between) {
  let low = between(null, null);
  let high = between(low, null);

  for (let depth = Math.floor(rng() * 30); depth > 0; depth--) {
    const next = between(low, high);

    if (rng() < 0.5) {
      low = next;
    } else {
      high = next;
    }
  }

  return [low, high];
}

function fixtures() {
  const roots = ['e2e/fixtures', 'frontend/src/library/fixtures'];
  const found = [];

  for (const root of roots) {
    let names = [];

    try {
      names = readdirSync(root);
    } catch {
      continue;
    }

    for (const name of names) {
      if (/\.(cho|chopro|chordpro|txt|pro)$/i.test(name)) {
        found.push({ filename: name, text: readFileSync(join(root, name), 'utf8') });
      }
    }
  }

  return found;
}

/** Scalars only: the client JSON-encodes its list fields before pushing them, as strings. */
function scalar(rng) {
  const roll = rng();

  if (roll < 0.15) return null;
  if (roll < 0.3) return Math.floor(rng() * 300);
  if (roll < 0.4) return rng() < 0.5;
  if (roll < 0.5) return '';

  return pick(rng, WORDS);
}

export function generate(seed, count, between) {
  const rng = random(seed);
  const cases = [];
  const add = (rule, input) => cases.push({ rule, input });

  for (const file of fixtures()) {
    add('chart', { body: file.text });
    add('importer', file);
    add('over_lyrics', { text: file.text });
  }

  for (let index = 0; index < count; index++) {
    const chart = body(rng);

    add('chart', { body: chart });
    add('transpose', {
      body: chart,
      from: pick(rng, KEYS),
      to: pick(rng, KEYS),
      capo: Math.floor(rng() * 12),
      layout: pick(rng, LAYOUTS),
    });
    add('over_lyrics', { text: overLyricsText(rng) });
    add('importer', { filename: `${Math.floor(rng() * 99)} - ${pick(rng, WORDS)}.cho`, text: chart });
    add('slides', {
      items: times(rng, 4, () => ({
        title: pick(rng, WORDS),
        body: rng() < 0.7 ? body(rng) : null,
        written_key: pick(rng, KEYS),
        set_key: rng() < 0.4 ? pick(rng, KEYS) : null,
        capo: Math.floor(rng() * 5),
        item_type: pick(rng, ['blank', 'announcement', 'scripture', null]),
        content: rng() < 0.4 ? times(rng, 8, () => pick(rng, WORDS)).join('\n') : null,
        sheet_id: rng() < 0.2 ? 'sheet-1' : null,
        sheet_pages: Math.floor(rng() * 4),
      })),
      font_size_vh: 2 + Math.floor(rng() * 30),
      safe_area_pct: Math.floor(rng() * 20),
    });
    add('selection', {
      sheets: sheets(rng),
      key: rng() < 0.9 ? pick(rng, KEYS) : null,
      part: pick(rng, PARTS),
    });

    const [low, high] = rankPair(rng, between);
    add('rank', { op: 'between', before: rng() < 0.1 ? null : low, after: rng() < 0.1 ? null : high });
    add('rank', { op: 'initial', count: Math.floor(rng() * 40) });

    const list = between === null ? [] : Array.from({ length: 5 }, (_, i) => i).reduce((ranks) => [...ranks, between(ranks.at(-1) ?? null, null)], []);
    add('rank', { op: 'move', ranks: list, from: Math.floor(rng() * list.length), to: Math.floor(rng() * list.length) });

    add('search', {
      songs: corpus(rng),
      query: times(rng, 3, () => pick(rng, WORDS).slice(0, 1 + Math.floor(rng() * 6))).join(' '),
      limit: 1 + Math.floor(rng() * 50),
    });
    add('time', {
      text: pick(rng, ['2026-09-08', '2026-09-08T14:30:00.123Z', '2026-09-08T14:30:00Z', 'nonsense', '', '1999-12-31T23:59:59.999Z']),
      epoch_ms: Math.floor(rng() * 4_000_000_000_000),
    });
    add('pins', {
      pinned: rng() < 0.3,
      scheduled_for: rng() < 0.9 ? `2026-0${1 + Math.floor(rng() * 9)}-${10 + Math.floor(rng() * 19)}` : null,
      now: '2026-09-06T12:00:00.000Z',
    });
  }

  return cases;
}
