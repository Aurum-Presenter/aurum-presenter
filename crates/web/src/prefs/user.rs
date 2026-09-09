//! Preferences that belong to a member across the whole workspace rather than to one song.
//!
//! The preferred part is the obvious one: a pianist wants the piano sheet every time, without
//! saying so once per song. It syncs, because it is a fact about the person, not the device.

use aurum_core::sheets::selection::Part;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::records::{Preference, alive};
use crate::db::{Database, DbError};
use crate::sync::SyncEngine;

const NAME: &str = "workspace";

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct UserPrefs {
    pub part: Option<Part>,
}

pub async fn read(db: &Database, user_id: &str) -> UserPrefs {
    let Some(row) = find(db, user_id).await else {
        return UserPrefs::default();
    };

    row.value
        .as_deref()
        .and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or_default()
}

pub async fn write(
    db: &Database,
    engine: &SyncEngine,
    user_id: &str,
    prefs: &UserPrefs,
) -> Result<(), DbError> {
    let id = find(db, user_id)
        .await
        .map(|row| row.id)
        .unwrap_or_else(crate::new_id);

    engine
        .record(
            "preferences",
            &id,
            "upsert",
            json!({
                "user_id": user_id,
                "scope_type": "workspace",
                "scope_id": null,
                "name": NAME,
                "value": serde_json::to_string(prefs).unwrap_or_else(|_| "{}".to_owned()),
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        )
        .await
}

/// Scanned rather than indexed: there is one of these per member, and the compound index the
/// per-song preferences use needs a scope id this row does not have.
async fn find(db: &Database, user_id: &str) -> Option<Preference> {
    alive(db.all::<Preference>("preferences").await.ok()?)
        .into_iter()
        .find(|row| row.user_id == user_id && row.scope_type == "workspace" && row.name == NAME)
}
