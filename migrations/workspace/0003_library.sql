-- The song library: the fields a band actually files songs by, and the second home a song can
-- have in another folder.
--
-- `alt_titles` and `tags` are JSON arrays rather than a join table. They are read whole, written
-- whole, and never queried across songs on the server — search is client-side by design — so a
-- table would buy nothing and cost a delta-pull join.
ALTER TABLE songs ADD COLUMN artist TEXT;
ALTER TABLE songs ADD COLUMN alt_titles TEXT CHECK (alt_titles IS NULL OR json_valid(alt_titles));
ALTER TABLE songs ADD COLUMN duration_sec INTEGER CHECK (duration_sec IS NULL OR duration_sec >= 0);

-- Archived is not deleted: an archived song keeps its charts, sheets and set history, and is
-- simply out of the way. Only `deleted_at` is a tombstone.
ALTER TABLE songs ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1));

CREATE INDEX songs_archived ON songs (archived);

-- A song has one home folder (`songs.folder_id`) and any number of additional placements.
-- The row carries its own id because the sync engine addresses every record by one.
CREATE TABLE song_placements (
    id         TEXT PRIMARY KEY,
    song_id    TEXT NOT NULL REFERENCES songs (id) ON DELETE CASCADE,
    folder_id  TEXT NOT NULL REFERENCES folders (id) ON DELETE CASCADE,
    updated_at TEXT NOT NULL,
    change_seq INTEGER NOT NULL,
    deleted_at TEXT,
    updated_by TEXT
);
CREATE INDEX song_placements_change_seq ON song_placements (change_seq);
CREATE INDEX song_placements_song ON song_placements (song_id);
CREATE INDEX song_placements_folder ON song_placements (folder_id);
