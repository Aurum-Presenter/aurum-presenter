-- Sheet attachments: which arrangement a sheet belongs to, what it is called, and the name of
-- the file it came from.
--
-- The object itself is not here and never will be: it lives in the object store, keyed by its
-- content hash, and this row is the metadata that syncs. A row with no sha256 is a sheet whose
-- file this device has not downloaded — the "not downloaded" state, not an error.
ALTER TABLE sheets ADD COLUMN arrangement_id TEXT REFERENCES arrangements (id) ON DELETE SET NULL;

-- Free text beside the part: "SATB", "with the second ending", "Kate's copy".
ALTER TABLE sheets ADD COLUMN label TEXT;

-- The name the file was uploaded under, shown when it is downloaded again.
ALTER TABLE sheets ADD COLUMN filename TEXT;

CREATE INDEX sheets_arrangement ON sheets (arrangement_id);
