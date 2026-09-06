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
  /** The home folder. Null is "unfiled", which is a permanent state, not a broken one. */
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
  /** JSON array of strings, stored whole: it is read whole and never queried across songs. */
  tags: string | null;
  artist: string | null;
  /** JSON array: the other names a congregation knows this song by. */
  alt_titles: string | null;
  duration_sec: number | null;
  /** Out of the way, not gone. Excluded from lists and search unless asked for. */
  archived: number;
}

/** A folder a song appears in besides its home folder. */
export interface SongPlacement extends SyncColumns {
  id: string;
  song_id: string;
  folder_id: string;
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
  /** What was pasted, before conversion. `over_lyrics` charts were converted on entry. */
  source_notation: 'chordpro' | 'over_lyrics';
  /** The pre-conversion text, kept for one undo and for debugging a bad conversion. */
  source_text: string | null;
  /** The arranger's suggested capo, 0–11. A reader's own capo lives in their preferences. */
  capo_hint: number | null;
}

export interface Sheet extends SyncColumns {
  id: string;
  song_id: string;
  /** One of the twelve keys, or null for a sheet that suits any key. */
  sheet_key: string | null;
  part: string | null;
  position: number;
  /** Null means the sheet applies to every arrangement of the song. */
  arrangement_id: string | null;
  label: string | null;
  filename: string | null;
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
  /** The service or gig date. Null is allowed, but it is what drives the 14-day auto-pin. */
  scheduled_for: string | null;
  notes: string | null;
  venue: string | null;
  /** JSON array of user ids. Display only — being named on a set grants nothing. */
  assigned_members: string | null;
  pinned: number;
}

export type SetItemType = 'announcement' | 'scripture' | 'prayer' | 'video' | 'blank' | 'text';

export interface SetItem extends SyncColumns {
  id: string;
  set_id: string;
  /** A fractional rank string, so two offline reorders merge by string order. */
  rank: string;
  /** An item is a song, or it carries its own content. Never both, never neither. */
  song_id: string | null;
  item_type: SetItemType | null;
  content: string | null;
  /** Kept so a set stays readable after its song is deleted. */
  title_snapshot: string | null;
  key_override: string | null;
  capo_override: number | null;
  arrangement_id: string | null;
  sheet_part_override: string | null;
  /** JSON array of section indices actually played; null means the whole chart. */
  sections: string | null;
  note: string | null;
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
  | 'folders' | 'songs' | 'song_placements' | 'arrangements' | 'sheets'
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

/**
 * The bytes themselves, but only where the origin private file system is missing. Everywhere
 * else this table stays empty and the files live in OPFS.
 */
export interface FileRecord {
  sheet_id: string;
  bytes: Blob;
}

/** A file waiting to be uploaded. It is kept until the server has it, so nothing is lost. */
export interface UploadRecord {
  sheet_id: string;
  sha256: string;
  size: number;
  filename: string;
  page_count: number | null;
  attempts: number;
  last_error: string | null;
  queued_at: string;
}

export class WorkspaceDb extends Dexie {
  folders!: EntityTable<Folder, 'id'>;
  songs!: EntityTable<Song, 'id'>;
  song_placements!: EntityTable<SongPlacement, 'id'>;
  arrangements!: EntityTable<Arrangement, 'id'>;
  sheets!: EntityTable<Sheet, 'id'>;
  annotations!: EntityTable<Annotation, 'id'>;
  sets!: EntityTable<SetRecord, 'id'>;
  set_items!: EntityTable<SetItem, 'id'>;
  preferences!: EntityTable<Preference, 'id'>;

  outbox!: EntityTable<OutboxOp, 'seq'>;
  files!: EntityTable<FileRecord, 'sheet_id'>;
  uploads!: EntityTable<UploadRecord, 'sheet_id'>;
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

    // A song can be filed in more than one folder. Added in version 2 rather than folded into
    // version 1: devices in the field already hold version 1 databases.
    this.version(2).stores({
      song_placements: 'id, song_id, folder_id, change_seq',
    });

    // Set items move from an integer position to a fractional rank, which is the index the
    // list is read by.
    this.version(3).stores({
      set_items: 'id, set_id, song_id, rank, change_seq',
    });

    // Sheet files: the bytes (where OPFS is missing) and the queue of files the server has not
    // been given yet.
    this.version(4).stores({
      files: 'sheet_id',
      uploads: 'sheet_id, queued_at',
    });
  }
}

export const SYNCED_TABLES: SyncedTable[] = [
  'folders', 'songs', 'song_placements', 'arrangements', 'sheets',
  'annotations', 'sets', 'set_items', 'preferences',
];
