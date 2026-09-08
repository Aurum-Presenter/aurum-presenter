-- Control tier: identity and tenancy. Opened on every request.
-- Types follow the SQLite mapping in the SQLite-backend change request:
--   uuid -> TEXT (UUIDv7), timestamptz -> TEXT (ISO-8601 UTC), citext -> TEXT COLLATE NOCASE.

CREATE TABLE users (
    id                TEXT PRIMARY KEY,
    email             TEXT COLLATE NOCASE NOT NULL UNIQUE,
    display_name      TEXT NOT NULL,
    email_verified_at TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);

CREATE TABLE credentials (
    user_id       TEXT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    password_hash TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

-- The secret is encrypted at rest with a key from the environment, so a database read alone
-- does not yield a working second factor.
CREATE TABLE totp_secrets (
    user_id            TEXT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    secret_encrypted   BLOB NOT NULL,
    last_accepted_step INTEGER,
    confirmed_at       TEXT,
    created_at         TEXT NOT NULL
);

CREATE TABLE recovery_codes (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    code_hash  TEXT NOT NULL,
    used_at    TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX recovery_codes_user ON recovery_codes (user_id);

-- family_id groups a rotating chain of refresh tokens. Presenting a token twice revokes the
-- whole family: the standard reuse-detection response to a stolen cookie.
CREATE TABLE sessions (
    id                 TEXT PRIMARY KEY,
    user_id            TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    family_id          TEXT NOT NULL,
    refresh_token_hash TEXT NOT NULL UNIQUE,
    user_agent         TEXT,
    created_at         TEXT NOT NULL,
    expires_at         TEXT NOT NULL,
    revoked_at         TEXT,
    replaced_by        TEXT
);
CREATE INDEX sessions_user ON sessions (user_id);
CREATE INDEX sessions_family ON sessions (family_id);

-- Issued after a correct password when TOTP is enrolled; exchanged for a session by a code.
CREATE TABLE totp_challenges (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    attempts   INTEGER NOT NULL DEFAULT 0,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE password_resets (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    used_at    TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE login_attempts (
    id         TEXT PRIMARY KEY,
    email      TEXT COLLATE NOCASE NOT NULL,
    ip         TEXT,
    successful INTEGER NOT NULL DEFAULT 0,
    at         TEXT NOT NULL
);
CREATE INDEX login_attempts_email_at ON login_attempts (email, at);
CREATE INDEX login_attempts_ip_at ON login_attempts (ip, at);

CREATE TABLE workspaces (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('personal', 'band')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE TABLE memberships (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    role         TEXT NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    UNIQUE (user_id, workspace_id)
);
CREATE INDEX memberships_workspace ON memberships (workspace_id);

CREATE TABLE invites (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    email        TEXT COLLATE NOCASE NOT NULL,
    role         TEXT NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
    token_hash   TEXT NOT NULL UNIQUE,
    invited_by   TEXT REFERENCES users (id) ON DELETE SET NULL,
    expires_at   TEXT NOT NULL,
    accepted_at  TEXT,
    created_at   TEXT NOT NULL
);
CREATE UNIQUE INDEX invites_pending ON invites (workspace_id, email) WHERE accepted_at IS NULL;

CREATE TABLE mail_queue (
    id           TEXT PRIMARY KEY,
    recipient    TEXT NOT NULL,
    subject      TEXT NOT NULL,
    body_html    TEXT NOT NULL,
    body_text    TEXT NOT NULL,
    attempts     INTEGER NOT NULL DEFAULT 0,
    last_error   TEXT,
    sent_at      TEXT,
    created_at   TEXT NOT NULL
);
CREATE INDEX mail_queue_unsent ON mail_queue (sent_at, created_at);
