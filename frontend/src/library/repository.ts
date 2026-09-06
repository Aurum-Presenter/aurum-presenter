import type { Folder, Song, SongPlacement, WorkspaceDb } from '../db/schema';
import { uuidv7 } from '../db/uuid';
import type { SyncEngine } from '../sync/engine';

/**
 * Library writes. Every one of them lands in IndexedDB and the outbox in a single transaction,
 * so the library is fully usable with the radio off and the server hears about it later.
 */

export interface SongInput {
  title: string;
  folder_id?: string | null;
  artist?: string | null;
  authors?: string | null;
  subtitle?: string | null;
  ccli_number?: string | null;
  copyright?: string | null;
  notes?: string | null;
  original_key?: string | null;
  tempo?: number | null;
  time_signature?: string | null;
  duration_sec?: number | null;
  tags?: string[];
  alt_titles?: string[];
}

export const MAX_TITLE = 200;

/** Client-side validation, for immediate feedback. The database re-checks what it can. */
export function validateSong(input: SongInput): string[] {
  const problems: string[] = [];
  const title = input.title.trim();

  if (title === '') {
    problems.push('A title is required.');
  }

  if (title.length > MAX_TITLE) {
    problems.push(`A title is at most ${MAX_TITLE} characters.`);
  }

  if (input.tempo != null && (input.tempo < 20 || input.tempo > 300)) {
    problems.push('Tempo is between 20 and 300 bpm.');
  }

  if (input.ccli_number != null && input.ccli_number !== '' && ! /^\d+$/.test(input.ccli_number)) {
    problems.push('A CCLI number is digits only.');
  }

  return problems;
}

/** `mm:ss` in, seconds out. Anything else is null rather than a guess. */
export function parseDuration(text: string): number | null {
  const match = /^\s*(\d{1,3}):([0-5]\d)\s*$/.exec(text);

  return match === null ? null : Number(match[1]) * 60 + Number(match[2]);
}

export function formatDuration(seconds: number | null): string {
  if (seconds === null) {
    return '';
  }

  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

function payloadOf(input: SongInput): Record<string, unknown> {
  return {
    title: input.title.trim(),
    folder_id: input.folder_id ?? null,
    artist: blank(input.artist),
    authors: blank(input.authors),
    subtitle: blank(input.subtitle),
    ccli_number: blank(input.ccli_number),
    copyright: blank(input.copyright),
    notes: blank(input.notes),
    original_key: blank(input.original_key),
    tempo: input.tempo ?? null,
    time_signature: blank(input.time_signature),
    duration_sec: input.duration_sec ?? null,
    tags: JSON.stringify(input.tags ?? []),
    alt_titles: JSON.stringify(input.alt_titles ?? []),
  };
}

function blank(value: string | null | undefined): string | null {
  return value == null || value.trim() === '' ? null : value.trim();
}

export function listOf(json: string | null): string[] {
  if (json === null || json === '') {
    return [];
  }

  try {
    const parsed = JSON.parse(json) as unknown;

    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === 'string') : [];
  } catch {
    return [];
  }
}

export class Library {
  constructor(
    private readonly db: WorkspaceDb,
    private readonly engine: SyncEngine,
  ) {}

  async createSong(input: SongInput): Promise<string> {
    // UUIDv7 on the device: a song created offline keeps the identity it was born with, so the
    // eventual push is an upsert and never a second copy.
    const id = uuidv7();

    await this.engine.record('songs', id, 'upsert', { ...payloadOf(input), archived: 0 });

    return id;
  }

  async updateSong(id: string, input: SongInput): Promise<void> {
    await this.engine.record('songs', id, 'upsert', payloadOf(input));
  }

  async setArchived(id: string, archived: boolean): Promise<void> {
    await this.engine.record('songs', id, 'upsert', { archived: archived ? 1 : 0 });
  }

  async moveSong(id: string, folderId: string | null): Promise<void> {
    await this.engine.record('songs', id, 'upsert', { folder_id: folderId });
  }

  /** Soft delete: it replicates, and Trash can put it back for 30 days. */
  async deleteSong(id: string): Promise<void> {
    await this.engine.record('songs', id, 'delete', {});
  }

  /**
   * Undelete is an upsert, not a flag flip: the server treats a tombstone as final for a
   * concurrent *edit*, but an explicit restore is a new write that clears it.
   */
  async restoreSong(id: string): Promise<void> {
    const song = await this.db.songs.get(id);

    if (song === undefined) {
      return;
    }

    await this.engine.record('songs', id, 'upsert', { title: song.title });
  }

  async duplicateSong(id: string): Promise<string | null> {
    const song = await this.db.songs.get(id);

    if (song === undefined) {
      return null;
    }

    const copy = await this.createSong({
      title: `${song.title} (copy)`,
      folder_id: song.folder_id,
      artist: song.artist,
      authors: song.authors,
      subtitle: song.subtitle,
      ccli_number: song.ccli_number,
      copyright: song.copyright,
      notes: song.notes,
      original_key: song.original_key,
      tempo: song.tempo,
      time_signature: song.time_signature,
      duration_sec: song.duration_sec,
      tags: listOf(song.tags),
      alt_titles: listOf(song.alt_titles),
    });

    const arrangements = await this.db.arrangements
      .where('song_id').equals(id)
      .filter((row) => row.deleted_at === null)
      .toArray();

    for (const arrangement of arrangements) {
      await this.engine.record('arrangements', uuidv7(), 'upsert', {
        song_id: copy,
        name: arrangement.name,
        body: arrangement.body,
        default_key: arrangement.default_key,
        is_default: arrangement.is_default,
        position: arrangement.position,
        source_notation: arrangement.source_notation,
        capo_hint: arrangement.capo_hint,
      });
    }

    return copy;
  }

  async createFolder(name: string, parentId: string | null): Promise<string> {
    const id = uuidv7();
    const siblings = await this.foldersUnder(parentId);

    await this.engine.record('folders', id, 'upsert', {
      name: name.trim(),
      parent_id: parentId,
      position: siblings.length,
    });

    return id;
  }

  async renameFolder(id: string, name: string): Promise<void> {
    await this.engine.record('folders', id, 'upsert', { name: name.trim() });
  }

  /** Refuses to hang a folder off its own descendant, which would strand the whole subtree. */
  async moveFolder(id: string, parentId: string | null): Promise<void> {
    const folders = await this.db.folders.filter((row) => row.deleted_at === null).toArray();

    if (wouldCycle(folders, id, parentId)) {
      throw new Error('A folder cannot be moved inside itself.');
    }

    await this.engine.record('folders', id, 'upsert', { parent_id: parentId });
  }

  /**
   * Deleting a folder never hard-deletes a song. The caller has already asked which of the two
   * outcomes the user wants, because silently archiving twelve songs is not recoverable by
   * anyone who did not expect it.
   */
  async deleteFolder(id: string, songs: 'move-to-parent' | 'archive'): Promise<void> {
    const folder = await this.db.folders.get(id);
    const contained = await this.db.songs
      .where('folder_id').equals(id)
      .filter((row) => row.deleted_at === null)
      .toArray();

    for (const song of contained) {
      if (songs === 'archive') {
        await this.engine.record('songs', song.id, 'upsert', { archived: 1, folder_id: folder?.parent_id ?? null });
      } else {
        await this.engine.record('songs', song.id, 'upsert', { folder_id: folder?.parent_id ?? null });
      }
    }

    for (const child of await this.foldersUnder(id)) {
      await this.engine.record('folders', child.id, 'upsert', { parent_id: folder?.parent_id ?? null });
    }

    await this.engine.record('folders', id, 'delete', {});
  }

  async addPlacement(songId: string, folderId: string): Promise<void> {
    const existing = await this.db.song_placements
      .where('song_id').equals(songId)
      .filter((row) => row.folder_id === folderId)
      .first();

    await this.engine.record('song_placements', existing?.id ?? uuidv7(), 'upsert', {
      song_id: songId,
      folder_id: folderId,
    });
  }

  async removePlacement(placement: SongPlacement): Promise<void> {
    await this.engine.record('song_placements', placement.id, 'delete', {});
  }

  private async foldersUnder(parentId: string | null): Promise<Folder[]> {
    return this.db.folders
      .filter((row) => row.deleted_at === null && row.parent_id === parentId)
      .toArray();
  }
}

/**
 * True when re-parenting `id` under `parentId` would close a loop — including the degenerate
 * case of dropping a folder onto itself.
 */
export function wouldCycle(folders: Pick<Folder, 'id' | 'parent_id'>[], id: string, parentId: string | null): boolean {
  if (parentId === null) {
    return false;
  }

  if (parentId === id) {
    return true;
  }

  const parents = new Map(folders.map((folder) => [folder.id, folder.parent_id]));

  let walker: string | null | undefined = parentId;
  const seen = new Set<string>();

  while (walker != null && ! seen.has(walker)) {
    if (walker === id) {
      return true;
    }

    seen.add(walker);
    walker = parents.get(walker) ?? null;
  }

  return false;
}

/** Songs a folder shows: its home songs plus anything placed there as a second home. */
export function songsInFolder(
  songs: Song[],
  placements: SongPlacement[],
  folderId: string | null,
): Song[] {
  const placed = new Set(
    placements.filter((placement) => placement.folder_id === folderId && placement.deleted_at === null)
      .map((placement) => placement.song_id),
  );

  return songs.filter((song) => song.folder_id === folderId || placed.has(song.id));
}
