import { useRef, useState } from 'react';

/**
 * Freehand marks over a sheet.
 *
 * Coordinates are normalised to 0–1 of the page, never pixels (business rule 7). A mark drawn on
 * a phone at fit-width has to land in the same place on a tablet at 200 % and on a rotated page,
 * and the only way that holds is to store where it is on the *page* rather than on the screen.
 */

export interface Stroke {
  kind: 'path';
  color: string;
  width: number;
  points: [number, number][];
}

const COLORS = ['#dc2626', '#2563eb', '#16a34a', '#eab308', '#0f172a'];

export function AnnotationLayer(props: {
  width: number;
  height: number;
  /** Everything visible: the reader's own marks plus anything shared with the band. */
  strokes: Stroke[];
  /** The subset this reader owns in the current scope, which is what editing replaces. */
  mine: Stroke[];
  drawing: boolean;
  onChange: (strokes: Stroke[]) => void;
}) {
  const [color, setColor] = useState(COLORS[0]!);
  const [width, setWidth] = useState(3);
  const [current, setCurrent] = useState<Stroke | null>(null);
  const surface = useRef<SVGSVGElement>(null);

  const at = (event: React.PointerEvent): [number, number] => {
    const box = surface.current!.getBoundingClientRect();

    return [
      Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)),
      Math.min(1, Math.max(0, (event.clientY - box.top) / box.height)),
    ];
  };

  const path = (stroke: Stroke): string =>
    stroke.points
      .map(([x, y], index) => `${index === 0 ? 'M' : 'L'} ${x * props.width} ${y * props.height}`)
      .join(' ');

  return (
    <>
      <svg
        ref={surface}
        className={`absolute inset-0 ${props.drawing ? 'cursor-crosshair touch-none' : 'pointer-events-none'}`}
        width={props.width}
        height={props.height}
        onPointerDown={(event) => {
          if (! props.drawing) return;
          event.currentTarget.setPointerCapture(event.pointerId);
          setCurrent({ kind: 'path', color, width, points: [at(event)] });
        }}
        onPointerMove={(event) => {
          if (current === null) return;
          setCurrent({ ...current, points: [...current.points, at(event)] });
        }}
        onPointerUp={() => {
          if (current === null) return;
          props.onChange([...props.mine, current]);
          setCurrent(null);
        }}
      >
        {[...props.strokes, ...(current === null ? [] : [current])].map((stroke, index) => (
          <path
            key={index}
            d={path(stroke)}
            stroke={stroke.color}
            strokeWidth={stroke.width}
            strokeLinecap="round"
            strokeLinejoin="round"
            fill="none"
          />
        ))}
      </svg>

      {props.drawing && (
        <div className="absolute left-2 top-2 flex items-center gap-2 rounded bg-white/90 p-1 shadow dark:bg-slate-900/90">
          {COLORS.map((option) => (
            <button
              key={option}
              className={`h-5 w-5 rounded-full ${option === color ? 'ring-2 ring-offset-1' : ''}`}
              style={{ backgroundColor: option }}
              onClick={() => setColor(option)}
              aria-label={`Draw in ${option}`}
            />
          ))}

          <input
            type="range"
            min={1}
            max={10}
            value={width}
            className="w-20"
            onChange={(event) => setWidth(Number(event.target.value))}
          />

          <button
            className="text-xs underline"
            onClick={() => props.onChange(props.mine.slice(0, -1))}
            disabled={props.mine.length === 0}
          >
            undo
          </button>

          <button className="text-xs underline" onClick={() => props.onChange([])} disabled={props.mine.length === 0}>
            clear mine
          </button>
        </div>
      )}
    </>
  );
}
