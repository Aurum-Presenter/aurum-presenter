-- Chord charts: what a chart was pasted as, and what it was pasted from.
--
-- Charts arrive from SongbookPro, OnSong and Ultimate Guitar as chords over lyrics, and are
-- converted to ChordPro on entry so there is only ever one canonical format in `body`. The
-- pre-conversion text is kept because a conversion that mis-reads a column is only debuggable
-- against the original, and because the editor offers one undo.
ALTER TABLE arrangements ADD COLUMN source_notation TEXT NOT NULL DEFAULT 'chordpro'
    CHECK (source_notation IN ('chordpro', 'over_lyrics'));

ALTER TABLE arrangements ADD COLUMN source_text TEXT;

-- The arranger's suggested capo. A reader's own capo is a preference and lives per user.
ALTER TABLE arrangements ADD COLUMN capo_hint INTEGER
    CHECK (capo_hint IS NULL OR (capo_hint BETWEEN 0 AND 11));
