import { useState } from 'react';
import type { Folder } from '../db/schema';

/**
 * The folder tree. Drag a folder to re-file it, drag a song onto a folder to move it there.
 *
 * The tree refuses a drop that would put a folder inside its own subtree — the caller checks the
 * path and says so out loud rather than silently doing nothing, because a drag that appears to
 * do nothing reads as a bug.
 */

export interface FolderTreeProps {
  folders: Folder[];
  selected: string | null;
  onSelect: (id: string | null) => void;
  canEdit: boolean;
  onCreate: (parentId: string | null) => void;
  onRename: (id: string, name: string) => void;
  onMoveFolder: (id: string, parentId: string | null) => void;
  onMoveSong: (songId: string, folderId: string | null) => void;
  onDelete: (folder: Folder) => void;
  counts: Map<string | null, number>;
}

export const SONG_DRAG_TYPE = 'application/x-aurum-song';
const FOLDER_DRAG_TYPE = 'application/x-aurum-folder';

export function FolderTree(props: FolderTreeProps) {
  const roots = props.folders.filter((folder) => folder.parent_id === null);

  return (
    <nav className="text-sm">
      <Row
        {...props}
        folder={null}
        depth={0}
        label="All songs"
        count={[...props.counts.values()].reduce((total, count) => total + count, 0)}
      />

      {roots.map((folder) => (
        <Branch key={folder.id} {...props} folder={folder} depth={0} />
      ))}

      {props.canEdit && (
        <button className="mt-2 text-xs underline text-slate-500" onClick={() => props.onCreate(null)}>
          New folder
        </button>
      )}
    </nav>
  );
}

function Branch({ folder, depth, ...props }: FolderTreeProps & { folder: Folder; depth: number }) {
  const [open, setOpen] = useState(true);
  const children = props.folders.filter((row) => row.parent_id === folder.id);

  return (
    <div>
      <Row
        {...props}
        folder={folder}
        depth={depth}
        label={folder.name}
        count={props.counts.get(folder.id) ?? 0}
        hasChildren={children.length > 0}
        open={open}
        onToggle={() => setOpen((value) => ! value)}
      />

      {open && children.map((child) => (
        <Branch key={child.id} {...props} folder={child} depth={depth + 1} />
      ))}
    </div>
  );
}

function Row(
  props: FolderTreeProps & {
    folder: Folder | null;
    depth: number;
    label: string;
    count: number;
    hasChildren?: boolean;
    open?: boolean;
    onToggle?: () => void;
  },
) {
  const [renaming, setRenaming] = useState(false);
  const [over, setOver] = useState(false);
  const id = props.folder?.id ?? null;
  const selected = props.selected === id;

  const drop = (event: React.DragEvent): void => {
    event.preventDefault();
    setOver(false);

    const song = event.dataTransfer.getData(SONG_DRAG_TYPE);
    const folder = event.dataTransfer.getData(FOLDER_DRAG_TYPE);

    if (song !== '') {
      props.onMoveSong(song, id);
    } else if (folder !== '' && folder !== id) {
      props.onMoveFolder(folder, id);
    }
  };

  return (
    <div
      className={`flex items-center gap-1 rounded px-2 py-1 ${selected ? 'bg-slate-200 dark:bg-slate-800' : ''} ${over ? 'ring-2 ring-sky-400' : ''}`}
      style={{ paddingLeft: `${props.depth * 12 + 8}px` }}
      onDragOver={(event) => { event.preventDefault(); setOver(true); }}
      onDragLeave={() => setOver(false)}
      onDrop={drop}
    >
      <button
        className={`w-4 text-slate-400 ${props.hasChildren === true ? '' : 'invisible'}`}
        onClick={props.onToggle}
        aria-label={props.open === true ? 'Collapse' : 'Expand'}
      >
        {props.open === true ? '▾' : '▸'}
      </button>

      {renaming && props.folder !== null ? (
        <input
          autoFocus
          className="flex-1 rounded border border-slate-300 px-1 dark:border-slate-700 dark:bg-slate-900"
          defaultValue={props.folder.name}
          onBlur={(event) => { props.onRename(props.folder!.id, event.target.value); setRenaming(false); }}
          onKeyDown={(event) => {
            if (event.key === 'Enter') event.currentTarget.blur();
            if (event.key === 'Escape') setRenaming(false);
          }}
        />
      ) : (
        <button
          className="flex-1 truncate text-left"
          draggable={props.canEdit && props.folder !== null}
          onDragStart={(event) => event.dataTransfer.setData(FOLDER_DRAG_TYPE, id ?? '')}
          onClick={() => props.onSelect(id)}
          onDoubleClick={() => props.canEdit && props.folder !== null && setRenaming(true)}
        >
          {props.label}
        </button>
      )}

      <span className="text-xs text-slate-400">{props.count}</span>

      {props.canEdit && props.folder !== null && (
        <span className="flex gap-1 text-xs text-slate-400">
          <button title="New subfolder" onClick={() => props.onCreate(id)}>＋</button>
          <button title="Delete folder" onClick={() => props.onDelete(props.folder!)}>🗑</button>
        </span>
      )}
    </div>
  );
}
