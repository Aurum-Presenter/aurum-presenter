//! Per-user, per-song reading preferences.
//!
//! These sync, but they are never shared: the row is keyed by user, and every member has their
//! own for the same song.

use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen::JsValue;

use crate::db::records::Preference;
use crate::db::{Database, DbError};
use crate::sync::SyncEngine;

const NAME: &str = "chart";

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SongPrefs {
    pub preferred_key: Option<String>,
    /// `None` means "not chosen", which is not the same as 0 — the arranger's capo hint fills it.
    pub capo: Option<i64>,
    pub arrangement_id: Option<String>,
    /// Kept offline on purpose: this song's sheets are downloaded and never evicted.
    #[serde(default)]
    pub pinned: bool,
}

pub async fn read(db: &Database, user_id: &str, song_id: &str) -> SongPrefs {
    let Ok(Some(row)) = find(db, user_id, song_id).await else {
        return SongPrefs::default();
    };

    // A preference nobody can parse is no preference, not a broken song page.
    row.value
        .as_deref()
        .and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or_default()
}

/// Writes through the outbox like every other change, so setting a key on a plane is no
/// different from setting one at home.
pub async fn write(
    db: &Database,
    engine: &SyncEngine,
    user_id: &str,
    song_id: &str,
    prefs: &SongPrefs,
) -> Result<(), DbError> {
    let existing = find(db, user_id, song_id).await?;
    let id = existing.map(|row| row.id).unwrap_or_else(crate::new_id);

    engine
        .record(
            "preferences",
            &id,
            "upsert",
            json!({
                "user_id": user_id,
                "scope_type": "song",
                "scope_id": song_id,
                "name": NAME,
                "value": serde_json::to_string(prefs).unwrap_or_else(|_| "{}".to_owned()),
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        )
        .await
}

/// Found by the compound index the store carries, which is the same shape the server's unique
/// constraint has: one row per user, scope and name.
async fn find(db: &Database, user_id: &str, song_id: &str) -> Result<Option<Preference>, DbError> {
    let key = js_sys::Array::of4(
        &JsValue::from_str(user_id),
        &JsValue::from_str("song"),
        &JsValue::from_str(song_id),
        &JsValue::from_str(NAME),
    );

    let rows: Vec<Preference> = db.by_index("preferences", "scope", &key).await?;

    Ok(rows.into_iter().find(|row| row.sync.deleted_at.is_none()))
}
