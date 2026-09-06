import { useLiveQuery } from 'dexie-react-hooks';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { useWorkspace } from '../app/workspace';
import type { Folder, Song } from '../db/schema';
import { FolderTree, SONG_DRAG_TYPE } from './FolderTree';
import { ImportDialog } from './ImportDialog';
import { listOf, songsInFolder } from './repository';
import { lyricsOf, type IndexedSong } from './search';
import { useSearch } from './useSearch';

/**
 * The library: folder tree on the left, songs on the right, search over everything.
 *
 * Every list on this page is rendered from IndexedDB. There is no loading state for a song the
 * device already has, because there is no request to wait for.
 */

type Sort = 'title' | 'recent' | 'tempo' | 'key';

export function LibraryPage() {
  const { db, library, canEdit } = useWorkspace();
  const { folderId } = useParams();
  const navigate = useNavigate();

  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<Sort>('title');
  const [tag, setTag] = useState('');
  const [key, setKey] = useState('');
  const [showArchived, setShowArchived] = useState(false);
  const [importing, setImporting] = useState(false);
  const [deleting, setDeleting] = useState<Folder | null>(null);
  const [newTitle, setNewTitle] = useState('');
  const [problem, setProblem] = useState<string | null>(null);
  const searchBox = useRef<HTMLInputElement>(null);

  const folders = useLiveQuery(
    () => db.folders.filter((row) => row.deleted_at === null).toArray(),
    [db],
    [],
  );

  const songs = useLiveQuery(
    () => db.songs.filter((row) => row.deleted_at === null).toArray(),
    [db],
    [],
  );

  const placements = useLiveQuery(() => db.song_placements.toArray(), [db], []);
  const arrangements = useLiveQuery(() => db.arrangements.toArray(), [db], []);

  // The search index wants the words a person would type: titles, artist, tags and the lyrics
  // out of the chart — never the chord letters.
  const indexed = useMemo<IndexedSong[]>(() => {
    const charts = new Map<string, string>();

    for (const arrangement of arrangements) {
      if (arrangement.deleted_at === null) {
        charts.set(arrangement.song_id, (charts.get(arrangement.song_id) ?? '') + ' ' + lyricsOf(arrangement.body));
      }
    }

    return songs.map((song) => ({
      id: song.id,
      title: song.title,
      altTitles: listOf(song.alt_titles),
      artist: song.artist,
      tags: listOf(song.tags),
      lyrics: charts.get(song.id) ?? '',
    }));
  }, [songs, arrangements]);

  const hits = useSearch(indexed, query);

  useEffect(() => {
    const shortcut = (event: KeyboardEvent): void => {
      const typing = document.activeElement instanceof HTMLInputElement
        || document.activeElement instanceof HTMLTextAreaElement;

      if ((event.key === '/' && ! typing) || (event.key === 'k' && (event.metaKey || event.ctrlKey))) {
        event.preventDefault();
        searchBox.current?.focus();
      }
    };

    window.addEventListener('keydown', shortcut);

    return () => window.removeEventListener('keydown', shortcut);
  }, []);

  const tags = useMemo(
    () => [...new Set(songs.flatMap((song) => listOf(song.tags)))].sort(),
    [songs],
  );

  const counts = useMemo(() => {
    const map = new Map<string | null, number>();

    for (const song of songs) {
      if (song.archived === 0) {
        map.set(song.folder_id, (map.get(song.folder_id) ?? 0) + 1);
      }
    }

    return map;
  }, [songs]);

  const visible = useMemo(() => {
    let rows = hits === null
      ? songsInFolder(songs, placements, folderId ?? null)
      : hits.map((hit) => songs.find((song) => song.id === hit.id)).filter((song): song is Song => song !== undefined);

    if (! showArchived) {
      rows = rows.filter((song) => song.archived === 0);
    }

    if (tag !== '') {
      rows = rows.filter((song) => listOf(song.tags).includes(tag));
    }

    if (key !== '') {
      rows = rows.filter((song) => song.original_key === key);
    }

    // Search results arrive ranked; re-sorting them by title would throw the ranking away.
    if (hits !== null) {
      return rows;
    }

    return [...rows].sort((a, b) => {
      switch (sort) {
        case 'recent':
          return b.updated_at.localeCompare(a.updated_at);
        case 'tempo':
          return (a.tempo ?? 999) - (b.tempo ?? 999);
        case 'key':
          return (a.original_key ?? 'zz').localeCompare(b.original_key ?? 'zz');
        default:
          return a.title.localeCompare(b.title);
      }
    });
  }, [hits, songs, placements, folderId, showArchived, tag, key, sort]);

  const duplicates = useMemo(() => {
    const seen = new Map<string, number>();

    for (const song of songs) {
      const name = song.title.trim().toLowerCase();
      seen.set(name, (seen.get(name) ?? 0) + 1);
    }

    return seen;
  }, [songs]);

  const keys = useMemo(
    () => [...new Set(songs.map((song) => song.original_key).filter((value): value is string => value !== null))].sort(),
    [songs],
  );

  const addSong = async (event: React.FormEvent): Promise<void> => {
    event.preventDefault();

    if (newTitle.trim() === '') {
      setProblem('A title is required.');
      return;
    }

    const id = await library.createSong({ title: newTitle, folder_id: folderId ?? null });
    setNewTitle('');
    navigate(`/song/${id}`);
  };

  const move = (id: string, target: string | null): void => {
    void library.moveSong(id, target);
  };

  const moveFolder = (id: string, parent: string | null): void => {
    library.moveFolder(id, parent).catch((error: Error) => setProblem(error.message));
  };

  return (
    <div className="mx-auto flex max-w-6xl gap-6 p-4">
      <aside className="hidden w-56 shrink-0 md:block">
        <FolderTree
          folders={folders}
          selected={folderId ?? null}
          onSelect={(id) => navigate(id === null ? '/library' : `/library/folder/${id}`)}
          canEdit={canEdit}
          counts={counts}
          onCreate={(parent) => {
            const name = prompt('Folder name');
            if (name !== null && name.trim() !== '') void library.createFolder(name, parent);
          }}
          onRename={(id, name) => { if (name.trim() !== '') void library.renameFolder(id, name); }}
          onMoveFolder={moveFolder}
          onMoveSong={move}
          onDelete={setDeleting}
        />

        <Link className="mt-4 block text-xs underline text-slate-500" to="/library/trash">Trash</Link>
      </aside>

      <main className="min-w-0 flex-1">
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <input
            ref={searchBox}
            className="min-w-40 flex-1 rounded border border-slate-300 px-3 py-2 dark:border-slate-700 dark:bg-slate-900"
            placeholder="Search titles, lyrics, tags…   (press /)"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />

          <select className="rounded border border-slate-300 bg-transparent px-2 py-2 text-sm dark:border-slate-700" value={sort} onChange={(e) => setSort(e.target.value as Sort)}>
            <option value="title">Title</option>
            <option value="recent">Recently edited</option>
            <option value="tempo">Tempo</option>
            <option value="key">Key</option>
          </select>

          {tags.length > 0 && (
            <select className="rounded border border-slate-300 bg-transparent px-2 py-2 text-sm dark:border-slate-700" value={tag} onChange={(e) => setTag(e.target.value)}>
              <option value="">All tags</option>
              {tags.map((value) => <option key={value} value={value}>{value}</option>)}
            </select>
          )}

          {keys.length > 0 && (
            <select className="rounded border border-slate-300 bg-transparent px-2 py-2 text-sm dark:border-slate-700" value={key} onChange={(e) => setKey(e.target.value)}>
              <option value="">Any key</option>
              {keys.map((value) => <option key={value} value={value}>{value}</option>)}
            </select>
          )}

          <label className="flex items-center gap-1 text-sm text-slate-500">
            <input type="checkbox" checked={showArchived} onChange={(e) => setShowArchived(e.target.checked)} />
            Archived
          </label>
        </div>

        {canEdit && (
          <form className="mb-4 flex gap-2" onSubmit={(event) => void addSong(event)}>
            <input
              className="flex-1 rounded border border-slate-300 px-3 py-2 dark:border-slate-700 dark:bg-slate-900"
              placeholder="Add a song…"
              value={newTitle}
              onChange={(event) => { setNewTitle(event.target.value); setProblem(null); }}
            />
            <button className="rounded bg-slate-900 px-4 py-2 text-white dark:bg-slate-100 dark:text-slate-900">Add</button>
            <button type="button" className="rounded border border-slate-300 px-4 py-2 dark:border-slate-700" onClick={() => setImporting(true)}>
              Import
            </button>
          </form>
        )}

        {problem !== null && (
          <p className="mb-3 rounded border border-red-300 bg-red-50 p-2 text-sm text-red-800">{problem}</p>
        )}

        {visible.length === 0 ? (
          <Empty query={query} canEdit={canEdit} onImport={() => setImporting(true)} />
        ) : (
          <ul className="divide-y divide-slate-200 dark:divide-slate-800">
            {visible.map((song) => (
              <li
                key={song.id}
                draggable={canEdit}
                onDragStart={(event) => event.dataTransfer.setData(SONG_DRAG_TYPE, song.id)}
              >
                <Link className="flex items-baseline gap-2 py-2" to={`/song/${song.id}`}>
                  <span className="font-medium">{song.title}</span>
                  {song.artist !== null && <span className="text-sm text-slate-500">{song.artist}</span>}
                  {song.original_key !== null && <span className="text-sm text-slate-500">· {song.original_key}</span>}
                  {song.tempo !== null && <span className="text-sm text-slate-500">· {song.tempo} bpm</span>}
                  {song.archived === 1 && <span className="text-xs text-amber-600">archived</span>}
                  {(duplicates.get(song.title.trim().toLowerCase()) ?? 0) > 1 && (
                    <span className="text-xs text-slate-400" title="Another song has this title">possible duplicate</span>
                  )}
                  <span className="ml-auto flex gap-1">
                    {listOf(song.tags).map((value) => (
                      <span key={value} className="rounded bg-slate-100 px-2 text-xs text-slate-600 dark:bg-slate-800 dark:text-slate-300">{value}</span>
                    ))}
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        )}
      </main>

      {importing && <ImportDialog onClose={() => setImporting(false)} folderId={folderId ?? null} />}

      {deleting !== null && (
        <DeleteFolderDialog
          folder={deleting}
          songs={counts.get(deleting.id) ?? 0}
          onCancel={() => setDeleting(null)}
          onConfirm={(choice) => {
            void library.deleteFolder(deleting.id, choice);
            setDeleting(null);
            navigate('/library');
          }}
        />
      )}
    </div>
  );
}

function Empty({ query, canEdit, onImport }: { query: string; canEdit: boolean; onImport: () => void }) {
  if (query.trim() !== '') {
    return <p className="py-8 text-center text-sm text-slate-500">Nothing matches “{query}”.</p>;
  }

  return (
    <div className="rounded border border-dashed border-slate-300 py-12 text-center dark:border-slate-700">
      <p className="text-slate-500">No songs here yet.</p>
      {canEdit && (
        <button className="mt-3 text-sm underline" onClick={onImport}>Import ChordPro or text files</button>
      )}
    </div>
  );
}

/**
 * Business rule 2: deleting a folder is never allowed to quietly decide what happens to the
 * songs inside it.
 */
function DeleteFolderDialog(props: {
  folder: Folder;
  songs: number;
  onCancel: () => void;
  onConfirm: (choice: 'move-to-parent' | 'archive') => void;
}) {
  return (
    <div className="fixed inset-0 z-20 flex items-center justify-center bg-slate-900/50 p-6">
      <div className="w-96 rounded bg-white p-4 shadow-lg dark:bg-slate-900">
        <h2 className="mb-2 font-semibold">Delete “{props.folder.name}”?</h2>
        <p className="mb-4 text-sm text-slate-500">
          {props.songs === 0
            ? 'The folder is empty. Subfolders move up to its parent.'
            : `${props.songs} song${props.songs === 1 ? '' : 's'} are filed here. Nothing is deleted — choose where they go.`}
        </p>
        <div className="flex flex-wrap gap-2">
          <button className="rounded bg-slate-900 px-3 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900" onClick={() => props.onConfirm('move-to-parent')}>
            Move songs to the parent folder
          </button>
          <button className="rounded border border-slate-300 px-3 py-2 text-sm dark:border-slate-700" onClick={() => props.onConfirm('archive')}>
            Archive the songs
          </button>
          <button className="ml-auto text-sm underline" onClick={props.onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
