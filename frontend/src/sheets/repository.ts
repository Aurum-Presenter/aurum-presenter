import { BlobStore, hashOf } from '../blobs/store';
import type { Sheet, WorkspaceDb } from '../db/schema';
import { uuidv7 } from '../db/uuid';
import type { SyncEngine } from '../sync/engine';
import type { Part } from './selection';

/**
 * Attaching, replacing and describing sheets.
 *
 * A sheet is two things that travel separately: a row, which syncs with everything else, and a
 * file, which goes through the blob queue. The row is written first and the file is stored
 * locally straight away, so an attach made on a plane is complete from the user's point of view
 * before the aircraft has landed.
 */

export const MAX_SHEET_BYTES = 50 * 1024 * 1024;
export const WARN_SHEET_BYTES = 10 * 1024 * 1024;

export const ACCEPTED = ['application/pdf', 'image/png', 'image/jpeg', 'image/heic'];

export interface SheetInput {
  key: string | null;
  part: Part;
  label: string | null;
  arrangementId: string | null;
}

export class SheetsRepository {
  constructor(
    private readonly db: WorkspaceDb,
    private readonly engine: SyncEngine,
    private readonly store: BlobStore,
  ) {}

  /** Rejects what the viewer could not open, rather than storing it and failing later. */
  static problemWith(file: File): string | null {
    if (! ACCEPTED.includes(file.type)) {
      return `${file.name} is a ${file.type === '' ? 'file of unknown type' : file.type}. Attach a PDF or an image.`;
    }

    if (file.size > MAX_SHEET_BYTES) {
      return `${file.name} is ${Math.round(file.size / 1024 / 1024)} MB. The limit is 50 MB.`;
    }

    return null;
  }

  async attach(songId: string, file: File, input: SheetInput): Promise<string> {
    const id = uuidv7();
    const existing = await this.db.sheets.where('song_id').equals(songId).count();

    await this.engine.record('sheets', id, 'upsert', {
      song_id: songId,
      sheet_key: input.key,
      part: input.part,
      label: input.label,
      arrangement_id: input.arrangementId,
      filename: file.name,
      mime_type: file.type,
      position: existing,
    });

    await this.storeAndQueue(id, file);

    return id;
  }

  /** Business rule 3: same bytes, nothing happens; different bytes, every device re-downloads. */
  async replace(sheetId: string, file: File): Promise<void> {
    const sheet = await this.db.sheets.get(sheetId);
    const sha256 = await hashOf(file);

    if (sheet?.sha256 === sha256) {
      return;
    }

    await this.engine.record('sheets', sheetId, 'upsert', {
      filename: file.name,
      mime_type: file.type,
    });

    await this.storeAndQueue(sheetId, file, sha256);
  }

  async update(sheetId: string, changes: Partial<Record<keyof Sheet, unknown>>): Promise<void> {
    await this.engine.record('sheets', sheetId, 'upsert', changes);
  }

  async remove(sheetId: string): Promise<void> {
    await this.engine.record('sheets', sheetId, 'delete', {});
    await this.store.remove(sheetId);
    await this.db.uploads.delete(sheetId);
  }

  private async storeAndQueue(sheetId: string, file: File, hash?: string): Promise<void> {
    const sha256 = hash ?? await hashOf(file);

    // Pinned, not opportunistic: the device that made the file is the only one that has it
    // until the queue drains, so eviction must not be allowed anywhere near it.
    await this.store.put(sheetId, sha256, file, 'pinned');

    await this.db.uploads.put({
      sheet_id: sheetId,
      sha256,
      size: file.size,
      filename: file.name,
      page_count: await pageCount(file),
      attempts: 0,
      last_error: null,
      queued_at: new Date().toISOString(),
    });
  }
}

/** Page count comes from the file itself; an image is one page by definition. */
async function pageCount(file: File): Promise<number | null> {
  if (file.type !== 'application/pdf') {
    return 1;
  }

  try {
    const pdfjs = await import('pdfjs-dist');
    const document = await pdfjs.getDocument({ data: await file.arrayBuffer() }).promise;
    const pages = document.numPages;
    await document.destroy();

    return pages;
  } catch {
    // A password-protected or damaged PDF still attaches; the viewer explains it later.
    return null;
  }
}
