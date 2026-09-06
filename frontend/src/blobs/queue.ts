import { api } from '../api/client';
import type { WorkspaceDb } from '../db/schema';
import { BlobStore } from './store';
import { asSoleWorker } from '../app/locks';

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
  /** Set when a pinned download would not fit. Nothing pinned is ever thrown away for it. */
  full: StorageFull | null;
}

/**
 * A pinned file that will not fit on this device.
 *
 * The device is out of room and the app will not choose for the user: a pinned file is
 * something somebody asked for, so the app names what could not be kept and lets them decide
 * which pin to release (business rule 10).
 */
export interface StorageFull {
  sheetId: string;
  songTitle: string;
  size: number;
}

export class BlobQueue {
  private running = false;
  private full: StorageFull | null = null;

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

    // One window moves files for the device; the others read the same local result.
    return asSoleWorker(`aurum-files-${this.workspaceId}`, () => this.pass(wanted), await this.state());
  }

  private async pass(wanted: Set<string>): Promise<QueueState> {
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
      full: this.full,
    };
  }

  /** Called once the user has released a pin, so the next pass is allowed to try again. */
  clearFull(): void {
    this.full = null;
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
    // Each pass decides for itself: a pin released since the last one may have made room.
    this.full = null;

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

        const bytes = await response.blob();

        try {
          await this.store.put(sheetId, sheet.sha256, bytes, 'pinned');
        } catch (error) {
          if (! isOutOfSpace(error)) {
            throw error;
          }

          // Out of room. Stop — every file after this one would fail the same way — and say
          // which file it was, so the user can decide what to release.
          this.full = {
            sheetId,
            songTitle: (await this.db.songs.get(sheet.song_id))?.title ?? 'A sheet',
            size: bytes.size,
          };

          return;
        }
      } catch {
        // Offline, or the object is not there yet. The sheet shows as not downloaded and the
        // next pass will try again.
      }
    }
  }

  /** Downloads one sheet because the user is looking at it right now. */
  async fetchNow(sheetId: string): Promise<Blob | null> {
    const sheet = await this.db.sheets.get(sheetId);

    if (sheet === undefined) {
      return null;
    }

    // The local copy first, and before the hash is even considered: the device that attached
    // the file holds it while `sha256` is still null — the server sets that on completion and
    // it arrives on a later pull. Reading "not downloaded" on the one device with the only copy
    // is exactly the failure acceptance criterion 4 is about.
    const cached = await this.store.get(sheetId);

    if (cached !== null && await this.store.has(sheetId, sheet.sha256)) {
      return cached;
    }

    if (sheet.sha256 === null) {
      // Nothing here, and nothing to ask the server for yet: the upload has not finished.
      return null;
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

/**
 * Whether a failed write means the device is full. Browsers disagree on the shape of it: a
 * DOMException on IndexedDB, a plain error out of the file system on some Safari builds.
 */
export function isOutOfSpace(error: unknown): boolean {
  const name = (error as { name?: string } | null)?.name ?? '';
  const message = error instanceof Error ? error.message.toLowerCase() : '';

  return name === 'QuotaExceededError'
    || name === 'NS_ERROR_FILE_NO_DEVICE_SPACE'
    || message.includes('quota')
    || message.includes('no space');
}
