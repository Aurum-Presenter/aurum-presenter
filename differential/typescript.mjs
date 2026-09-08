/**
 * The other side of the differential harness: the TypeScript the Rust was ported from.
 *
 * Bundled by `run.mjs` with the frontend's own esbuild, then handed the same cases the Rust
 * example gets. The canonical shapes below are written out by hand to match
 * `crates/core/examples/differential.rs` — mirroring them rather than deriving them from either
 * implementation's types is the point: a shared misunderstanding cannot then hide a divergence.
 */
import { parseChart } from '../frontend/src/chart/chordpro';
import { parseKey, formatKey } from '../frontend/src/chart/notes';
import { renderChart, inlineText, overLyricsRows } from '../frontend/src/chart/render';
import { detectNotation, isChordLine, toChordPro } from '../frontend/src/chart/overLyrics';
import { buildSlides, textOf } from '../frontend/src/present/slides';
import { selectSheet, explain } from '../frontend/src/sheets/selection';
import { initialRanks, rankBetween, rankForMove } from '../frontend/src/sets/rank';
import { buildIndex, search } from '../frontend/src/library/search';
import { importFile } from '../frontend/src/library/importer';
import { isAutoPinned } from '../frontend/src/sets/repository';
import { generate } from './cases.mjs';

const linesOf = (line) => ({
  comment: line.comment,
  segments: line.segments.map((segment) => [segment.chord === null ? null : segment.chord.text, segment.lyric]),
});

const RULES = {
  chart({ body }) {
    const chart = parseChart(body ?? '');

    return {
      meta: [
        chart.meta.title, chart.meta.subtitle, chart.meta.artist, chart.meta.key,
        chart.meta.tempo, chart.meta.time, chart.meta.capo,
      ],
      sections: chart.sections.map((section) => ({
        kind: section.kind,
        label: section.label,
        lines: section.lines.map(linesOf),
      })),
      warnings: chart.warnings.map((warning) => [warning.line, warning.token]),
      error: chart.error === null ? null : [chart.error.line, chart.error.message],
    };
  },

  transpose({ body, from, to, capo, layout }) {
    const source = parseKey(from);
    const target = parseKey(to);

    if (source === null || target === null) {
      return { error: 'not a key' };
    }

    const rendered = renderChart(parseChart(body ?? ''), { source, target, capo: capo ?? 0, layout: layout ?? 'inline' });

    return {
      shape_key: formatKey(rendered.shapeKey),
      respelled: rendered.respelled,
      lines: rendered.sections.flatMap((section) => section.lines).map((line) => ({
        inline: inlineText(line),
        chords: overLyricsRows(line).chords,
        lyrics: overLyricsRows(line).lyrics,
        rendered: line.segments.map((segment) => segment.chord),
      })),
    };
  },

  over_lyrics({ text }) {
    return {
      notation: detectNotation(text ?? ''),
      chordpro: toChordPro(text ?? ''),
      chord_line: isChordLine(text ?? ''),
    };
  },

  slides({ items, font_size_vh, safe_area_pct }) {
    const snapshot = {
      setId: null,
      setName: 'Differential',
      takenAt: '',
      items: (items ?? []).map((item, index) => ({
        itemId: `item-${index}`,
        songId: null,
        title: item.title ?? 'Untitled',
        body: item.body ?? null,
        writtenKey: item.written_key ?? null,
        setKey: item.set_key ?? null,
        capo: item.capo ?? 0,
        itemType: item.item_type ?? null,
        content: item.content ?? null,
        note: null,
        sheetId: item.sheet_id ?? null,
        sheetPages: item.sheet_pages ?? null,
      })),
    };

    return buildSlides(snapshot, font_size_vh ?? 8, safe_area_pct ?? 5).map((slide) => ({
      id: slide.id,
      kind: slide.kind,
      label: slide.label,
      text: slide.text,
      page: slide.page,
      set_key: slide.setKey,
      capo: slide.capo,
      lines: slide.lines.map(textOf),
    }));
  },

  selection({ sheets, key, part }) {
    const rows = (sheets ?? []).map((sheet) => ({
      id: sheet.id,
      sheet_key: sheet.sheet_key ?? null,
      part: sheet.part ?? null,
      position: sheet.position ?? 0,
      deleted_at: sheet.deleted === true ? '2026-01-01T00:00:00.000Z' : null,
    }));

    const parsed = key === null || key === undefined ? null : parseKey(key);
    const found = selectSheet(rows, parsed, part ?? null);

    if (found === null) {
      return null;
    }

    // The Rust names its fallbacks in the enum's own spelling; map to it here rather than
    // renaming an enum to suit a test.
    const FALLBACKS = {
      exact: 'Exact', 'other-part': 'OtherPart', 'any-key': 'AnyKey',
      'nearest-key': 'NearestKey', first: 'First',
    };

    return { id: found.sheet.id, fallback: FALLBACKS[found.fallback], explain: explain(found, parsed, part ?? null) };
  },

  rank(input) {
    try {
      if (input.op === 'initial') {
        return initialRanks(input.count ?? 0);
      }

      if (input.op === 'move') {
        return rankForMove(input.ranks ?? [], input.from ?? 0, input.to ?? 0);
      }

      return rankBetween(input.before ?? null, input.after ?? null);
    } catch {
      return { error: true };
    }
  },

  search({ songs, query, limit }) {
    const index = buildIndex((songs ?? []).map((song) => ({
      id: song.id,
      title: song.title ?? '',
      altTitles: song.alt_titles ?? [],
      artist: song.artist ?? null,
      tags: song.tags ?? [],
      lyrics: song.lyrics ?? '',
    })));

    return search(index, query ?? '', limit ?? 50)
      .map((hit) => [hit.id, Math.round(hit.score * 1e6) / 1e6, hit.field]);
  },

  importer({ filename, text }) {
    const result = importFile(filename ?? '', text ?? '');

    return {
      error: result.error,
      song: result.song === null ? null : {
        title: result.song.title,
        artist: result.song.artist,
        original_key: result.song.original_key,
        tempo: result.song.tempo,
        time_signature: result.song.time_signature,
        body: result.song.body,
        notation: result.song.sourceNotation,
        source_text: result.song.sourceText,
      },
    };
  },


  time({ text, epoch_ms }) {
    const parsed = Date.parse(text ?? '');

    return {
      parsed: Number.isNaN(parsed) ? null : parsed,
      formatted: epoch_ms === undefined || epoch_ms === null ? null : new Date(epoch_ms).toISOString(),
    };
  },

  pins({ pinned, scheduled_for, now }) {
    return {
      auto_pinned: isAutoPinned(
        { pinned: pinned === true ? 1 : 0, scheduled_for: scheduled_for ?? null },
        new Date(now),
      ),
    };
  },
};

// `generate` needs a working rankBetween to produce ranks that are genuinely in order; it is
// only ever used to build inputs, never as an answer either side is checked against.
// Written and then returned from, never `process.exit`: a megabyte of JSON on a pipe is still
// in flight when exit() runs, and the parent would parse half a corpus.
if (process.argv[2] === 'generate') {
  process.stdout.write(JSON.stringify(
    generate(Number(process.argv[3]), Number(process.argv[4]), rankBetween),
  ));
} else {

  const cases = JSON.parse(await new Promise((resolve) => {
    let text = '';
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', (chunk) => { text += chunk; });
    process.stdin.on('end', () => resolve(text));
  }));

  const results = cases.map((entry) => {
    const rule = RULES[entry.rule];

    if (rule === undefined) {
      return { unknown_rule: entry.rule };
    }

    try {
      return rule(entry.input) ?? null;
    } catch (error) {
      return { threw: String(error && error.message ? error.message : error) };
    }
  });

  process.stdout.write(JSON.stringify(results));
}
