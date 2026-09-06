import Dexie, { type EntityTable } from 'dexie';

/**
 * The device's working source of truth.
 *
 * One Dexie database per workspace, mirroring the server's one-SQLite-file-per-workspace
 * layout. Because the file *is* the scope on the server, these records carry no workspaceId —
 * switching workspaces means opening a different database, not filtering a shared one.
 */

/** Columns the server owns. A local edit never sets these; they arrive on the next pull. */
export interface SyncColumns {
  updated_at: string;
  change_seq: number;
  deleted_at: string | null;
  updated_by: string | null;
}

export interface Folder extends SyncColumns {
  id: string;
  parent_id: string | null;
  name: string;
  position: number;
}

export interface Song extends SyncColumns {
  id: string;
  folder_id: string | null;
  title: string;
  subtitle: string | null;
  authors: string | null;
  ccli_number: string | null;
  copyright: string | null;
  notes: string | null;
  original_key: string | null;
  tempo: number | null;
  time_signature: string | null;
  tags: string | null;
}

export interface Arrangement extends SyncColumns {
  id: string;
  song_id: string;
  name: string;
  /** ChordPro. The canonical chart storage — never a PDF. */
  body: string;
  default_key: string | null;
  is_default: number;
  position: number;
}

export interface Sheet extends SyncColumns {
  id: string;
  song_id: string;
  sheet_key: string | null;
  part: string | null;
  position: number;
  /** Null until the file has been uploaded. That is the "not downloaded" state, not an error. */
  sha256: string | null;
  size: number | null;
  page_count: number | null;
  mime_type: string;
  uploaded_at: string | null;
}

export interface Annotation extends SyncColumns {
  id: string;
  sheet_id: string;
  page: number;
  strokes: string;
  scope: 'personal' | 'shared';
  author_id: string;
}

export interface SetRecord extends SyncColumns {
  id: string;
  name: string;
  scheduled_for: string | null;
  notes: string | null;
}

export interface SetItem extends SyncColumns {
  id: string;
  set_id: string;
  song_id: string | null;
  kind: 'song' | 'note' | 'break' | 'media';
  title: string | null;
  key_override: string | null;
  position: number;
  notes: string | null;
}

export interface Preference extends SyncColumns {
  id: string;
  user_id: string;
  scope_type: 'song' | 'set' | 'folder' | 'workspace';
  scope_id: string | null;
  name: string;
  value: string | null;
}

export type SyncedTable =
  | 'folders' | 'songs' | 'arrangements' | 'sheets'
  | 'annotations' | 'sets' | 'set_items' | 'preferences';

/** A durable local mutation, replayed to the server when connectivity returns. */
export interface OutboxOp {
  seq?: number;
  op_id: string;
  table: SyncedTable;
  record_id: string;
  op: 'upsert' | 'delete';
  payload: Record<string, unknown>;
  /**
   * The updated_at this device last saw for the record. The server uses it to decide whether
   * anything changed underneath the edit, and so whether a conflict record is warranted.
   */
  base_updated_at: string | null;
  attempts: number;
  last_error: string | null;
  status: 'pending' | 'parked';
  created_at: string;
}

export interface SyncState {
  key: 'watermark';
  change_seq: number;
  last_pull_at: string | null;
  last_push_at: string | null;
}

export interface ConflictRecord {
  id: string;
  table: SyncedTable;
  record_id: string;
  field: string;
  losing_value: string | null;
  at: string;
  reviewed_at: string | null;
}

export interface BlobRecord {
  sheet_id: string;
  sha256: string;
  size: number;
  pin_reason: 'pinned' | 'opportunistic';
  cached_at: string;
}

export class WorkspaceDb extends Dexie {
  folders!: EntityTable<Folder, 'id'>;
  songs!: EntityTable<Song, 'id'>;
  arrangements!: EntityTable<Arrangement, 'id'>;
  sheets!: EntityTable<Sheet, 'id'>;
  annotations!: EntityTable<Annotation, 'id'>;
  sets!: EntityTable<SetRecord, 'id'>;
  set_items!: EntityTable<SetItem, 'id'>;
  preferences!: EntityTable<Preference, 'id'>;

  outbox!: EntityTable<OutboxOp, 'seq'>;
  sync_state!: EntityTable<SyncState, 'key'>;
  conflicts!: EntityTable<ConflictRecord, 'id'>;
  blobs!: EntityTable<BlobRecord, 'sheet_id'>;

  constructor(workspaceId: string) {
    super(`aurum-${workspaceId}`);

    this.version(1).stores({
      folders: 'id, parent_id, change_seq',
      songs: 'id, folder_id, title, change_seq',
      arrangements: 'id, song_id, change_seq',
      sheets: 'id, song_id, change_seq',
      annotations: 'id, sheet_id, change_seq',
      sets: 'id, scheduled_for, change_seq',
      set_items: 'id, set_id, change_seq',
      preferences: 'id, [user_id+scope_type+scope_id+name], change_seq',

      // ++seq gives the outbox a monotonic local order, which is what lets it be drained
      // strictly in the order the user made the changes.
      outbox: '++seq, op_id, status, created_at',
      sync_state: 'key',
      conflicts: 'id, table, record_id, reviewed_at',
      blobs: 'sheet_id, cached_at, pin_reason',
    });
  }
}

export const SYNCED_TABLES: SyncedTable[] = [
  'folders', 'songs', 'arrangements', 'sheets',
  'annotations', 'sets', 'set_items', 'preferences',
];
