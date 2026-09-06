export const API_URL = (import.meta.env.VITE_API_URL as string | undefined) ?? 'http://localhost:8080';

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
    readonly details?: Record<string, unknown>,
  ) {
    super(message);
    this.name = 'ApiError';
  }

  /** Transient failures the outbox should retry; anything else is parked. */
  get isRetryable(): boolean {
    return this.status >= 500 || this.status === 429;
  }
}

/**
 * The access token lives in memory only. Persisting it would put a bearer credential somewhere
 * a script can read it, and it is worth so little — fifteen minutes — that recovering it from
 * the refresh cookie on reload is cheaper than protecting it.
 */
let accessToken: string | null = null;
let refreshInFlight: Promise<boolean> | null = null;

export function setAccessToken(token: string | null): void {
  accessToken = token;
}

export function hasAccessToken(): boolean {
  return accessToken !== null;
}

/**
 * The current token, for the one caller that cannot use `api()`: a WebSocket handshake, which
 * a browser will not let us add headers to, so the token goes in the query string instead. It
 * lives fifteen minutes and the socket lives two, which is the trade that makes that acceptable.
 */
export function currentAccessToken(): string | null {
  return accessToken;
}

/**
 * Why a session could not be restored. "Offline" and "signed out" look the same to a fetch that
 * fails, and they must not look the same to the app: one is a reason to show a sign-in screen,
 * the other is a reason to show the library that is already on the device.
 */
export type RestoreResult = 'ok' | 'signed-out' | 'offline';

async function refresh(): Promise<boolean> {
  // A single in-flight refresh, shared by every caller. Without this, a burst of parallel
  // requests hitting a just-expired token would each rotate the refresh cookie — and rotation
  // treats a second use of the same token as theft, which would revoke the whole family.
  refreshInFlight ??= (async () => {
    try {
      const response = await fetch(`${API_URL}/api/v1/auth/refresh`, {
        method: 'POST',
        credentials: 'include',
      });

      if (!response.ok) {
        accessToken = null;
        return false;
      }

      const body = (await response.json()) as { access_token: string };
      accessToken = body.access_token;
      return true;
    } finally {
      refreshInFlight = null;
    }
  })();

  return refreshInFlight;
}

/** The same call, but saying which of the two failures happened. */
async function restore(): Promise<RestoreResult> {
  try {
    return await refresh() ? 'ok' : 'signed-out';
  } catch {
    // The request never reached the server. That says nothing about the session.
    return 'offline';
  }
}

export async function api<T>(
  path: string,
  init: RequestInit & { retryOnExpiry?: boolean } = {},
): Promise<T> {
  const { retryOnExpiry = true, ...request } = init;

  const headers = new Headers(request.headers);
  headers.set('Accept', 'application/json');

  if (request.body !== undefined && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }

  if (accessToken !== null) {
    headers.set('Authorization', `Bearer ${accessToken}`);
  }

  const response = await fetch(`${API_URL}/api/v1${path}`, {
    ...request,
    headers,
    credentials: 'include',
  });

  if (response.status === 204) {
    return undefined as T;
  }

  const body = (await response.json().catch(() => null)) as
    | { error?: { code: string; message: string; details?: Record<string, unknown> } }
    | null;

  if (!response.ok) {
    const error = body?.error;

    // A token that expired, or a device that started offline and has none at all: both are
    // fixed by asking for a new one, and both must not surface as an error to the caller.
    if (response.status === 401 && retryOnExpiry && (error?.code === 'token_expired' || accessToken === null)) {
      if (await refresh()) {
        return api<T>(path, { ...init, retryOnExpiry: false });
      }
    }

    throw new ApiError(
      response.status,
      error?.code ?? 'unknown',
      error?.message ?? response.statusText,
      error?.details,
    );
  }

  return body as T;
}

export interface Workspace {
  id: string;
  name: string;
  kind: 'personal' | 'band';
  role: 'owner' | 'editor' | 'viewer';
  created_at: string;
  updated_at: string;
}

export interface Account {
  id: string;
  email: string;
  display_name: string;
  totp: { enrolled: boolean; required: boolean; recovery_codes_remaining: number };
  workspaces: Workspace[];
}

export const auth = {
  async register(email: string, displayName: string, password: string) {
    const result = await api<{ access_token: string }>('/auth/register', {
      method: 'POST',
      body: JSON.stringify({ email, display_name: displayName, password }),
    });
    setAccessToken(result.access_token);
    return result;
  },

  async login(email: string, password: string) {
    const result = await api<{
      access_token?: string;
      totp_required?: boolean;
      challenge_id?: string;
      totp_enrolment_required?: boolean;
    }>('/auth/login', { method: 'POST', body: JSON.stringify({ email, password }) });

    if (result.access_token) {
      setAccessToken(result.access_token);
    }

    return result;
  },

  async completeTotp(challengeId: string, code: string) {
    const result = await api<{ access_token: string }>('/auth/login/totp', {
      method: 'POST',
      body: JSON.stringify({ challenge_id: challengeId, code }),
    });
    setAccessToken(result.access_token);
    return result;
  },

  forgotPassword: (email: string) =>
    api<{ sent: boolean }>('/auth/password/forgot', { method: 'POST', body: JSON.stringify({ email }) }),

  resetPassword: (token: string, password: string) =>
    api<{ reset: boolean }>('/auth/password/reset', { method: 'POST', body: JSON.stringify({ token, password }) }),

  /** Restores a session on app start from the httpOnly refresh cookie. */
  restore,

  async logout() {
    await api('/auth/logout', { method: 'POST' });
    setAccessToken(null);
  },
};

const ACCOUNT_KEY = 'aurum.account';

/**
 * The last account this device saw.
 *
 * Kept so that a launch with no connection reaches the library rather than a sign-in screen:
 * the session is still valid, the songs are still here, and there is nothing to sign in to.
 */
export function cachedAccount(): Account | null {
  try {
    const stored = localStorage.getItem(ACCOUNT_KEY);

    return stored === null ? null : (JSON.parse(stored) as Account);
  } catch {
    return null;
  }
}

export function forgetCachedAccount(): void {
  try {
    localStorage.removeItem(ACCOUNT_KEY);
  } catch {
    // Storage blocked; there was nothing cached to forget.
  }
}

export const account = {
  async me(): Promise<Account> {
    const me = await api<Account>('/account');

    try {
      localStorage.setItem(ACCOUNT_KEY, JSON.stringify(me));
    } catch {
      // Without storage the app works online only, which the About page explains.
    }

    return me;
  },

  totp: {
    enrol: () => api<{ secret: string; provisioning_uri: string; digits: number; period: number }>(
      '/account/totp/enrol',
      { method: 'POST' },
    ),
    confirm: (code: string) => api<{ enrolled: boolean; recovery_codes: string[]; notice: string }>(
      '/account/totp/confirm',
      { method: 'POST', body: JSON.stringify({ code }) },
    ),
    disable: (password: string, code: string) => api<void>('/account/totp', {
      method: 'DELETE',
      body: JSON.stringify({ password, code }),
    }),
    newRecoveryCodes: (password: string, code: string) => api<{ recovery_codes: string[] }>(
      '/account/totp/recovery-codes',
      { method: 'POST', body: JSON.stringify({ password, code }) },
    ),
  },
};

export interface Member {
  user_id: string;
  email: string;
  display_name: string;
  role: 'owner' | 'editor' | 'viewer';
}

export interface Invite {
  id: string;
  email: string;
  role: 'editor' | 'viewer';
  expires_at: string;
  created_at: string;
  not_sent?: boolean;
}

export const members = {
  list: (workspaceId: string) => api<{ members: Member[] }>(`/workspaces/${workspaceId}/members`),

  setRole: (workspaceId: string, userId: string, role: Member['role']) =>
    api<{ members: Member[] }>(`/workspaces/${workspaceId}/members/${userId}`, {
      method: 'PATCH',
      body: JSON.stringify({ role }),
    }),

  remove: (workspaceId: string, userId: string) =>
    api<{ members: Member[] }>(`/workspaces/${workspaceId}/members/${userId}`, { method: 'DELETE' }),
};

export const invites = {
  list: (workspaceId: string) => api<{ invites: Invite[] }>(`/workspaces/${workspaceId}/invites`),

  create: (workspaceId: string, email: string, role: Invite['role']) =>
    api<{ invite: Invite; link: string; pending: Invite[] }>(`/workspaces/${workspaceId}/invites`, {
      method: 'POST',
      body: JSON.stringify({ email, role }),
    }),

  revoke: (workspaceId: string, inviteId: string) =>
    api<{ invites: Invite[] }>(`/workspaces/${workspaceId}/invites/${inviteId}`, { method: 'DELETE' }),

  preview: (token: string) => api<{
    invite: { workspace_name: string; email: string; role: string; expires_at: string; used: boolean; expired: boolean };
  }>(`/invites/${token}`),

  accept: (token: string) => api<{ workspace: Workspace; role: string }>('/invites/accept', {
    method: 'POST',
    body: JSON.stringify({ token }),
  }),
};

export const workspaces = {
  list: () => api<{ workspaces: Workspace[] }>('/workspaces'),
  create: (name: string, id?: string) =>
    api<{ workspace: Workspace }>('/workspaces', {
      method: 'POST',
      body: JSON.stringify({ name, id }),
    }),
};
