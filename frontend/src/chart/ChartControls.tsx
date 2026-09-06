import { useState } from 'react';
import type { DisplayPrefs } from './ChartView';
import type { KeySource } from './effectiveKey';
import { formatKey, shortestInterval, transposeKey, type Key } from './notes';
import { shapeKeyOf } from './render';

/**
 * The header of a chart: which arrangement, which key, which capo, how it is laid out.
 *
 * The key control says where the key came from, because there are four places it can come from
 * (business rule 7) and "why is this in F?" is otherwise unanswerable from the screen.
 */

const SOURCE_LABEL: Record<KeySource, string> = {
  set: 'from this set',
  preference: 'your preferred key',
  arrangement: 'arrangement default',
  song: 'original key',
  none: 'no key set',
};

export interface Arrangement {
  id: string;
  name: string;
}

export interface ChartControlsProps {
  arrangements: Arrangement[];
  arrangementId: string | null;
  onArrangement: (id: string) => void;

  target: Key | null;
  source: KeySource;
  original: Key | null;
  onKey: (key: Key | null) => void;

  capo: number;
  onCapo: (capo: number) => void;

  display: DisplayPrefs;
  onDisplay: (display: DisplayPrefs) => void;

  respelled: boolean;
  canEdit: boolean;
  editing: boolean;
  onToggleEdit: () => void;
}

export function ChartControls(props: ChartControlsProps) {
  const [open, setOpen] = useState(false);

  const choices = props.original === null
    ? []
    : Array.from({ length: 12 }, (_, semitones) => transposeKey(props.original!, semitones));

  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-slate-200 pb-3 dark:border-slate-800">
      {props.arrangements.length > 1 && (
        <select
          className="rounded border border-slate-300 bg-transparent px-2 py-1 text-sm dark:border-slate-700"
          value={props.arrangementId ?? ''}
          onChange={(event) => props.onArrangement(event.target.value)}
        >
          {props.arrangements.map((arrangement) => (
            <option key={arrangement.id} value={arrangement.id}>{arrangement.name}</option>
          ))}
        </select>
      )}

      <div className="relative">
        <button
          className="rounded border border-slate-300 px-3 py-1 text-sm dark:border-slate-700"
          onClick={() => setOpen((value) => ! value)}
        >
          {props.target === null ? 'Set key' : `Key of ${formatKey(props.target)}`}
          {props.capo > 0 && props.target !== null && (
            <span className="ml-2 text-slate-500">
              Capo {props.capo} — shapes in {formatKey(shapeKeyOf(props.target, props.capo))}
            </span>
          )}
        </button>

        {open && (
          <div className="absolute z-10 mt-1 w-72 rounded border border-slate-200 bg-white p-3 shadow-lg dark:border-slate-700 dark:bg-slate-900">
            <p className="mb-2 text-xs text-slate-500">
              {props.target === null ? SOURCE_LABEL.none : `Currently ${SOURCE_LABEL[props.source]}.`}
            </p>

            <div className="mb-3 grid grid-cols-4 gap-1">
              {choices.map((choice) => {
                const active = props.target !== null && formatKey(choice) === formatKey(props.target);
                const move = props.original === null ? 0 : shortestInterval(props.original, choice);

                return (
                  <button
                    key={formatKey(choice)}
                    title={move === 0 ? 'Written key' : `${move > 0 ? 'Up' : 'Down'} ${Math.abs(move)} semitones`}
                    className={`rounded px-2 py-1 text-sm ${active ? 'bg-slate-900 text-white dark:bg-slate-100 dark:text-slate-900' : 'border border-slate-200 dark:border-slate-700'}`}
                    onClick={() => props.onKey(choice)}
                  >
                    {formatKey(choice)}
                  </button>
                );
              })}
            </div>

            <label className="flex items-center gap-2 text-sm">
              Capo
              <select
                className="rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
                value={props.capo}
                onChange={(event) => props.onCapo(Number(event.target.value))}
              >
                {Array.from({ length: 12 }, (_, fret) => (
                  <option key={fret} value={fret}>{fret === 0 ? 'none' : fret}</option>
                ))}
              </select>
            </label>

            <p className="mt-2 text-xs text-slate-500">
              A capo changes the shapes you play, never the key the band hears.
            </p>

            <button className="mt-3 text-sm underline" onClick={() => { props.onKey(null); props.onCapo(0); }}>
              Reset to the written key
            </button>
          </div>
        )}
      </div>

      <select
        className="rounded border border-slate-300 bg-transparent px-2 py-1 text-sm dark:border-slate-700"
        value={props.display.layout}
        onChange={(event) => props.onDisplay({ ...props.display, layout: event.target.value as DisplayPrefs['layout'] })}
      >
        <option value="over">Chords over lyrics</option>
        <option value="inline">Inline brackets</option>
        <option value="nashville">Nashville numbers</option>
      </select>

      <select
        className="rounded border border-slate-300 bg-transparent px-2 py-1 text-sm dark:border-slate-700"
        value={props.display.mode}
        onChange={(event) => props.onDisplay({ ...props.display, mode: event.target.value as DisplayPrefs['mode'] })}
      >
        <option value="both">Chords and lyrics</option>
        <option value="chords">Chords only</option>
        <option value="lyrics">Lyrics only</option>
      </select>

      <div className="flex items-center gap-1">
        <button
          className="rounded border border-slate-300 px-2 text-sm dark:border-slate-700"
          title="Smaller"
          onClick={() => props.onDisplay({ ...props.display, fontSize: Math.max(11, props.display.fontSize - 1) })}
        >
          A−
        </button>
        <button
          className="rounded border border-slate-300 px-2 text-sm dark:border-slate-700"
          title="Larger"
          onClick={() => props.onDisplay({ ...props.display, fontSize: Math.min(40, props.display.fontSize + 1) })}
        >
          A+
        </button>
        <button
          className="rounded border border-slate-300 px-2 text-sm dark:border-slate-700"
          title="One or two columns"
          onClick={() => props.onDisplay({ ...props.display, columns: props.display.columns === 1 ? 2 : 1 })}
        >
          {props.display.columns === 1 ? '1 col' : '2 col'}
        </button>
      </div>

      {props.respelled && (
        <span
          className="rounded-full bg-amber-100 px-2 py-1 text-xs text-amber-900"
          title="One chord had no single-accidental spelling in this key and was written enharmonically."
        >
          respelled
        </span>
      )}

      {props.canEdit && (
        <button className="ml-auto text-sm underline" onClick={props.onToggleEdit}>
          {props.editing ? 'Done' : 'Edit chart'}
        </button>
      )}
    </div>
  );
}
