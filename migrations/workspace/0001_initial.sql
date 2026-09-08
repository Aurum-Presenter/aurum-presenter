-- Content tier: one file per workspace.
--
-- There is deliberately NO workspace_id column anywhere below. The file is the scope, which is
-- what replaced Postgres row-level security: a query cannot forget the predicate, because there
-- is no predicate to forget.
--
-- Every synced table carries the same four sync columns:
--   updated_at  TEXT     server clock, set by the application on write
--   change_seq  INTEGER  from sync_counter; the delta-pull watermark
--   deleted_at  TEXT     tombstone; always wins over a concurrent edit
--   updated_by  TEXT     user id, for the conflict panel's "changed by"

-- The single-row counter that replaces the per-workspace Postgres sequence. Bumped inside
-- BEGIN IMMEDIATE, so values are strictly ordered and — unlike a sequence — gap-free.
CREATE TABLE sync_counter (
    id  INTEGER PRIMARY KEY CHECK (id = 1),
    seq INTEGER NOT NULL DEFAULT 0
);
INSERT INTO sync_counter (id, seq) VALUES (1, 0);

CREATE TABLE folders (
    id         TEXT PRIMARY KEY,
    parent_id  TEXT REFERENCES folders (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    position   INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    change_seq INTEGER NOT NULL,
    deleted_at TEXT,
    updated_by TEXT
);
CREATE INDEX folders_change_seq ON folders (change_seq);
CREATE INDEX folders_parent ON folders (parent_id);

CREATE TABLE songs (
    id              TEXT PRIMARY KEY,
    folder_id       TEXT REFERENCES folders (id) ON DELETE SET NULL,
    title           TEXT NOT NULL,
    subtitle        TEXT,
    authors         TEXT,
    ccli_number     TEXT,
    copyright       TEXT,
    notes           TEXT,
    original_key    TEXT,
    tempo           INTEGER,
    time_signature  TEXT,
    tags            TEXT CHECK (tags IS NULL OR json_valid(tags)),
    updated_at      TEXT NOT NULL,
    change_seq      INTEGER NOT NULL,
    deleted_at      TEXT,
    updated_by      TEXT
);
CREATE INDEX songs_change_seq ON songs (change_seq);
CREATE INDEX songs_folder ON songs (folder_id);
CREATE INDEX songs_title ON songs (title COLLATE NOCASE);

CREATE TABLE arrangements (
    id          TEXT PRIMARY KEY,
    song_id     TEXT NOT NULL REFERENCES songs (id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    body        TEXT NOT NULL DEFAULT '',
    default_key TEXT,
    is_default  INTEGER NOT NULL DEFAULT 0,
    position    INTEGER NOT NULL DEFAULT 0,
    updated_at  TEXT NOT NULL,
    change_seq  INTEGER NOT NULL,
    deleted_at  TEXT,
    updated_by  TEXT
);
CREATE INDEX arrangements_change_seq ON arrangements (change_seq);
CREATE INDEX arrangements_song ON arrangements (song_id);

CREATE TABLE sheets (
    id          TEXT PRIMARY KEY,
    song_id     TEXT NOT NULL REFERENCES songs (id) ON DELETE CASCADE,
    sheet_key   TEXT,
    part        TEXT,
    position    INTEGER NOT NULL DEFAULT 0,
    sha256      TEXT,
    size        INTEGER,
    page_count  INTEGER,
    mime_type   TEXT NOT NULL DEFAULT 'application/pdf',
    uploaded_at TEXT,
    updated_at  TEXT NOT NULL,
    change_seq  INTEGER NOT NULL,
    deleted_at  TEXT,
    updated_by  TEXT
);
CREATE INDEX sheets_change_seq ON sheets (change_seq);
CREATE INDEX sheets_song ON sheets (song_id);

CREATE TABLE annotations (
    id         TEXT PRIMARY KEY,
    sheet_id   TEXT NOT NULL REFERENCES sheets (id) ON DELETE CASCADE,
    page       INTEGER NOT NULL,
    strokes    TEXT NOT NULL CHECK (json_valid(strokes)),
    scope      TEXT NOT NULL DEFAULT 'personal' CHECK (scope IN ('personal', 'shared')),
    author_id  TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    change_seq INTEGER NOT NULL,
    deleted_at TEXT,
    updated_by TEXT
);
CREATE INDEX annotations_change_seq ON annotations (change_seq);
CREATE INDEX annotations_sheet ON annotations (sheet_id);

CREATE TABLE sets (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    scheduled_for TEXT,
    notes         TEXT,
    updated_at    TEXT NOT NULL,
    change_seq    INTEGER NOT NULL,
    deleted_at    TEXT,
    updated_by    TEXT
);
CREATE INDEX sets_change_seq ON sets (change_seq);
CREATE INDEX sets_scheduled ON sets (scheduled_for);

CREATE TABLE set_items (
    id           TEXT PRIMARY KEY,
    set_id       TEXT NOT NULL REFERENCES sets (id) ON DELETE CASCADE,
    song_id      TEXT REFERENCES songs (id) ON DELETE SET NULL,
    kind         TEXT NOT NULL DEFAULT 'song' CHECK (kind IN ('song', 'note', 'break', 'media')),
    title        TEXT,
    key_override TEXT,
    position     INTEGER NOT NULL DEFAULT 0,
    notes        TEXT,
    updated_at   TEXT NOT NULL,
    change_seq   INTEGER NOT NULL,
    deleted_at   TEXT,
    updated_by   TEXT
);
CREATE INDEX set_items_change_seq ON set_items (change_seq);
CREATE INDEX set_items_set ON set_items (set_id);

-- Per-user, never shared: preferred key, capo, font size, pinned-for-offline.
CREATE TABLE preferences (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL,
    scope_type TEXT NOT NULL CHECK (scope_type IN ('song', 'set', 'folder', 'workspace')),
    scope_id   TEXT,
    name       TEXT NOT NULL,
    value      TEXT CHECK (value IS NULL OR json_valid(value)),
    updated_at TEXT NOT NULL,
    change_seq INTEGER NOT NULL,
    deleted_at TEXT,
    updated_by TEXT,
    UNIQUE (user_id, scope_type, scope_id, name)
);
CREATE INDEX preferences_change_seq ON preferences (change_seq);

-- Push idempotency: a retried batch after a lost response must not apply twice.
-- Purged after 24 hours by the maintenance command.
CREATE TABLE applied_ops (
    op_id      TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL,
    applied_at TEXT NOT NULL
);
CREATE INDEX applied_ops_applied_at ON applied_ops (applied_at);

-- The losing side of a per-field last-writer-wins resolution, kept so nothing is silently
-- destroyed and the conflict review panel has something to show on every device.
CREATE TABLE sync_conflicts (
    id           TEXT PRIMARY KEY,
    table_name   TEXT NOT NULL,
    record_id    TEXT NOT NULL,
    field        TEXT NOT NULL,
    losing_value TEXT,
    losing_user  TEXT,
    at           TEXT NOT NULL
);
CREATE INDEX sync_conflicts_at ON sync_conflicts (at);
