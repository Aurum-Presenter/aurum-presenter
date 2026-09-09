//! The local file store for sheet PDFs.
//!
//! Files live in the origin private file system where it exists — it holds hundreds of megabytes
//! without the structured-clone cost of putting blobs through IndexedDB — and in an IndexedDB
//! store everywhere else. Either way the *record* of what is held lives in `blobs`, so the pin
//! policy and the eviction pass work the same on both.
//!
//! OPFS is not in stable `web-sys`, so the handful of calls it needs go through `Reflect`. That
//! is four method names, and the alternative is putting fifty-megabyte PDFs through the
//! structured clone algorithm on every read.

use js_sys::{Array, Object, Reflect};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::Blob;

use crate::db::records::BlobRecord;
use crate::db::{Database, DbError};

/// Opportunistic downloads are capped; pinned files are not, because the user asked for them.
pub const OPPORTUNISTIC_BUDGET: i64 = 500 * 1024 * 1024;

/// Why a file is on this device. Only opportunistic files are ever evicted.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PinReason {
    /// The user asked for this song or set to be kept.
    Pinned,
    /// A set inside the auto-pin window, or a song in one.
    Auto,
    #[default]
    Opportunistic,
}

impl PinReason {
    pub fn as_str(self) -> &'static str {
        match self {
            PinReason::Pinned => "pinned",
            PinReason::Auto => "auto",
            PinReason::Opportunistic => "opportunistic",
        }
    }
}

/// What is in a file store, as a row that outlives the file itself. `bytes` is only used on the
/// IndexedDB fallback path.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredFile {
    sheet_id: String,
    #[serde(with = "serde_wasm_bindgen::preserve")]
    bytes: JsValue,
}

async fn call(target: &JsValue, method: &str, args: &Array) -> Option<JsValue> {
    let function = Reflect::get(target, &JsValue::from_str(method))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;

    let answer = Reflect::apply(&function, target, args).ok()?;

    match answer.dyn_into::<js_sys::Promise>() {
        Ok(promise) => JsFuture::from(promise).await.ok(),
        // A synchronous answer is still an answer.
        Err(value) => Some(value),
    }
}

fn options(create: bool) -> JsValue {
    let held = Object::new();
    let _ = Reflect::set(
        &held,
        &JsValue::from_str("create"),
        &JsValue::from_bool(create),
    );

    held.into()
}

#[derive(Clone)]
pub struct BlobStore {
    db: Database,
    workspace_id: String,
}

impl BlobStore {
    pub fn new(db: Database, workspace_id: &str) -> BlobStore {
        BlobStore {
            db,
            workspace_id: workspace_id.to_owned(),
        }
    }

    /// The workspace's directory in the origin private file system, or `None` where there is no
    /// OPFS — in which case everything falls back to the `files` store.
    async fn directory(&self) -> Option<JsValue> {
        let storage = web_sys::window()?.navigator().storage();
        let root = call(storage.as_ref(), "getDirectory", &Array::new()).await?;

        if root.is_undefined() || root.is_null() {
            return None;
        }

        call(
            &root,
            "getDirectoryHandle",
            &Array::of2(
                &JsValue::from_str(&format!("aurum-{}", self.workspace_id)),
                &options(true),
            ),
        )
        .await
    }

    pub async fn put(
        &self,
        sheet_id: &str,
        sha256: &str,
        bytes: &Blob,
        pin: PinReason,
    ) -> Result<(), DbError> {
        let size = bytes.size() as i64;

        match self.directory().await {
            Some(directory) => {
                let handle = call(
                    &directory,
                    "getFileHandle",
                    &Array::of2(&JsValue::from_str(sheet_id), &options(true)),
                )
                .await;

                let Some(handle) = handle else {
                    return Err(DbError::Request(
                        "the sheet could not be written".to_owned(),
                    ));
                };

                let Some(writable) = call(&handle, "createWritable", &Array::new()).await else {
                    return Err(DbError::Request(
                        "the sheet could not be written".to_owned(),
                    ));
                };

                call(&writable, "write", &Array::of1(bytes.as_ref())).await;
                call(&writable, "close", &Array::new()).await;
            }

            None => {
                self.db
                    .put(
                        "files",
                        &StoredFile {
                            sheet_id: sheet_id.to_owned(),
                            bytes: bytes.clone().into(),
                        },
                    )
                    .await?;
            }
        }

        self.db
            .put(
                "blobs",
                &BlobRecord {
                    sheet_id: sheet_id.to_owned(),
                    sha256: sha256.to_owned(),
                    size,
                    pin_reason: pin.as_str().to_owned(),
                    cached_at: crate::now(),
                },
            )
            .await?;

        Ok(())
    }

    pub async fn get(&self, sheet_id: &str) -> Option<Blob> {
        let record: BlobRecord = self.db.get("blobs", sheet_id).await.ok().flatten()?;

        let Some(directory) = self.directory().await else {
            let held: Option<StoredFile> = self.db.get("files", sheet_id).await.ok().flatten();

            return held.and_then(|held| held.bytes.dyn_into::<Blob>().ok());
        };

        let handle = call(
            &directory,
            "getFileHandle",
            &Array::of1(&JsValue::from_str(sheet_id)),
        )
        .await;

        let file = match handle {
            Some(handle) => call(&handle, "getFile", &Array::new()).await,
            None => None,
        };

        match file.and_then(|file| file.dyn_into::<Blob>().ok()) {
            Some(bytes) => {
                // Touching `cached_at` on read is what makes eviction least-recently-*used*
                // rather than least-recently-downloaded.
                let _ = self
                    .db
                    .put(
                        "blobs",
                        &BlobRecord {
                            cached_at: crate::now(),
                            ..record
                        },
                    )
                    .await;

                Some(bytes)
            }

            None => {
                // The record outlived the file — a cleared origin, a failed write. Forget the
                // record so the queue downloads it again.
                let _ = self.db.delete("blobs", &JsValue::from_str(sheet_id)).await;

                None
            }
        }
    }

    /// Whether this device holds the file, and holds the *right* one. A different hash means it
    /// was replaced upstream: what is cached is the wrong bytes.
    pub async fn has(&self, sheet_id: &str, sha256: Option<&str>) -> bool {
        let held: Option<BlobRecord> = self.db.get("blobs", sheet_id).await.ok().flatten();

        match (held, sha256) {
            (Some(_), None) => true,
            (Some(record), Some(wanted)) => record.sha256 == wanted,
            (None, _) => false,
        }
    }

    pub async fn remove(&self, sheet_id: &str) -> Result<(), DbError> {
        match self.directory().await {
            Some(directory) => {
                call(
                    &directory,
                    "removeEntry",
                    &Array::of1(&JsValue::from_str(sheet_id)),
                )
                .await;
            }
            None => {
                self.db
                    .delete("files", &JsValue::from_str(sheet_id))
                    .await?
            }
        }

        self.db.delete("blobs", &JsValue::from_str(sheet_id)).await
    }

    pub async fn pin(&self, sheet_id: &str, pin: PinReason) -> Result<(), DbError> {
        let Some(record): Option<BlobRecord> = self.db.get("blobs", sheet_id).await? else {
            return Ok(());
        };

        self.db
            .put(
                "blobs",
                &BlobRecord {
                    pin_reason: pin.as_str().to_owned(),
                    ..record
                },
            )
            .await?;

        Ok(())
    }

    /// Drops the least recently used opportunistic files, either to stay inside the app's own
    /// budget or because the origin is near the browser's quota (business rule 10). Pinned files
    /// are never touched: if a pinned download will not fit, the user is told which pin to
    /// release rather than quietly losing one.
    pub async fn evict(&self, budget: i64) -> Result<usize, DbError> {
        let mut budget = budget;

        if let Some((usage, quota)) = estimate().await
            && quota > 0
            && (usage as f64) / (quota as f64) > 0.85
        {
            budget = budget.min(quota / 2);
        }

        let mut opportunistic: Vec<BlobRecord> = self
            .db
            .all::<BlobRecord>("blobs")
            .await?
            .into_iter()
            .filter(|record| record.pin_reason == PinReason::Opportunistic.as_str())
            .collect();

        opportunistic.sort_by(|left, right| left.cached_at.cmp(&right.cached_at));

        let mut total: i64 = opportunistic.iter().map(|record| record.size).sum();
        let mut dropped = 0;

        for record in opportunistic {
            if total <= budget {
                break;
            }

            self.remove(&record.sheet_id).await?;
            total -= record.size;
            dropped += 1;
        }

        Ok(dropped)
    }
}

/// Business rule 11: ask the browser to keep this origin once the user has pinned something.
/// On iOS an origin that is not persisted can be cleared after a week of not being opened, which
/// would take a pinned set with it.
pub async fn request_persistence() -> bool {
    let Some(storage) = web_sys::window().map(|window| window.navigator().storage()) else {
        return false;
    };

    if matches!(call(storage.as_ref(), "persisted", &Array::new()).await, Some(answer) if answer.is_truthy())
    {
        return true;
    }

    matches!(call(storage.as_ref(), "persist", &Array::new()).await, Some(answer) if answer.is_truthy())
}

/// What the browser says this origin is using, and how much it may use.
pub async fn estimate() -> Option<(i64, i64)> {
    let storage = web_sys::window()?.navigator().storage();
    let held = call(storage.as_ref(), "estimate", &Array::new()).await?;

    let number = |name: &str| {
        Reflect::get(&held, &JsValue::from_str(name))
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0) as i64
    };

    Some((number("usage"), number("quota")))
}

/// Hex sha256 of the bytes, which is both the integrity check and the object key.
pub async fn hash_of(bytes: &Blob) -> Option<String> {
    let buffer = JsFuture::from(bytes.array_buffer()).await.ok()?;
    let subtle = web_sys::window()?.crypto().ok()?.subtle();

    let digest = JsFuture::from(
        subtle
            .digest_with_str_and_buffer_source("SHA-256", buffer.dyn_ref()?)
            .ok()?,
    )
    .await
    .ok()?;

    let bytes = js_sys::Uint8Array::new(&digest).to_vec();

    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
