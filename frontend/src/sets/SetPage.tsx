import { useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';
import { formatKey, parseKey, transposeKey } from '../chart/notes';
import { AddItemsDialog } from './AddItemsDialog';
import { isAutoPinned, parseList, Sets } from './repository';
import { useResolvedSet, type ResolvedItem } from './useResolvedSet';

/**
 * The set editor: the running order, per-item overrides, and the ways out of it — read,
 * present, print.
 *
 * A key override here is a band decision and overrides everyone's personal key. Anything a
 * member changes for themselves stays on their own device.
 */
export function SetPage() {
  const { setId } = useParams();
  const { db, engine, me, canEdit } = useWorkspace();
  const navigate = useNavigate();
  const sets = new Sets(db, engine);
  const { set, items } = useResolvedSet(setId);

  const [adding, setAdding] = useState(false);
  const [dragging, setDragging] = useState<number | null>(null);

  if (set === null) {
    return <p className="p-6 text-sm text-slate-500">Loading…</p>;
  }

  if (set.deleted_at !== null) {
    return <p className="p-6 text-sm text-slate-500">This set has been deleted.</p>;
  }

  const assigned = parseList(set.assigned_members);

  return (
    <div className="mx-auto max-w-4xl p-4">
      <Link className="text-sm underline" to="/sets">← Sets</Link>

      <div className="mb-4 mt-2 flex flex-wrap items-center gap-3">
        {canEdit ? (
          <input
            className="rounded border border-transparent bg-transparent text-2xl font-semibold hover:border-slate-300 dark:hover:border-slate-700"
            value={set.name}
            onChange={(event) => void sets.update(set.id, { name: event.target.value })}
          />
        ) : (
          <h2 className="text-2xl font-semibold">{set.name}</h2>
        )}

        {canEdit && (
          <>
            <input
              className="rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
              type="date"
              value={set.scheduled_for ?? ''}
              onChange={(event) => void sets.update(set.id, { scheduled_for: event.target.value === '' ? null : event.target.value })}
            />
            <input
              className="rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
              placeholder="Venue"
              value={set.venue ?? ''}
              onChange={(event) => void sets.update(set.id, { venue: event.target.value })}
            />
            <label className="flex items-center gap-1 text-sm text-slate-500">
              <input
                type="checkbox"
                checked={set.pinned === 1}
                onChange={(event) => void sets.update(set.id, { pinned: event.target.checked ? 1 : 0 })}
              />
              Keep offline
            </label>
          </>
        )}

        {isAutoPinned(set) && set.pinned === 0 && (
          <span className="rounded-full bg-sky-100 px-2 py-1 text-xs text-sky-900">kept offline — it is coming up</span>
        )}

        <span className="ml-auto flex gap-3 text-sm">
          {items.length > 0 && <Link className="underline" to={`/sets/${set.id}/read/0`}>Read</Link>}
          {items.length > 0 && <Link className="underline" to={`/sets/${set.id}/print`}>Print</Link>}
        </span>
      </div>

      {canEdit && (
        <label className="mb-4 block text-sm">
          <span className="text-slate-500">Playing</span>
          <select
            multiple
            className="mt-1 w-full rounded border border-slate-300 p-1 dark:border-slate-700 dark:bg-slate-900"
            size={Math.min(4, Math.max(2, me.workspaces.length))}
            value={assigned}
            onChange={(event) => void sets.update(set.id, {
              assigned_members: JSON.stringify([...event.target.selectedOptions].map((option) => option.value)),
            })}
          >
            <option value={me.id}>{me.display_name} (you)</option>
          </select>
          <span className="text-xs text-slate-400">Who is playing. This is a note, not a permission.</span>
        </label>
      )}

      {items.length === 0 ? (
        <p className="rounded border border-dashed border-slate-300 py-10 text-center text-sm text-slate-500 dark:border-slate-700">
          Nothing in this set yet.
        </p>
      ) : (
        <ol className="divide-y divide-slate-200 dark:divide-slate-800">
          {items.map((resolved, index) => (
            <li
              key={resolved.item.id}
              draggable={canEdit}
              onDragStart={() => setDragging(index)}
              onDragOver={(event) => event.preventDefault()}
              onDrop={() => {
                if (dragging !== null) void sets.move(set.id, dragging, index);
                setDragging(null);
              }}
              className="py-2"
            >
              <Item
                index={index}
                resolved={resolved}
                canEdit={canEdit}
                onChange={(changes) => void sets.updateItem(resolved.item.id, changes)}
                onRemove={() => void sets.removeItem(resolved.item.id)}
                onOpen={() => resolved.song !== null && navigate(`/song/${resolved.song.id}`)}
              />
            </li>
          ))}
        </ol>
      )}

      {canEdit && (
        <button className="mt-4 rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900" onClick={() => setAdding(true)}>
          Add items
        </button>
      )}

      {adding && <AddItemsDialog setId={set.id} onClose={() => setAdding(false)} />}
    </div>
  );
}

function Item(props: {
  index: number;
  resolved: ResolvedItem;
  canEdit: boolean;
  onChange: (changes: Record<string, unknown>) => void;
  onRemove: () => void;
  onOpen: () => void;
}) {
  const { item, missing, song } = props.resolved;
  const keys = Array.from({ length: 12 }, (_, semitones) => transposeKey(parseKey('C')!, semitones));

  return (
    <div className="flex flex-wrap items-baseline gap-2">
      <span className="w-6 text-sm text-slate-400">{props.index + 1}</span>

      {song !== null ? (
        <button className="font-medium underline-offset-2 hover:underline" onClick={props.onOpen}>{props.resolved.title}</button>
      ) : (
        <span className={missing ? 'font-medium text-amber-700 dark:text-amber-400' : 'font-medium'}>
          {props.resolved.title}
          {missing && <span className="ml-2 text-xs">missing song</span>}
        </span>
      )}

      {item.item_type !== null && (
        <span className="rounded bg-slate-100 px-2 text-xs text-slate-600 dark:bg-slate-800 dark:text-slate-300">{item.item_type}</span>
      )}

      {props.resolved.key !== null && (
        <span className="text-sm text-slate-500" title={`Key from the ${props.resolved.source === 'set' ? 'set override' : props.resolved.source}`}>
          {formatKey(props.resolved.key)}
          {props.resolved.source === 'set' && <span className="ml-1 text-xs">· set key</span>}
        </span>
      )}

      {props.canEdit && (
        <span className="ml-auto flex flex-wrap items-center gap-2 text-sm">
          {item.song_id !== null && (
            <>
              <select
                className="rounded border border-slate-300 bg-transparent px-1 dark:border-slate-700"
                value={item.key_override ?? ''}
                onChange={(event) => props.onChange({ key_override: event.target.value === '' ? null : event.target.value })}
                title="A key for the whole band, for this set only"
              >
                <option value="">each member's key</option>
                {keys.map((key) => <option key={formatKey(key)} value={formatKey(key)}>{formatKey(key)}</option>)}
              </select>

              <select
                className="rounded border border-slate-300 bg-transparent px-1 dark:border-slate-700"
                value={item.capo_override ?? ''}
                onChange={(event) => props.onChange({ capo_override: event.target.value === '' ? null : Number(event.target.value) })}
                title="Capo for this set"
              >
                <option value="">capo: each member</option>
                {Array.from({ length: 12 }, (_, fret) => <option key={fret} value={fret}>capo {fret}</option>)}
              </select>
            </>
          )}

          <input
            className="w-40 rounded border border-slate-300 px-2 py-1 text-xs dark:border-slate-700 dark:bg-slate-900"
            placeholder="Note — start at chorus…"
            defaultValue={item.note ?? ''}
            onBlur={(event) => props.onChange({ note: event.target.value === '' ? null : event.target.value })}
          />

          <button className="text-red-700 underline dark:text-red-400" onClick={props.onRemove}>Remove</button>
        </span>
      )}

      {item.note !== null && ! props.canEdit && <span className="text-xs text-slate-500">{item.note}</span>}
      {item.content !== null && <p className="basis-full pl-6 text-sm text-slate-500">{item.content}</p>}
    </div>
  );
}
