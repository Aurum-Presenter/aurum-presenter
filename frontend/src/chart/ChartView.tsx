import { useEffect, useMemo } from 'react';
import { parseCached, parseChart, type Chart } from './chordpro';
import { overLyricsRows, renderChart, type Layout, type RenderedLine, type RenderedSection } from './render';
import type { Key } from './notes';

/**
 * The reading surface. Everything here is derived: the stored ChordPro is parsed once, and a key
 * change re-renders from the model rather than rewriting anything.
 */

export interface DisplayPrefs {
  layout: Layout;
  fontSize: number;
  columns: 1 | 2;
  /** Chords-only and lyrics-only are the two modes a player asks for mid-rehearsal. */
  mode: 'both' | 'chords' | 'lyrics';
  showSections: boolean;
}

export const DEFAULT_DISPLAY: DisplayPrefs = {
  layout: 'over',
  fontSize: 16,
  columns: 1,
  mode: 'both',
  showSections: true,
};

const SECTION_LABELS: Record<RenderedSection['kind'], string> = {
  verse: 'Verse', chorus: 'Chorus', bridge: 'Bridge', prechorus: 'Pre-chorus',
  tag: 'Tag', intro: 'Intro', outro: 'Outro', none: '',
};

export interface ChartViewProps {
  /**
   * Cache key: `${arrangementId}:${updated_at}`, so a save invalidates the parsed model.
   * Empty means "do not cache" — the editor's preview re-parses on every keystroke by design.
   */
  cacheKey: string;
  body: string;
  source: Key;
  target: Key;
  capo: number;
  display: DisplayPrefs;
  onRespelled?: (respelled: boolean) => void;
}

export function ChartView(props: ChartViewProps) {
  const chart = useMemo(
    () => (props.cacheKey === '' ? parseChart(props.body) : parseCached(props.cacheKey, props.body)),
    [props.cacheKey, props.body],
  );

  const rendered = useMemo(
    () => renderChart(chart, {
      source: props.source,
      target: props.target,
      capo: props.capo,
      layout: props.display.layout,
    }),
    [chart, props.source, props.target, props.capo, props.display.layout],
  );

  const { onRespelled } = props;
  useEffect(() => { onRespelled?.(rendered.respelled); }, [onRespelled, rendered.respelled]);

  if (chart.error !== null) {
    return <PlainFallback chart={chart} body={props.body} />;
  }

  return (
    <div
      className={props.display.columns === 2 ? 'gap-8 md:columns-2' : ''}
      style={{ fontSize: `${props.display.fontSize}px` }}
    >
      {rendered.sections.map((section, index) => (
        <section key={index} className="mb-5 break-inside-avoid">
          {props.display.showSections && section.kind !== 'none' && (
            <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-slate-500">
              {SECTION_LABELS[section.kind]}{section.label === null ? '' : ` ${section.label}`}
            </h3>
          )}

          {section.lines.map((line, lineIndex) => (
            <Line key={lineIndex} line={line} display={props.display} />
          ))}
        </section>
      ))}
    </div>
  );
}

function Line({ line, display }: { line: RenderedLine; display: DisplayPrefs }) {
  if (line.comment !== null) {
    return <p className="my-1 italic text-slate-500">{line.comment}</p>;
  }

  if (display.mode === 'lyrics') {
    const text = line.segments.map((segment) => segment.lyric).join('');

    return <p className="whitespace-pre-wrap">{text === '' ? ' ' : text}</p>;
  }

  if (display.mode === 'chords') {
    const chords = line.segments.map((s) => s.chord).filter((c): c is string => c !== null);

    return chords.length === 0
      ? null
      : <p className="font-mono font-semibold text-sky-700 dark:text-sky-300">{chords.join('  ')}</p>;
  }

  if (display.layout === 'inline') {
    return (
      <p className="whitespace-pre-wrap leading-8">
        {line.segments.map((segment, index) => (
          <span key={index}>
            {segment.chord !== null && (
              <span className="font-mono font-semibold text-sky-700 dark:text-sky-300">[{segment.chord}]</span>
            )}
            {segment.lyric}
          </span>
        ))}
      </p>
    );
  }

  // Chords over lyrics: two monospace rows whose columns line up by construction, which is the
  // same alignment the paste converter preserved on the way in.
  const rows = overLyricsRows(line);

  return (
    <div className="font-mono leading-tight">
      <div className="whitespace-pre font-semibold text-sky-700 dark:text-sky-300">
        {rows.chords === '' ? ' ' : rows.chords}
      </div>
      <div className="whitespace-pre">{rows.lyrics === '' ? ' ' : rows.lyrics}</div>
    </div>
  );
}

/**
 * A chart that cannot be read is still a chart someone has to play from tonight, so it falls
 * back to its own source text with the offending line named.
 */
function PlainFallback({ chart, body }: { chart: Chart; body: string }) {
  return (
    <div>
      <p className="mb-3 rounded border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900">
        This chart could not be read (line {chart.error!.line}: {chart.error!.message}) and is
        shown exactly as it was written.
      </p>
      <pre className="whitespace-pre-wrap font-mono text-sm">{body}</pre>
    </div>
  );
}
