-- How the audience screen looks. Workspace-scoped and synced like any other record, so a band
-- sets its font and its background once and every operator's laptop already agrees.
--
-- A device may override the active theme for one session without touching the workspace
-- default; that override is local and never written here.
CREATE TABLE presenter_themes (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    is_default          INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    font_family         TEXT NOT NULL DEFAULT 'system-ui, sans-serif',
    -- The maximum size. The fitting pass may come down from it, never up.
    font_size_vh        REAL NOT NULL DEFAULT 8,
    text_color          TEXT NOT NULL DEFAULT '#ffffff',
    background_kind     TEXT NOT NULL DEFAULT 'color' CHECK (background_kind IN ('color', 'gradient', 'image')),
    background_value    TEXT NOT NULL DEFAULT '#000000',
    align               TEXT NOT NULL DEFAULT 'center' CHECK (align IN ('left', 'center')),
    safe_area_pct       REAL NOT NULL DEFAULT 5,
    show_section_labels INTEGER NOT NULL DEFAULT 0 CHECK (show_section_labels IN (0, 1)),
    updated_at          TEXT NOT NULL,
    change_seq          INTEGER NOT NULL,
    deleted_at          TEXT,
    updated_by          TEXT
);
CREATE INDEX presenter_themes_change_seq ON presenter_themes (change_seq);
