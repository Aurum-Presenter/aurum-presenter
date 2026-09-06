import { useLiveQuery } from 'dexie-react-hooks';
import { useMemo, useState } from 'react';
import { useWorkspace } from '../app/workspace';
import type { Song } from '../db/schema';
import { ITEM_TYPES, Sets } from './repository';

/**
 * Add items to a set: songs picked from the library in the order they were selected, or one of
 * the non-song items that still take a place in the running order.
 */
export function AddItemsDialog({ setId, onClose }: { setId: string; onClose: () => void }) {
  const { db, engine } = useWorkspace();
  const sets = useMemo(() => new Sets(db, engine), [db, engine]);

  const [query, setQuery] = useState('');
  const [picked, setPicked] = useState<Song[]>([]);
  const [type, setType] = useState(ITEM_TYPES[0]!.value);
  const [content, setContent] = useState('');

  const songs = useLiveQuery(
    () => db.songs.filter((song) => song.deleted_at === null && song.archived === 0).toArray(),
    [db],
    [],
  );

  const matches = useMemo(() => {
    const text = query.trim().toLowerCase();

    return songs
      .filter((song) => text === '' || song.title.toLowerCase().includes(text) || (song.artist ?? '').toLowerCase().includes(text))
      .sort((a, b) => a.title.localeCompare(b.title))
      .slice(0, 60);
  }, [songs, query]);

  const toggle = (song: Song): void => {
    // Selection order is the order they land in, which is what a person building a set expects.
    setPicked((current) => current.some((s) => s.id === song.id)
      ? current.filter((s) => s.id !== song.id)
      : [...current, song]);
  };

  return (
    <div className="fixed inset-0 z-20 flex items-center justify-center bg-slate-900/50 p-6" onClick={onClose}>
      <div
        className="flex max-h-[80vh] w-[36rem] flex-col rounded bg-white p-4 shadow-lg dark:bg-slate-900"
        onClick={(event) => event.stopPropagation()}
      >
        <h2 className="mb-2 font-semibold">Add to the set</h2>

        <input
          className="mb-2 rounded border border-slate-300 px-3 py-2 dark:border-slate-700 dark:bg-slate-950"
          placeholder="Search the library…"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />

        <ul className="mb-3 min-h-24 flex-1 overflow-auto rounded border border-slate-200 dark:border-slate-800">
          {matches.map((song) => {
            const order = picked.findIndex((s) => s.id === song.id);

            return (
              <li key={song.id}>
                <button
                  className={`flex w-full items-baseline gap-2 px-2 py-1 text-left text-sm ${order >= 0 ? 'bg-sky-50 dark:bg-sky-950' : ''}`}
                  onClick={() => toggle(song)}
                >
                  <span className="w-5 text-xs text-slate-400">{order >= 0 ? order + 1 : ''}</span>
                  <span>{song.title}</span>
                  {song.artist !== null && <span className="text-slate-500">{song.artist}</span>}
                  {song.original_key !== null && <span className="ml-auto text-slate-500">{song.original_key}</span>}
                </button>
              </li>
            );
          })}
        </ul>

        <div className="mb-3 flex flex-wrap items-center gap-2 text-sm">
          <select
            className="rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
            value={type}
            onChange={(event) => setType(event.target.value as typeof type)}
          >
            {ITEM_TYPES.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>

          <input
            className="flex-1 rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-950"
            placeholder="Its text — read Psalm 121, roll the video…"
            value={content}
            onChange={(event) => setContent(event.target.value)}
          />

          <button
            className="rounded border border-slate-300 px-3 py-1 dark:border-slate-700"
            onClick={() => { void sets.addItem(setId, type, content); setContent(''); }}
          >
            Add item
          </button>
        </div>

        <div className="flex gap-2">
          <button
            className="rounded bg-slate-900 px-4 py-2 text-sm text-white disabled:opacity-40 dark:bg-slate-100 dark:text-slate-900"
            disabled={picked.length === 0}
            onClick={() => { void sets.addSongs(setId, picked).then(onClose); }}
          >
            Add {picked.length > 0 ? `${picked.length} song${picked.length === 1 ? '' : 's'}` : 'songs'}
          </button>
          <button className="text-sm underline" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}
