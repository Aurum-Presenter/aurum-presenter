-- Every workspace starts with a theme, so the first service a band runs is not configured, it
-- is just correct: white text on black, centred, 8vh. The presenter-output spec asks for this
-- at workspace creation, and a migration is where "at creation" lives when the file is the
-- workspace.
--
-- Guarded, because this file also runs against workspaces that already have themes: an early
-- adopter who made their own must not be given a second default. The counter is only bumped
-- when a row is actually written, so change_seq stays gap-free.
UPDATE sync_counter
SET seq = seq + 1
WHERE NOT EXISTS (SELECT 1 FROM presenter_themes);

INSERT INTO presenter_themes (
    id, name, is_default, font_family, font_size_vh, text_color,
    background_kind, background_value, align, safe_area_pct, show_section_labels,
    updated_at, change_seq, deleted_at, updated_by
)
SELECT
    '01950000-0000-7000-8000-000000000001',
    'Default',
    1,
    'system-ui, sans-serif',
    8,
    '#ffffff',
    'color',
    '#000000',
    'center',
    5,
    0,
    strftime('%Y-%m-%dT%H:%M:%f', 'now') || 'Z',
    (SELECT seq FROM sync_counter),
    NULL,
    NULL
WHERE NOT EXISTS (SELECT 1 FROM presenter_themes);
