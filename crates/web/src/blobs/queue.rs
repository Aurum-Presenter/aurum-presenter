//! Sheet files moving between the device and the object store.
//!
//! Deliberately separate from the metadata sync. A 40 MB piano score must never hold up a key
//! change from reaching the rest of the band, so this runs beside the outbox and its failures
//! never surface as sync failures.

use std::cell::RefCell;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, Request, RequestInit, Response};

use super::store::{BlobStore, PinReason, request_persistence};
use crate::api::Api;
use crate::app::locks::as_sole_worker;
use crate::db::Database;
use crate::db::records::{Sheet, Song, UploadRecord};

#[derive(Clone, Debug, Deserialize)]
struct UploadPart {
    part_number: i64,
    offset: f64,
    length: f64,
    url: String,
}

#[derive(Clone, Debug, Deserialize)]
struct UploadPlan {
    already_stored: bool,
    upload_id: Option<String>,
    #[serde(default)]
    parts: Vec<UploadPart>,
}

#[derive(Clone, Debug, Serialize)]
struct CompletedPart {
    part_number: i64,
    etag: String,
}

#[derive(Clone, Debug, Deserialize)]
struct SignedUrl {
    url: String,
}

/// A pinned file that will not fit on this device.
///
/// The device is out of room and the app will not choose for the user: a pinned file is
/// something somebody asked for, so the app names what could not be kept and lets them decide
/// which pin to release (business rule 10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageFull {
    pub sheet_id: String,
    pub song_title: String,
    pub size: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueueState {
    pub uploading: usize,
    pub failed: usize,
    pub full: Option<StorageFull>,
}

thread_local! {
    /// One pass at a time in this window; `as_sole_worker` handles the other windows.
    static RUNNING: RefCell<bool> = const { RefCell::new(false) };
    static FULL: RefCell<Option<StorageFull>> = const { RefCell::new(None) };
}

#[derive(Clone)]
pub struct BlobQueue {
    db: Database,
    store: BlobStore,
    api: Api,
    workspace_id: String,
}

impl BlobQueue {
    pub fn new(db: Database, store: BlobStore, api: Api, workspace_id: &str) -> BlobQueue {
        BlobQueue {
            db,
            store,
            api,
            workspace_id: workspace_id.to_owned(),
        }
    }

    /// One pass: push everything the server does not have, then pull everything this device is
    /// supposed to be keeping. Uploads go first — a file that exists only here is the only copy.
    pub async fn run(&self, wanted: &BTreeSet<String>) -> QueueState {
        if RUNNING.with(|running| *running.borrow()) || !crate::online() {
            return self.state().await;
        }

        let held = self.state().await;
        let queue = self.clone();
        let wanted = wanted.clone();

        // One window moves files for the device; the others read the same local result.
        as_sole_worker(
            &format!("aurum-files-{}", self.workspace_id),
            async move { queue.pass(&wanted).await },
            held,
        )
        .await
    }

    async fn pass(&self, wanted: &BTreeSet<String>) -> QueueState {
        RUNNING.with(|running| *running.borrow_mut() = true);

        // Business rule 11: once this device is keeping something deliberately, ask the browser
        // not to clear the origin under pressure.
        if !wanted.is_empty() {
            request_persistence().await;
        }

        self.drain_uploads().await;
        self.fetch_wanted(wanted).await;
        let _ = self.store.evict(super::store::OPPORTUNISTIC_BUDGET).await;

        RUNNING.with(|running| *running.borrow_mut() = false);

        self.state().await
    }

    pub async fn state(&self) -> QueueState {
        let uploads: Vec<UploadRecord> = self.db.all("uploads").await.unwrap_or_default();

        QueueState {
            uploading: uploads
                .iter()
                .filter(|upload| upload.last_error.is_none())
                .count(),
            failed: uploads
                .iter()
                .filter(|upload| upload.last_error.is_some())
                .count(),
            full: FULL.with(|full| full.borrow().clone()),
        }
    }

    /// Called once the user has released a pin, so the next pass is allowed to try again.
    pub fn clear_full() {
        FULL.with(|full| *full.borrow_mut() = None);
    }

    async fn drain_uploads(&self) {
        let mut uploads: Vec<UploadRecord> = self.db.all("uploads").await.unwrap_or_default();

        uploads.sort_by(|left, right| left.queued_at.cmp(&right.queued_at));

        for upload in uploads {
            let Some(bytes) = self.store.get(&upload.sheet_id).await else {
                // The local copy is gone; there is nothing left to upload and nothing to be done.
                let _ = self
                    .db
                    .delete("uploads", &upload.sheet_id.as_str().into())
                    .await;

                continue;
            };

            match self.push_one(&upload, &bytes).await {
                Ok(()) => {
                    let _ = self
                        .db
                        .delete("uploads", &upload.sheet_id.as_str().into())
                        .await;
                }

                // The file stays on the device and the entry stays in the queue: the next pass
                // tries again, and nothing the user made is lost in the meantime.
                Err(why) => {
                    let _ = self
                        .db
                        .put(
                            "uploads",
                            &UploadRecord {
                                attempts: upload.attempts + 1,
                                last_error: Some(why),
                                ..upload
                            },
                        )
                        .await;
                }
            }
        }
    }

    async fn push_one(&self, upload: &UploadRecord, bytes: &Blob) -> Result<(), String> {
        let plan: UploadPlan = self
            .api
            .post(
                &format!(
                    "/workspaces/{}/sheets/{}/upload-url",
                    self.workspace_id, upload.sheet_id
                ),
                &serde_json::json!({ "sha256": upload.sha256, "size": upload.size }),
            )
            .await
            .map_err(|error| error.to_string())?;

        let mut done: Vec<CompletedPart> = Vec::new();

        // Content addressing means identical bytes are already at the key: an upload that has
        // happened once, from any device, never happens again.
        if !plan.already_stored {
            for part in &plan.parts {
                let slice = bytes
                    .slice_with_f64_and_f64(part.offset, part.offset + part.length)
                    .map_err(|_| "The file could not be read for upload.".to_owned())?;

                let etag = put_part(&part.url, &slice)
                    .await
                    .ok_or_else(|| format!("Part {} failed to upload.", part.part_number))?;

                done.push(CompletedPart {
                    part_number: part.part_number,
                    etag,
                });
            }
        }

        let mut body = serde_json::json!({
            "sha256": upload.sha256,
            "size": upload.size,
            "page_count": upload.page_count,
        });

        if !plan.already_stored {
            body["upload_id"] = serde_json::json!(plan.upload_id);
            body["parts"] = serde_json::to_value(&done).unwrap_or_default();
        }

        self.api
            .post::<_, serde_json::Value>(
                &format!(
                    "/workspaces/{}/sheets/{}/complete",
                    self.workspace_id, upload.sheet_id
                ),
                &body,
            )
            .await
            .map_err(|error| error.to_string())?;

        Ok(())
    }

    async fn fetch_wanted(&self, wanted: &BTreeSet<String>) {
        // Each pass decides for itself: a pin released since the last one may have made room.
        BlobQueue::clear_full();

        for sheet_id in wanted {
            let Ok(Some(sheet)) = self.db.get::<Sheet>("sheets", sheet_id).await else {
                continue;
            };

            let (Some(sha256), None) = (sheet.sha256.as_deref(), sheet.sync.deleted_at.as_deref())
            else {
                continue;
            };

            if self.store.has(sheet_id, Some(sha256)).await {
                let _ = self.store.pin(sheet_id, PinReason::Pinned).await;
                continue;
            }

            let Some(bytes) = self.download(sheet_id).await else {
                // Offline, or the object is not there yet. The sheet shows as not downloaded
                // and the next pass will try again.
                continue;
            };

            if self
                .store
                .put(sheet_id, sha256, &bytes, PinReason::Pinned)
                .await
                .is_err()
            {
                // Out of room. Stop — every file after this one would fail the same way — and
                // say which file it was, so the user can decide what to release.
                let title = self
                    .db
                    .get::<Song>("songs", &sheet.song_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|song| song.title)
                    .unwrap_or_else(|| "A sheet".to_owned());

                FULL.with(|full| {
                    *full.borrow_mut() = Some(StorageFull {
                        sheet_id: sheet_id.clone(),
                        song_title: title,
                        size: bytes.size() as i64,
                    });
                });

                return;
            }
        }
    }

    /// Downloads one sheet because the user is looking at it right now.
    pub async fn fetch_now(&self, sheet_id: &str) -> Option<Blob> {
        let sheet: Sheet = self.db.get("sheets", sheet_id).await.ok().flatten()?;

        // The local copy first, and before the hash is even considered: the device that attached
        // the file holds it while `sha256` is still `None` — the server sets that on completion
        // and it arrives on a later pull. Reading "not downloaded" on the one device with the
        // only copy is exactly the failure acceptance criterion 4 is about.
        if let Some(cached) = self.store.get(sheet_id).await
            && self.store.has(sheet_id, sheet.sha256.as_deref()).await
        {
            return Some(cached);
        }

        // Nothing here, and nothing to ask the server for yet: the upload has not finished.
        let sha256 = sheet.sha256.as_deref()?;
        let bytes = self.download(sheet_id).await?;

        let _ = self
            .store
            .put(sheet_id, sha256, &bytes, PinReason::Opportunistic)
            .await;

        Some(bytes)
    }

    async fn download(&self, sheet_id: &str) -> Option<Blob> {
        let signed: SignedUrl = self
            .api
            .get(&format!(
                "/workspaces/{}/sheets/{sheet_id}/url",
                self.workspace_id
            ))
            .await
            .ok()?;

        let window = web_sys::window()?;
        let response: Response = JsFuture::from(window.fetch_with_str(&signed.url))
            .await
            .ok()
            .map(Response::unchecked_from_js)?;

        if !response.ok() {
            return None;
        }

        JsFuture::from(response.blob().ok()?)
            .await
            .ok()
            .map(Blob::unchecked_from_js)
    }
}

/// One part of a multipart upload, straight to the object store rather than through the API.
async fn put_part(url: &str, slice: &Blob) -> Option<String> {
    let init = RequestInit::new();

    init.set_method("PUT");
    init.set_body(slice.as_ref());

    let request = Request::new_with_str_and_init(url, &init).ok()?;
    let window = web_sys::window()?;

    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .ok()
        .map(Response::unchecked_from_js)?;

    if !response.ok() {
        return None;
    }

    // The object store quotes its ETags; the completion call wants them bare.
    Some(
        response
            .headers()
            .get("ETag")
            .ok()
            .flatten()
            .unwrap_or_default()
            .replace('"', ""),
    )
}
