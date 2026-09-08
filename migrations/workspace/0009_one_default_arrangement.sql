-- Exactly one default arrangement per song, enforced by the file rather than remembered by
-- every writer.
--
-- Two devices offline can each make a different arrangement the default; the sync service
-- demotes the others as it applies the later write, so this index is a statement of what is
-- already true rather than a way of refusing anybody's edit. Tombstoned rows are excluded —
-- a deleted arrangement that was once the default must not block the new one.
--
-- Existing files are tidied first: where a song somehow has two defaults, the one edited most
-- recently keeps it, which is the same rule the rest of sync settles on.
UPDATE arrangements
SET is_default = 0
WHERE is_default = 1
  AND deleted_at IS NULL
  AND id NOT IN (
      SELECT id FROM arrangements AS winner
      WHERE winner.is_default = 1
        AND winner.deleted_at IS NULL
        AND winner.updated_at = (
            SELECT MAX(other.updated_at) FROM arrangements AS other
            WHERE other.song_id = winner.song_id AND other.is_default = 1 AND other.deleted_at IS NULL
        )
      GROUP BY winner.song_id
  );

CREATE UNIQUE INDEX arrangements_one_default
    ON arrangements (song_id)
    WHERE is_default = 1 AND deleted_at IS NULL;
