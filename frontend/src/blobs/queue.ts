import { api } from '../api/client';
import type { WorkspaceDb } from '../db/schema';
import { BlobStore } from './store';

/**
 * The blob queue: sheet files moving between the device and the object store.
 *
 * It is deliberately separate from the metadata sync. A 40 MB piano score must never hold up a
 * key change from reaching the rest of the band, so this runs beside the outbox and its
 * failures never surface as sync failures.
 */

interface UploadPlan {
  already_stored: boolean;
  key: string;
  upload_id?: string;
  part_size?: number;
  parts?: { part_number: number; offset: number; length: number; url: string }[];
}

export interface QueueState {
  uploading: number;
  downloading: number;
  failed: number;
}

export class BlobQueue {
  private running = false;

  constructor(
    private readonly db: WorkspaceDb,
    private readonly store: BlobStore,
    private readonly workspaceId: string,
  ) {}

  /**
   * One pass: push everything the server does not have, then pull everything this device is
   * supposed to be keeping. Uploads go first — a file that exists only here is the only copy.
   */
  async run(wanted: Set<string>): Promise<QueueState> {
    if (this.running || ! navigator.onLine) {
      return this.state();
    }

    this.running = true;

    try {
      // Business rule 11: once this device is keeping something deliberately, ask the browser
      // not to clear the origin under pressure.
      if (wanted.size > 0) {
        await BlobStore.requestPersistence();
      }

      await this.drainUploads();
      await this.fetchWanted(wanted);
      await this.store.evict();
    } finally {
      this.running = false;
    }

    return this.state();
  }

  async state(): Promise<QueueState> {
    const uploads = await this.db.uploads.toArray();

    return {
      uploading: uploads.filter((upload) => upload.last_error === null).length,
      downloading: 0,
      failed: uploads.filter((upload) => upload.last_error !== null).length,
    };
  }

  private async drainUploads(): Promise<void> {
    for (const upload of await this.db.uploads.orderBy('queued_at').toArray()) {
      const bytes = await this.store.get(upload.sheet_id);

      if (bytes === null) {
        // The local copy is gone; there is nothing left to upload and nothing to be done.
        await this.db.uploads.delete(upload.sheet_id);
        continue;
      }

      try {
        const plan = await api<UploadPlan>(
          `/workspaces/${this.workspaceId}/sheets/${upload.sheet_id}/upload-url`,
          { method: 'POST', body: JSON.stringify({ sha256: upload.sha256, size: upload.size }) },
        );

        const parts: { part_number: number; etag: string }[] = [];

        // Content addressing means identical bytes are already at the key: an upload that has
        // happened once, from any device, never happens again.
        if (! plan.already_stored && plan.parts !== undefined) {
          for (const part of plan.parts) {
            const slice = bytes.slice(part.offset, part.offset + part.length);
            const response = await fetch(part.url, { method: 'PUT', body: slice });

            if (! response.ok) {
              throw new Error(`Part ${part.part_number} failed with ${response.status}`);
            }

            parts.push({
              part_number: part.part_number,
              etag: (response.headers.get('ETag') ?? '').replaceAll('"', ''),
            });
          }
        }

        await api(`/workspaces/${this.workspaceId}/sheets/${upload.sheet_id}/complete`, {
          method: 'POST',
          body: JSON.stringify({
            sha256: upload.sha256,
            size: upload.size,
            page_count: upload.page_count,
            ...(plan.already_stored ? {} : { upload_id: plan.upload_id, parts }),
          }),
        });

        await this.db.uploads.delete(upload.sheet_id);
      } catch (error) {
        // The file stays on the device and the entry stays in the queue: the next pass tries
        // again, and nothing the user made is lost in the meantime.
        await this.db.uploads.update(upload.sheet_id, {
          attempts: upload.attempts + 1,
          last_error: error instanceof Error ? error.message : 'Upload failed',
        });
      }
    }
  }

  private async fetchWanted(wanted: Set<string>): Promise<void> {
    for (const sheetId of wanted) {
      const sheet = await this.db.sheets.get(sheetId);

      if (sheet === undefined || sheet.sha256 === null || sheet.deleted_at !== null) {
        continue;
      }

      if (await this.store.has(sheetId, sheet.sha256)) {
        await this.store.pin(sheetId, 'pinned');
        continue;
      }

      try {
        const { url } = await api<{ url: string }>(`/workspaces/${this.workspaceId}/sheets/${sheetId}/url`);
        const response = await fetch(url);

        if (! response.ok) {
          continue;
        }

        await this.store.put(sheetId, sheet.sha256, await response.blob(), 'pinned');
      } catch {
        // Offline, or the object is not there yet. The sheet shows as not downloaded and the
        // next pass will try again.
      }
    }
  }

  /** Downloads one sheet because the user is looking at it right now. */
  async fetchNow(sheetId: string): Promise<Blob | null> {
    const sheet = await this.db.sheets.get(sheetId);

    if (sheet === undefined || sheet.sha256 === null) {
      return null;
    }

    const cached = await this.store.get(sheetId);

    if (cached !== null && await this.store.has(sheetId, sheet.sha256)) {
      return cached;
    }

    const { url } = await api<{ url: string }>(`/workspaces/${this.workspaceId}/sheets/${sheetId}/url`);
    const response = await fetch(url);

    if (! response.ok) {
      throw new Error('The file could not be downloaded.');
    }

    const bytes = await response.blob();
    await this.store.put(sheetId, sheet.sha256, bytes, 'opportunistic');

    return bytes;
  }
}
