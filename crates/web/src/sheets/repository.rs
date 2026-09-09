//! Attaching, replacing and describing sheets.
//!
//! A sheet is two things that travel separately: a row, which syncs with everything else, and a
//! file, which goes through the blob queue. The row is written first and the file is stored
//! locally straight away, so an attach made on a plane is complete from the user's point of view
//! before the aircraft has landed.

use serde_json::{Map, Value, json};
use web_sys::File;

use super::renderer::{PdfiumRenderer, SheetRenderer, bytes_of};
use crate::blobs::{BlobStore, PinReason, hash_of};
use crate::db::records::{Sheet, UploadRecord, alive};
use crate::db::{Database, DbError};
use crate::sync::SyncEngine;

/// The largest file that may be attached. Beyond this a "sheet" is a scan nobody meant to make.
pub const MAX_SHEET_BYTES: f64 = 50.0 * 1024.0 * 1024.0;

/// Where the size stops being incidental and starts being a decision every pinned device pays for.
pub const WARN_SHEET_BYTES: f64 = 10.0 * 1024.0 * 1024.0;

pub const ACCEPTED: [&str; 4] = ["application/pdf", "image/png", "image/jpeg", "image/heic"];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetInput {
    pub key: Option<String>,
    pub part: String,
    pub label: Option<String>,
    pub arrangement_id: Option<String>,
}

#[derive(Clone)]
pub struct Sheets {
    db: Database,
    engine: SyncEngine,
    store: BlobStore,
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// Rejects what the viewer could not open, rather than storing it and failing later.
pub fn problem_with(file: &File) -> Option<String> {
    let kind = file.type_();

    if !ACCEPTED.contains(&kind.as_str()) {
        let named = if kind.is_empty() {
            "file of unknown type".to_owned()
        } else {
            kind
        };

        return Some(format!(
            "{} is a {named}. Attach a PDF or an image.",
            file.name()
        ));
    }

    if file.size() > MAX_SHEET_BYTES {
        return Some(format!(
            "{} is {} MB. The limit is 50 MB.",
            file.name(),
            (file.size() / 1024.0 / 1024.0).round(),
        ));
    }

    None
}

/// Sizes a musician can act on: a 40 MB score is a decision, 17 KB is not.
pub fn file_size(bytes: i64) -> String {
    if bytes < 1024 * 1024 {
        format!("{} KB", (bytes as f64 / 1024.0).round().max(1.0))
    } else {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    }
}

/// The page count comes from the file itself; an image is one page by definition.
///
/// It is read here rather than waited for from the server: the device that attached the sheet
/// knows how many pages it has, and the row is read on this device long before it is pushed
/// anywhere. It is also the number the "may not line up" rule is decided on.
async fn page_count(file: &File) -> Option<i64> {
    if file.type_() != "application/pdf" {
        return Some(1);
    }

    let renderer = PdfiumRenderer::load().await.ok()?;
    let bytes = bytes_of(file.as_ref()).await?;

    // A password-protected or damaged PDF still attaches; the viewer explains it later.
    renderer.page_count(&bytes).ok().map(i64::from)
}

impl Sheets {
    pub fn new(db: Database, engine: SyncEngine, store: BlobStore) -> Sheets {
        Sheets { db, engine, store }
    }

    pub async fn attach(
        &self,
        song_id: &str,
        file: &File,
        input: &SheetInput,
    ) -> Result<String, DbError> {
        let id = crate::new_id();
        let existing = self.of_song(song_id).await?.len() as i64;
        let pages = page_count(file).await;

        self.engine
            .record(
                "sheets",
                &id,
                "upsert",
                object(json!({
                    "song_id": song_id,
                    "sheet_key": input.key,
                    "part": input.part,
                    "label": input.label,
                    "arrangement_id": input.arrangement_id,
                    "filename": file.name(),
                    "mime_type": file.type_(),
                    "position": existing,
                    "page_count": pages,
                })),
            )
            .await?;

        self.store_and_queue(&id, file, None, pages).await?;

        Ok(id)
    }

    /// Business rule 3: same bytes, nothing happens; different bytes, every device re-downloads.
    pub async fn replace(&self, sheet_id: &str, file: &File) -> Result<(), DbError> {
        let sheet: Option<Sheet> = self.db.get("sheets", sheet_id).await?;
        let Some(sha256) = hash_of(file.as_ref()).await else {
            return Ok(());
        };

        if sheet.as_ref().and_then(|row| row.sha256.as_deref()) == Some(sha256.as_str()) {
            return Ok(());
        }

        // Business rule 7: marks are normalised to a page, so they survive anything but the
        // pages themselves moving. When the count changes they are kept and flagged, never
        // deleted — a musician's own fingering is not the app's to throw away.
        let pages = page_count(file).await;
        let before = sheet.and_then(|row| row.page_count);
        let moved = matches!((before, pages), (Some(before), Some(now)) if before != now);

        let mut payload = object(json!({
            "filename": file.name(),
            "mime_type": file.type_(),
            "page_count": pages,
        }));

        if moved {
            payload.insert("pages_changed_at".to_owned(), json!(crate::now()));
        }

        self.engine
            .record("sheets", sheet_id, "upsert", payload)
            .await?;
        self.store_and_queue(sheet_id, file, Some(sha256), pages)
            .await
    }

    pub async fn update(&self, sheet_id: &str, changes: Map<String, Value>) -> Result<(), DbError> {
        self.engine
            .record("sheets", sheet_id, "upsert", changes)
            .await
    }

    pub async fn remove(&self, sheet_id: &str) -> Result<(), DbError> {
        self.engine
            .record("sheets", sheet_id, "delete", Map::new())
            .await?;
        self.store.remove(sheet_id).await?;
        self.db.delete("uploads", &sheet_id.into()).await
    }

    /// A song's sheets, in the order they were attached.
    pub async fn of_song(&self, song_id: &str) -> Result<Vec<Sheet>, DbError> {
        let mut sheets = alive(
            self.db
                .by_index::<Sheet>("sheets", "song_id", &song_id.into())
                .await?,
        );

        sheets.sort_by_key(|sheet| sheet.position);

        Ok(sheets)
    }

    pub async fn file(&self, sheet_id: &str) -> Option<web_sys::Blob> {
        self.store.get(sheet_id).await
    }

    async fn store_and_queue(
        &self,
        sheet_id: &str,
        file: &File,
        hash: Option<String>,
        pages: Option<i64>,
    ) -> Result<(), DbError> {
        let Some(sha256) = (match hash {
            Some(hash) => Some(hash),
            None => hash_of(file.as_ref()).await,
        }) else {
            return Ok(());
        };

        // Pinned, not opportunistic: the device that made the file is the only one that has it
        // until the queue drains, so eviction must not be allowed anywhere near it.
        self.store
            .put(sheet_id, &sha256, file.as_ref(), PinReason::Pinned)
            .await?;

        self.db
            .put(
                "uploads",
                &UploadRecord {
                    sheet_id: sheet_id.to_owned(),
                    sha256,
                    size: file.size() as i64,
                    filename: file.name(),
                    page_count: pages,
                    attempts: 0,
                    last_error: None,
                    queued_at: crate::now(),
                },
            )
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_is_rounded_to_something_a_person_can_act_on() {
        assert_eq!(file_size(17_000), "17 KB");
        assert_eq!(file_size(1), "1 KB", "never zero: the file exists");
        assert_eq!(file_size(42_000_000), "40.1 MB");
    }
}
