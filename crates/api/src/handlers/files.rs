//! Sheet PDFs and workspace assets.
//!
//! Bytes never pass through this server: the client is handed presigned URLs and talks to the
//! object store directly. Keys are content-addressed, which makes "uploading the same file
//! twice" free rather than a rule anybody has to remember.

use aurum_core::storage::keys;
use axum::Json;
use axum::extract::{Path, State};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};

use crate::db::{blocking, now, write_batch};
use crate::error::{ApiError, ApiResult};
use crate::extract::{Body, Read, Workspace, Write, object, optional_string, require_string};
use crate::state::AppState;

const DOWNLOAD_TTL: u64 = 3600;
const UPLOAD_TTL: u64 = 900;
const PART_SIZE: i64 = 8 * 1024 * 1024;
const MAX_SHEET_SIZE: i64 = 512 * 1024 * 1024;
const MAX_ASSET_SIZE: i64 = 16 * 1024 * 1024;

pub async fn sheet_download_url(
    State(state): State<AppState>,
    workspace: Workspace<Read>,
    Path((_, sheet_id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let (sha256, size) = {
        let state = state.clone();
        let workspace_id = workspace.id.clone();
        let db = workspace.open(&state)?;

        blocking(move || {
            let _ = workspace_id;

            let row: Option<(Option<String>, Option<i64>)> = db
                .query_row(
                    "SELECT sha256, size FROM sheets WHERE id = ?1 AND deleted_at IS NULL",
                    [&sheet_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            let (sha256, size) = row.ok_or_else(|| {
                ApiError::not_found("No such sheet in this workspace.").with_code("sheet_not_found")
            })?;

            // A row without a file is the "not downloaded" state, not an error — the client
            // shows it plainly rather than treating it as a failure.
            let sha256 = sha256.ok_or_else(|| {
                ApiError::not_found("This sheet has no file yet.").with_code("file_not_uploaded")
            })?;

            Ok((sha256, size))
        })
        .await?
    };

    let key = keys::sheet_key(&workspace.id, &sha256);

    Ok(Json(json!({
        "url": state.storage.presign_get(&key, DOWNLOAD_TTL).await?,
        "sha256": sha256,
        "size": size,
        "expires_in": DOWNLOAD_TTL,
    })))
}

pub async fn sheet_upload_url(
    State(state): State<AppState>,
    workspace: Workspace<Write>,
    Path((_, sheet_id)): Path<(String, String)>,
    Body(body): Body<Value>,
) -> ApiResult<Json<Value>> {
    let body = object(&body);
    let sha256 = require_string(&body, "sha256", 64)?.to_lowercase();
    let size = body.get("size").and_then(Value::as_i64).unwrap_or_default();

    if !keys::is_valid_sha256(&sha256) {
        return Err(ApiError::validation(
            "sha256",
            "\"sha256\" must be 64 hex characters.",
        ));
    }

    if !(1..=MAX_SHEET_SIZE).contains(&size) {
        return Err(ApiError::validation(
            "size",
            format!("\"size\" must be between 1 and {MAX_SHEET_SIZE} bytes."),
        ));
    }

    {
        let db = workspace.open(&state)?;
        let sheet_id = sheet_id.clone();

        blocking(move || {
            let exists: Option<i64> = db
                .query_row("SELECT 1 FROM sheets WHERE id = ?1", [&sheet_id], |row| {
                    row.get(0)
                })
                .optional()?;

            exists.map(|_| ()).ok_or_else(|| {
                ApiError::not_found("No such sheet in this workspace.").with_code("sheet_not_found")
            })
        })
        .await?;
    }

    let key = keys::sheet_key(&workspace.id, &sha256);

    // Content addressing makes deduplication free: identical bytes are already at this key.
    if state.storage.exists(&key).await {
        return Ok(Json(json!({ "already_stored": true, "key": key })));
    }

    let upload_id = state
        .storage
        .create_multipart_upload(&key, "application/pdf")
        .await?;

    let mut parts = Vec::new();

    for number in 1..=(size + PART_SIZE - 1) / PART_SIZE {
        let offset = (number - 1) * PART_SIZE;

        parts.push(json!({
            "part_number": number,
            "offset": offset,
            "length": PART_SIZE.min(size - offset),
            "url": state
                .storage
                .presign_upload_part(&key, &upload_id, number as i32, UPLOAD_TTL)
                .await?,
        }));
    }

    Ok(Json(json!({
        "already_stored": false,
        "key": key,
        "upload_id": upload_id,
        "part_size": PART_SIZE,
        "parts": parts,
        "expires_in": UPLOAD_TTL,
    })))
}

/// The server verifies the stored object before it writes the hash to the row. This is why
/// `sha256`, `size` and `uploaded_at` are not client-writable columns: a push that could set
/// them would be a client claiming a file it never uploaded.
pub async fn sheet_complete(
    State(state): State<AppState>,
    workspace: Workspace<Write>,
    Path((_, sheet_id)): Path<(String, String)>,
    Body(body): Body<Value>,
) -> ApiResult<Json<Value>> {
    let body = object(&body);
    let sha256 = require_string(&body, "sha256", 64)?.to_lowercase();

    if !keys::is_valid_sha256(&sha256) {
        return Err(ApiError::validation(
            "sha256",
            "\"sha256\" must be 64 hex characters.",
        ));
    }

    let key = keys::sheet_key(&workspace.id, &sha256);

    if let Some(upload_id) = optional_string(&body, "upload_id") {
        let parts: Vec<(i32, String)> = body
            .get("parts")
            .and_then(Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| {
                        Some((
                            part.get("part_number")?.as_i64()? as i32,
                            part.get("etag")?.as_str()?.to_owned(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();

        if parts.is_empty() {
            return Err(ApiError::validation("parts", "\"parts\" cannot be empty."));
        }

        if state
            .storage
            .complete_multipart_upload(&key, &upload_id, &parts)
            .await
            .is_err()
        {
            state.storage.abort_multipart_upload(&key, &upload_id).await;

            return Err(ApiError::unprocessable(
                "The upload could not be assembled. Start it again.",
            )
            .with_code("upload_incomplete"));
        }
    }

    let stored_size = verify_stored(&state, &key, &body).await?;
    let page_count = body.get("page_count").and_then(Value::as_i64);
    let user_id = workspace.caller.user.id.clone();
    let mut db = workspace.open(&state)?;
    let sheet = sheet_id.clone();
    let hash = sha256.clone();

    let updated = blocking(move || {
        write_batch(&mut db, 1, |transaction, seq| {
            Ok(transaction.execute(
                "UPDATE sheets
                    SET sha256 = ?1, size = ?2,
                        page_count = COALESCE(?3, page_count),
                        uploaded_at = ?4, updated_at = ?4, change_seq = ?5, updated_by = ?6
                  WHERE id = ?7",
                rusqlite::params![hash, stored_size, page_count, now(), seq, user_id, sheet],
            )?)
        })
    })
    .await?;

    if updated == 0 {
        return Err(
            ApiError::not_found("No such sheet in this workspace.").with_code("sheet_not_found")
        );
    }

    Ok(Json(json!({
        "sheet_id": sheet_id,
        "sha256": sha256,
        "size": stored_size,
    })))
}

// -- Workspace assets --------------------------------------------------------------------------

pub async fn asset_url(
    State(state): State<AppState>,
    workspace: Workspace<Read>,
    Path((_, sha256)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let sha256 = sha256.to_lowercase();

    if !keys::is_valid_sha256(&sha256) {
        return Err(ApiError::not_found("No such asset.").with_code("asset_not_found"));
    }

    // The extension is part of the key, so the type has to be named to find the object.
    for content_type in keys::asset_content_types() {
        let Ok(key) = keys::asset_key(&workspace.id, &sha256, content_type) else {
            continue;
        };

        if state.storage.exists(&key).await {
            return Ok(Json(json!({
                "url": state.storage.presign_get(&key, DOWNLOAD_TTL).await?,
                "content_type": content_type,
                "expires_in": DOWNLOAD_TTL,
            })));
        }
    }

    Err(ApiError::not_found("That asset has not been uploaded.").with_code("asset_not_found"))
}

pub async fn asset_upload_url(
    State(state): State<AppState>,
    workspace: Workspace<Write>,
    Body(body): Body<Value>,
) -> ApiResult<Json<Value>> {
    let body = object(&body);
    let sha256 = require_string(&body, "sha256", 64)?.to_lowercase();
    let content_type = require_string(&body, "content_type", 64)?.to_lowercase();
    let size = body.get("size").and_then(Value::as_i64).unwrap_or_default();

    if !keys::is_valid_sha256(&sha256) {
        return Err(ApiError::validation(
            "sha256",
            "\"sha256\" must be 64 hex characters.",
        ));
    }

    if !(1..=MAX_ASSET_SIZE).contains(&size) {
        return Err(ApiError::validation(
            "size",
            format!("A background image must be between 1 and {MAX_ASSET_SIZE} bytes."),
        ));
    }

    let key = asset_key(&workspace.id, &sha256, &content_type)?;

    // Content addressing makes the "already uploaded" case free: identical bytes are here.
    if state.storage.exists(&key).await {
        return Ok(Json(
            json!({ "already_stored": true, "key": key, "asset": sha256 }),
        ));
    }

    let upload_id = state
        .storage
        .create_multipart_upload(&key, &content_type)
        .await?;

    Ok(Json(json!({
        "already_stored": false,
        "key": key,
        "asset": sha256,
        "upload_id": upload_id,
        "url": state.storage.presign_upload_part(&key, &upload_id, 1, UPLOAD_TTL).await?,
        "expires_in": UPLOAD_TTL,
    })))
}

pub async fn asset_complete(
    State(state): State<AppState>,
    workspace: Workspace<Write>,
    Body(body): Body<Value>,
) -> ApiResult<Json<Value>> {
    let body = object(&body);
    let sha256 = require_string(&body, "sha256", 64)?.to_lowercase();
    let content_type = require_string(&body, "content_type", 64)?.to_lowercase();
    let key = asset_key(&workspace.id, &sha256, &content_type)?;

    if let Some(upload_id) = optional_string(&body, "upload_id") {
        let etag = require_string(&body, "etag", 128)?;

        if state
            .storage
            .complete_multipart_upload(&key, &upload_id, &[(1, etag)])
            .await
            .is_err()
        {
            state.storage.abort_multipart_upload(&key, &upload_id).await;

            return Err(ApiError::unprocessable(
                "The upload could not be assembled. Start it again.",
            )
            .with_code("upload_incomplete"));
        }
    }

    let size = verify_stored(&state, &key, &body).await?;

    Ok(Json(json!({
        "asset": sha256,
        "content_type": content_type,
        "size": size,
    })))
}

fn asset_key(workspace_id: &str, sha256: &str, content_type: &str) -> ApiResult<String> {
    keys::asset_key(workspace_id, sha256, content_type)
        .map_err(|error| ApiError::unprocessable(error.to_string()).with_code("unsupported_type"))
}

/// The object has to be there, and it has to be the size the client said it would be. A file
/// that does not match what was declared is deleted rather than recorded.
async fn verify_stored(
    state: &AppState,
    key: &str,
    body: &serde_json::Map<String, Value>,
) -> ApiResult<i64> {
    let (stored, _) = state.storage.head(key).await.ok_or_else(|| {
        ApiError::unprocessable("No object was stored at that key.").with_code("upload_missing")
    })?;

    let declared = body.get("size").and_then(Value::as_i64).unwrap_or_default();

    if declared > 0 && stored != declared {
        state.storage.delete(key).await?;

        return Err(
            ApiError::unprocessable("The stored file does not match what was declared.")
                .with_code("checksum_mismatch")
                .with_details(json!({ "expected": declared, "stored": stored })),
        );
    }

    Ok(stored)
}
