import { createHash } from 'node:crypto';
import { API } from './env.mjs';
import { freshCode, totp } from './totp.mjs';

/**
 * A minimal API client for provisioning, not for testing.
 *
 * The specs exercise the app through the browser; this exists only so a run can create the world
 * it needs — an account, a song, a set, a sheet — without depending on whatever happens to be in
 * a developer's database. Every run starts from nothing, which is what makes the suite portable
 * across implementations of the server.
 */
export class Api {
  #token = null;
  #cookies = new Map();

  constructor(base = API) {
    this.base = base;
  }

  async call(path, { method = 'GET', body, raw = false } = {}) {
    const headers = { Accept: 'application/json' };

    if (body !== undefined) {
      headers['Content-Type'] = 'application/json';
    }

    if (this.#token !== null) {
      headers.Authorization = `Bearer ${this.#token}`;
    }

    if (this.#cookies.size > 0) {
      headers.Cookie = [...this.#cookies].map(([name, value]) => `${name}=${value}`).join('; ');
    }

    const response = await fetch(`${this.base}/api/v1${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });

    for (const line of response.headers.getSetCookie?.() ?? []) {
      const [pair] = line.split(';');
      const [name, value] = pair.split('=');
      this.#cookies.set(name.trim(), value);
    }

    if (raw) {
      return response;
    }

    const text = await response.text();
    const parsed = text === '' ? null : JSON.parse(text);

    if (! response.ok) {
      throw new Error(`${method} ${path} → ${response.status} ${text.slice(0, 300)}`);
    }

    return parsed;
  }

  async register(email, displayName, password) {
    const result = await this.call('/auth/register', {
      method: 'POST',
      body: { email, display_name: displayName, password },
    });

    this.#token = result.access_token;

    return result;
  }

  /** Signs in as an existing account, answering the two-factor challenge with its own secret. */
  async login(email, password, secret) {
    const start = await this.call('/auth/login', { method: 'POST', body: { email, password } });

    if (start.access_token !== undefined) {
      this.#token = start.access_token;

      return start;
    }

    const finished = await this.call('/auth/login/totp', {
      method: 'POST',
      body: { challenge_id: start.challenge_id, code: await freshCode(secret) },
    });

    this.#token = finished.access_token;

    return finished;
  }

  async me() {
    return this.call('/account');
  }

  async enrolTotp() {
    return this.call('/account/totp/enrol', { method: 'POST' });
  }

  async confirmTotp(code) {
    return this.call('/account/totp/confirm', { method: 'POST', body: { code } });
  }

  async push(workspaceId, ops) {
    return this.call(`/workspaces/${workspaceId}/sync/push`, { method: 'POST', body: { ops } });
  }

  /**
   * The whole three-step upload: ask for a plan, PUT the parts straight at the object store, then
   * tell the API the bytes are there. Mirrors what the blob queue does in the app.
   */
  async uploadSheet(workspaceId, sheetId, bytes, { pageCount = 1 } = {}) {
    const sha256 = createHash('sha256').update(bytes).digest('hex');

    const plan = await this.call(`/workspaces/${workspaceId}/sheets/${sheetId}/upload-url`, {
      method: 'POST',
      body: { sha256, size: bytes.length },
    });

    const parts = [];

    if (! plan.already_stored) {
      for (const part of plan.parts ?? []) {
        const slice = bytes.subarray(part.offset, part.offset + part.length);
        const response = await fetch(part.url, { method: 'PUT', body: slice });

        if (! response.ok) {
          throw new Error(`part ${part.part_number} → ${response.status}`);
        }

        parts.push({
          part_number: part.part_number,
          etag: (response.headers.get('ETag') ?? '').replaceAll('"', ''),
        });
      }
    }

    await this.call(`/workspaces/${workspaceId}/sheets/${sheetId}/complete`, {
      method: 'POST',
      body: {
        sha256,
        size: bytes.length,
        page_count: pageCount,
        ...(plan.already_stored ? {} : { upload_id: plan.upload_id, parts }),
      },
    });

    return sha256;
  }
}

/** UUIDv7, so seeded ids sort by creation time exactly as the app's own do. */
export function uuidv7() {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);

  const stamp = BigInt(Date.now());

  for (let index = 0; index < 6; index++) {
    bytes[index] = Number((stamp >> BigInt(8 * (5 - index))) & 0xffn);
  }

  bytes[6] = (bytes[6] & 0x0f) | 0x70;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;

  const hex = [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');

  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
