import { api } from '../api/client';
import { hashOf } from '../blobs/store';
import type { WorkspaceDb } from '../db/schema';

/**
 * The image behind an audience slide.
 *
 * It travels the way sheets do — content-addressed, private, cached on the device — because it
 * is shown in the one place that must not depend on a network. A background that is not cached
 * falls back to the theme's colour (presenter-output business rule 2); it never shows white,
 * and it never shows a broken image to a room.
 */
export interface UploadedBackground {
  sha256: string;
  contentType: string;
}

export async function uploadBackground(db: WorkspaceDb, workspaceId: string, file: File): Promise<UploadedBackground> {
  const sha256 = await hashOf(file);

  const plan = await api<{ already_stored: boolean; upload_id?: string; url?: string }>(
    `/workspaces/${workspaceId}/assets/upload-url`,
    { method: 'POST', body: JSON.stringify({ sha256, size: file.size, content_type: file.type }) },
  );

  let etag = '';

  if (! plan.already_stored && plan.url !== undefined) {
    const response = await fetch(plan.url, { method: 'PUT', body: file });

    if (! response.ok) {
      throw new Error('The image could not be uploaded.');
    }

    etag = (response.headers.get('ETag') ?? '').replaceAll('"', '');
  }

  await api(`/workspaces/${workspaceId}/assets/complete`, {
    method: 'POST',
    body: JSON.stringify({
      sha256,
      content_type: file.type,
      size: file.size,
      ...(plan.already_stored ? {} : { upload_id: plan.upload_id, etag }),
    }),
  });

  // Keep the bytes here too, so the device that uploaded it can present with no network.
  await db.files.put({ sheet_id: backgroundKey(sha256), bytes: file });

  return { sha256, contentType: file.type };
}

/**
 * A URL the audience window can paint from, preferring the copy on this device. Returns null
 * when there is neither a cached copy nor a connection — the caller then uses the colour.
 */
export async function backgroundUrl(db: WorkspaceDb, workspaceId: string, sha256: string): Promise<string | null> {
  const cached = await db.files.get(backgroundKey(sha256));

  if (cached !== undefined) {
    return URL.createObjectURL(cached.bytes);
  }

  try {
    const { url } = await api<{ url: string }>(`/workspaces/${workspaceId}/assets/${sha256}/url`);
    const response = await fetch(url);

    if (! response.ok) {
      return null;
    }

    const bytes = await response.blob();
    await db.files.put({ sheet_id: backgroundKey(sha256), bytes });

    return URL.createObjectURL(bytes);
  } catch {
    return null;
  }
}

/**
 * Backgrounds share the local file table with sheets, under a prefixed key: they are the same
 * kind of thing — bytes this device holds so a screen can be painted with no network.
 */
function backgroundKey(sha256: string): string {
  return `theme-background:${sha256}`;
}
