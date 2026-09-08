-- Sets: the running order for one service or gig.
--
-- `set_items` is rebuilt rather than extended. The initial schema had a coarse `kind`
-- (song/note/break/media) and an integer `position`; the feature needs the full non-song
-- vocabulary and a fractional rank, and neither a CHECK constraint nor a column type can be
-- altered in place in SQLite. The rebuild carries every existing row across.

ALTER TABLE sets ADD COLUMN venue TEXT;

-- Display only: a set names who is playing, it does not grant them anything.
ALTER TABLE sets ADD COLUMN assigned_members TEXT
    CHECK (assigned_members IS NULL OR json_valid(assigned_members));

-- Explicitly kept offline, on top of the automatic window around the date.
ALTER TABLE sets ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1));

CREATE TABLE set_items_rebuilt (
    id                  TEXT PRIMARY KEY,
    set_id              TEXT NOT NULL REFERENCES sets (id) ON DELETE CASCADE,

    -- A fractional rank, not an index: two people reordering offline merge deterministically
    -- and nobody has to renumber the rows in between.
    rank                TEXT NOT NULL,

    -- Business rule 2: an item is a song, or it is its own content. Never both, never neither.
    song_id             TEXT REFERENCES songs (id) ON DELETE SET NULL,
    item_type           TEXT CHECK (item_type IS NULL OR item_type IN
                            ('announcement', 'scripture', 'prayer', 'video', 'blank', 'text')),
    content             TEXT,

    -- Kept when the song is deleted, so a set from two years ago still reads.
    title_snapshot      TEXT,

    key_override        TEXT,
    capo_override       INTEGER CHECK (capo_override IS NULL OR (capo_override BETWEEN 0 AND 11)),
    arrangement_id      TEXT REFERENCES arrangements (id) ON DELETE SET NULL,
    sheet_part_override TEXT,

    -- Ordered section ids actually played; null means the whole chart.
    sections            TEXT CHECK (sections IS NULL OR json_valid(sections)),
    note                TEXT,

    updated_at          TEXT NOT NULL,
    change_seq          INTEGER NOT NULL,
    deleted_at          TEXT,
    updated_by          TEXT,

    CHECK ((song_id IS NOT NULL AND item_type IS NULL) OR (song_id IS NULL AND item_type IS NOT NULL))
);

INSERT INTO set_items_rebuilt (
    id, set_id, rank, song_id, item_type, content, title_snapshot, key_override, note,
    updated_at, change_seq, deleted_at, updated_by
)
SELECT
    id,
    set_id,
    -- Integer positions become evenly spaced rank strings, keeping the order they had.
    printf('a%04d', position),
    song_id,
    CASE WHEN song_id IS NOT NULL THEN NULL
         WHEN kind = 'media' THEN 'video'
         WHEN kind = 'break' THEN 'blank'
         ELSE 'text' END,
    notes,
    title,
    key_override,
    notes,
    updated_at, change_seq, deleted_at, updated_by
FROM set_items;

DROP TABLE set_items;
ALTER TABLE set_items_rebuilt RENAME TO set_items;

CREATE INDEX set_items_change_seq ON set_items (change_seq);
CREATE INDEX set_items_set ON set_items (set_id);
CREATE INDEX set_items_song ON set_items (song_id);
