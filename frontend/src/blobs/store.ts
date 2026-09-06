import type { BlobRecord, WorkspaceDb } from '../db/schema';

/**
 * The local file store for sheet PDFs.
 *
 * Files live in the origin private file system where it exists — it holds hundreds of megabytes
 * without the structured-clone cost of putting blobs through IndexedDB — and in a Dexie table
 * everywhere else. Either way the *record* of what is held lives in `blobs`, so the pin policy
 * and the eviction pass work the same on both.
 */

export type PinReason = BlobRecord['pin_reason'];

/** Opportunistic downloads are capped; pinned files are not, because the user asked for them. */
export const OPPORTUNISTIC_BUDGET = 500 * 1024 * 1024;

interface Opfs {
  getDirectoryHandle: (name: string, options?: { create?: boolean }) => Promise<Opfs>;
  getFileHandle: (name: string, options?: { create?: boolean }) => Promise<{
    getFile: () => Promise<File>;
    createWritable: () => Promise<{ write: (data: Blob) => Promise<void>; close: () => Promise<void> }>;
  }>;
  removeEntry: (name: string) => Promise<void>;
}

export class BlobStore {
  constructor(
    private readonly db: WorkspaceDb,
    private readonly workspaceId: string,
  ) {}

  async put(sheetId: string, sha256: string, bytes: Blob, pin: PinReason): Promise<void> {
    const directory = await this.directory();

    if (directory === null) {
      await this.db.files.put({ sheet_id: sheetId, bytes });
    } else {
      const handle = await directory.getFileHandle(sheetId, { create: true });
      const writable = await handle.createWritable();
      await writable.write(bytes);
      await writable.close();
    }

    await this.db.blobs.put({
      sheet_id: sheetId,
      sha256,
      size: bytes.size,
      pin_reason: pin,
      cached_at: new Date().toISOString(),
    });
  }

  async get(sheetId: string): Promise<Blob | null> {
    const record = await this.db.blobs.get(sheetId);

    if (record === undefined) {
      return null;
    }

    const directory = await this.directory();

    if (directory === null) {
      return (await this.db.files.get(sheetId))?.bytes ?? null;
    }

    try {
      const handle = await directory.getFileHandle(sheetId);

      // Touching cached_at on read is what makes eviction least-recently-*used* rather than
      // least-recently-downloaded.
      await this.db.blobs.update(sheetId, { cached_at: new Date().toISOString() });

      return await handle.getFile();
    } catch {
      // The record outlived the file — a cleared origin, a failed write. Forget the record so
      // the queue downloads it again.
      await this.db.blobs.delete(sheetId);

      return null;
    }
  }

  async has(sheetId: string, sha256: string | null): Promise<boolean> {
    const record = await this.db.blobs.get(sheetId);

    // A different hash means the file was replaced upstream: what is cached is the wrong bytes.
    return record !== undefined && (sha256 === null || record.sha256 === sha256);
  }

  async remove(sheetId: string): Promise<void> {
    const directory = await this.directory();

    if (directory === null) {
      await this.db.files.delete(sheetId);
    } else {
      await directory.removeEntry(sheetId).catch(() => undefined);
    }

    await this.db.blobs.delete(sheetId);
  }

  async pin(sheetId: string, pin: PinReason): Promise<void> {
    await this.db.blobs.update(sheetId, { pin_reason: pin });
  }

  /** Drops the least recently used opportunistic files until the cache is inside its budget. */
  async evict(budget = OPPORTUNISTIC_BUDGET): Promise<number> {
    const opportunistic = (await this.db.blobs.toArray())
      .filter((record) => record.pin_reason === 'opportunistic')
      .sort((a, b) => a.cached_at.localeCompare(b.cached_at));

    let total = opportunistic.reduce((sum, record) => sum + record.size, 0);
    let dropped = 0;

    for (const record of opportunistic) {
      if (total <= budget) {
        break;
      }

      await this.remove(record.sheet_id);
      total -= record.size;
      dropped++;
    }

    return dropped;
  }

  private async directory(): Promise<Opfs | null> {
    try {
      const storage = navigator.storage as unknown as { getDirectory?: () => Promise<Opfs> };

      if (storage.getDirectory === undefined) {
        return null;
      }

      const root = await storage.getDirectory();

      return await root.getDirectoryHandle(`aurum-${this.workspaceId}`, { create: true });
    } catch {
      return null;
    }
  }
}

/** Hex sha256 of the bytes, which is both the integrity check and the object key. */
export async function hashOf(bytes: Blob): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', await bytes.arrayBuffer());

  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}
