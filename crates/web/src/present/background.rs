//! The image behind an audience slide.
//!
//! It travels the way sheets do — content-addressed, private, cached on the device — because it
//! is shown in the one place that must not depend on a network. A background that is not cached
//! falls back to the theme's colour (presenter-output business rule 2); it never shows white,
//! and it never shows a broken image to a room.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, File, Request, RequestInit, Response, Url};

use crate::api::Api;
use crate::blobs::hash_of;
use crate::db::Database;

#[derive(Clone, Debug, Deserialize)]
struct AssetPlan {
    already_stored: bool,
    upload_id: Option<String>,
    url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct SignedUrl {
    url: String,
}

/// Backgrounds share the local file table with sheets, under a prefixed key: they are the same
/// kind of thing — bytes this device holds so a screen can be painted with no network.
fn key_of(sha256: &str) -> String {
    format!("theme-background:{sha256}")
}

#[derive(Clone, Debug, Serialize)]
struct StoredBytes {
    sheet_id: String,
    #[serde(with = "serde_wasm_bindgen::preserve")]
    bytes: JsValue,
}

#[derive(Clone, Debug, Deserialize)]
struct ReadBytes {
    #[serde(with = "serde_wasm_bindgen::preserve")]
    bytes: JsValue,
}

async fn keep(db: &Database, sha256: &str, bytes: &Blob) {
    let _ = db
        .put_quietly(
            "files",
            &StoredBytes {
                sheet_id: key_of(sha256),
                bytes: bytes.clone().into(),
            },
        )
        .await;
}

pub async fn upload(db: &Database, api: &Api, workspace_id: &str, file: &File) -> Option<String> {
    let sha256 = hash_of(file.as_ref()).await?;

    let plan: AssetPlan = api
        .post(
            &format!("/workspaces/{workspace_id}/assets/upload-url"),
            &serde_json::json!({
                "sha256": sha256,
                "size": file.size(),
                "content_type": file.type_(),
            }),
        )
        .await
        .ok()?;

    let mut etag = String::new();

    if !plan.already_stored {
        etag = put(plan.url.as_deref()?, file.as_ref()).await?;
    }

    let mut body = serde_json::json!({
        "sha256": sha256,
        "content_type": file.type_(),
        "size": file.size(),
    });

    if !plan.already_stored {
        body["upload_id"] = serde_json::json!(plan.upload_id);
        body["etag"] = serde_json::json!(etag);
    }

    api.post::<_, serde_json::Value>(
        &format!("/workspaces/{workspace_id}/assets/complete"),
        &body,
    )
    .await
    .ok()?;

    // Keep the bytes here too, so the device that uploaded it can present with no network.
    keep(db, &sha256, file.as_ref()).await;

    Some(sha256)
}

/// A URL the audience window can paint from, preferring the copy on this device. `None` when
/// there is neither a cached copy nor a connection — the caller then uses the colour.
pub async fn url_of(db: &Database, api: &Api, workspace_id: &str, sha256: &str) -> Option<String> {
    if let Ok(Some(held)) = db.get::<ReadBytes>("files", &key_of(sha256)).await
        && let Ok(bytes) = held.bytes.dyn_into::<Blob>()
    {
        return Url::create_object_url_with_blob(&bytes).ok();
    }

    let signed: SignedUrl = api
        .get(&format!("/workspaces/{workspace_id}/assets/{sha256}/url"))
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

    let bytes: Blob = JsFuture::from(response.blob().ok()?)
        .await
        .ok()
        .map(Blob::unchecked_from_js)?;

    keep(db, sha256, &bytes).await;

    Url::create_object_url_with_blob(&bytes).ok()
}

async fn put(url: &str, body: &Blob) -> Option<String> {
    let init = RequestInit::new();

    init.set_method("PUT");
    init.set_body(body.as_ref());

    let request = Request::new_with_str_and_init(url, &init).ok()?;
    let window = web_sys::window()?;

    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .ok()
        .map(Response::unchecked_from_js)?;

    response.ok().then(|| {
        response
            .headers()
            .get("ETag")
            .ok()
            .flatten()
            .unwrap_or_default()
            .replace('"', "")
    })
}
