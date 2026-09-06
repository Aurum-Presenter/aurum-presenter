import { useEffect, useRef, useState } from 'react';
import type { Chart } from '../chart/chordpro';
import { parseKey, type Key } from '../chart/notes';
import { overLyricsRows, renderChart } from '../chart/render';
import type { Theme } from './session';
import { textOf, type Slide } from './slides';

/**
 * Drawing a slide.
 *
 * The audience and the stage render the same slide with different rules: the audience gets
 * lyrics, large, on the theme's background; the stage keeps the chords and puts them in the
 * reader's own key. Neither holds any logic about what happens next — they are functions of the
 * state they were handed.
 */

export function AudienceSlide({ slide, theme, workspaceId }: { slide: Slide | null; theme: Theme; workspaceId: string }) {
  const lines = slide === null ? [] : slide.kind === 'text' ? (slide.text ?? '').split('\n') : slide.lines.map(textOf);
  const fitted = useFittedSize(theme.font_size_vh, lines);

  if (slide?.kind === 'sheet') {
    return <SheetSlide slide={slide} workspaceId={workspaceId} />;
  }

  return (
    <div
      className="flex h-full w-full flex-col items-center justify-center"
      style={{ padding: `${theme.safe_area_pct}vh ${theme.safe_area_pct}vw` }}
    >
      {theme.show_section_labels && slide?.label != null && (
        <p className="mb-4 uppercase tracking-widest opacity-60" style={{ fontSize: `${fitted / 3}vh` }}>
          {slide.label}
        </p>
      )}

      <div
        className={`w-full ${theme.align === 'center' ? 'text-center' : 'text-left'}`}
        style={{ fontSize: `${fitted}vh`, lineHeight: 1.25, fontFamily: theme.font_family }}
      >
        {lines.map((line, index) => (
          <p key={index} className="whitespace-pre-wrap">{line === '' ? ' ' : line}</p>
        ))}
      </div>
    </div>
  );
}

/**
 * Stage-view business rule 7: chords in the reader's own key, unless the set fixed one for
 * everybody.
 */
export function StageSlide({
  slide,
  targetKey,
  showChords,
  scale,
}: {
  slide: Slide | null;
  targetKey: Key | null;
  showChords: boolean;
  scale: number;
}) {
  if (slide === null) {
    return <p className="opacity-60">Waiting for the first slide…</p>;
  }

  if (slide.kind === 'text') {
    return <p className="whitespace-pre-wrap" style={{ fontSize: `${scale}vh` }}>{slide.text}</p>;
  }

  if (slide.kind === 'blank' || slide.kind === 'sheet' || slide.kind === 'title') {
    return <p className="opacity-60" style={{ fontSize: `${scale / 2}vh` }}>{slide.songTitle}</p>;
  }

  const written = parseKey(slide.writtenKey ?? 'C') ?? parseKey('C')!;
  const target = parseKey(slide.setKey ?? '') ?? targetKey ?? written;

  const chart: Chart = {
    meta: { title: null, subtitle: null, artist: null, key: null, tempo: null, time: null, capo: null },
    sections: [{ kind: 'none', label: null, lines: slide.lines }],
    warnings: [],
    error: null,
  };

  const rendered = renderChart(chart, { source: written, target, capo: slide.capo, layout: 'over' });

  return (
    <div style={{ fontSize: `${scale}vh` }}>
      {rendered.sections[0]!.lines.map((line, index) => {
        const rows = overLyricsRows(line);

        return (
          <div key={index} className="font-mono leading-tight">
            {showChords && (
              <div className="whitespace-pre font-semibold text-sky-300">{rows.chords === '' ? ' ' : rows.chords}</div>
            )}
            <div className="whitespace-pre">{rows.lyrics === '' ? ' ' : rows.lyrics}</div>
          </div>
        );
      })}
    </div>
  );
}

/**
 * A sheet page on the audience screen, rendered from the file this device already holds.
 *
 * The output window is the same origin as the control surface, so it opens the same local
 * database and reads the same cached file. Nothing is fetched: if the file is not on the device
 * the slide says which page it would have been, rather than showing a broken image to a room.
 */
function SheetSlide({ slide, workspaceId }: { slide: Slide; workspaceId: string }) {
  const [image, setImage] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      const [{ WorkspaceDb }, { BlobStore }, { renderPdfToImages }] = await Promise.all([
        import('../db/schema'),
        import('../blobs/store'),
        import('../sheets/render'),
      ]);

      const db = new WorkspaceDb(workspaceId);
      const file = await new BlobStore(db, workspaceId).get(slide.sheetId!);
      const pages = file === null ? [] : await renderPdfToImages(file, 1920);

      if (! cancelled) {
        setImage(pages[(slide.page ?? 1) - 1] ?? null);
      }

      db.close();
    })();

    return () => { cancelled = true; };
  }, [slide.sheetId, slide.page, workspaceId]);

  return (
    <div className="flex h-full w-full items-center justify-center">
      {image === null
        ? <p className="opacity-60">{slide.songTitle} — page {slide.page}</p>
        : <img src={image} alt="" className="max-h-full max-w-full" />}
    </div>
  );
}

/**
 * Fitting text rather than letting it overflow: a long verse comes down in size until it fits,
 * which is what the slide-splitting rules leave for the renderer to finish.
 */
function useFittedSize(maximum: number, lines: string[]): number {
  const longest = useRef(0);
  longest.current = lines.reduce((most, line) => Math.max(most, line.length), 0);

  const byCount = lines.length === 0 ? maximum : Math.min(maximum, 78 / lines.length);
  const byWidth = longest.current === 0 ? maximum : Math.min(maximum, 170 / longest.current);

  return Math.max(2.5, Math.min(byCount, byWidth));
}
