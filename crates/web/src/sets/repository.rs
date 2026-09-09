//! Reading and writing sets.
//!
//! Every write here is local-first like the rest of the app. Ordering uses fractional ranks, so
//! adding an item touches one row and reordering touches exactly the row that moved — which is
//! what lets two people reorder the same set offline and have both moves survive.

use aurum_core::sets::rank::{rank_between, rank_for_move};
use serde_json::{Map, Value, json};

use crate::db::records::{Set as SetRecord, SetItem, Song, alive};
use crate::db::{Database, DbError};
use crate::sync::SyncEngine;

/// What a set is created or renamed with. The date is what drives the 14-day auto-pin, so it is
/// a first-class field rather than another note.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetInput {
    pub name: String,
    pub scheduled_for: Option<String>,
    pub venue: Option<String>,
    pub notes: Option<String>,
    pub assigned_members: Vec<String>,
}

/// The kinds of item a set can hold besides a song. Each one is a thing that happens in a
/// service and has to sit in the running order at the right moment.
pub const ITEM_TYPES: [(&str, &str); 6] = [
    ("announcement", "Announcement"),
    ("scripture", "Scripture"),
    ("prayer", "Prayer"),
    ("video", "Video cue"),
    ("blank", "Blank / logo"),
    ("text", "Free text"),
];

#[derive(Clone)]
pub struct Sets {
    db: Database,
    engine: SyncEngine,
}

fn blank(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

impl Sets {
    pub fn new(db: Database, engine: SyncEngine) -> Sets {
        Sets { db, engine }
    }

    pub async fn create(&self, input: &SetInput) -> Result<String, DbError> {
        let id = crate::new_id();

        self.engine
            .record(
                "sets",
                &id,
                "upsert",
                object(json!({
                    "name": input.name.trim(),
                    "scheduled_for": blank(input.scheduled_for.as_deref()),
                    "venue": blank(input.venue.as_deref()),
                    "notes": blank(input.notes.as_deref()),
                    "assigned_members": serde_json::to_string(&input.assigned_members)
                        .unwrap_or_else(|_| "[]".to_owned()),
                    "pinned": 0,
                })),
            )
            .await?;

        Ok(id)
    }

    pub async fn update(&self, id: &str, changes: Map<String, Value>) -> Result<(), DbError> {
        self.engine.record("sets", id, "upsert", changes).await
    }

    pub async fn remove(&self, id: &str) -> Result<(), DbError> {
        self.engine.record("sets", id, "delete", Map::new()).await
    }

    /// Business rule 6: items and overrides come along, the date does not. A copied set is next
    /// month's service, and dating it today would pin it and put it on the wrong Sunday.
    pub async fn duplicate(&self, id: &str) -> Result<Option<String>, DbError> {
        let Some(original) = self.db.get::<SetRecord>("sets", id).await? else {
            return Ok(None);
        };

        let copy = self
            .create(&SetInput {
                name: format!("{} (copy)", original.name),
                scheduled_for: None,
                venue: original.venue,
                notes: original.notes,
                assigned_members: parse_list(original.assigned_members.as_deref()),
            })
            .await?;

        for item in self.items(id).await? {
            self.engine
                .record(
                    "set_items",
                    &crate::new_id(),
                    "upsert",
                    object(json!({
                        "set_id": copy,
                        "rank": item.rank,
                        "song_id": item.song_id,
                        "item_type": item.item_type,
                        "content": item.content,
                        "title_snapshot": item.title_snapshot,
                        "key_override": item.key_override,
                        "capo_override": item.capo_override,
                        "arrangement_id": item.arrangement_id,
                        "sheet_part_override": item.sheet_part_override,
                        "sections": item.sections,
                        "note": item.note,
                    })),
                )
                .await?;
        }

        Ok(Some(copy))
    }

    /// A set's items, in the order they are played.
    pub async fn items(&self, set_id: &str) -> Result<Vec<SetItem>, DbError> {
        let mut items = alive(
            self.db
                .by_index::<SetItem>("set_items", "set_id", &set_id.into())
                .await?,
        );

        // Ranks are strings compared as strings; that is the whole point of them.
        items.sort_by(|left, right| left.rank.cmp(&right.rank));

        Ok(items)
    }

    /// Appends songs in the order they were chosen, each with the title it had at the time —
    /// so a set stays readable after a song is renamed or deleted.
    pub async fn add_songs(&self, set_id: &str, songs: &[Song]) -> Result<(), DbError> {
        let mut last = self
            .items(set_id)
            .await?
            .last()
            .map(|item| item.rank.clone());

        for song in songs {
            let Ok(rank) = rank_between(last.as_deref(), None) else {
                return Ok(());
            };

            self.engine
                .record(
                    "set_items",
                    &crate::new_id(),
                    "upsert",
                    object(json!({
                        "set_id": set_id,
                        "rank": rank,
                        "song_id": song.id,
                        "title_snapshot": song.title,
                    })),
                )
                .await?;

            last = Some(rank);
        }

        Ok(())
    }

    pub async fn add_item(
        &self,
        set_id: &str,
        item_type: &str,
        content: &str,
    ) -> Result<(), DbError> {
        let last = self
            .items(set_id)
            .await?
            .last()
            .map(|item| item.rank.clone());

        let Ok(rank) = rank_between(last.as_deref(), None) else {
            return Ok(());
        };

        self.engine
            .record(
                "set_items",
                &crate::new_id(),
                "upsert",
                object(json!({
                    "set_id": set_id,
                    "rank": rank,
                    "item_type": item_type,
                    "content": content,
                })),
            )
            .await
    }

    /// One row moves, and only one. Two people reordering the same set offline both keep their
    /// move, because neither touched the other's row.
    pub async fn move_item(&self, set_id: &str, from: usize, to: usize) -> Result<(), DbError> {
        let items = self.items(set_id).await?;
        let ranks: Vec<String> = items.iter().map(|item| item.rank.clone()).collect();

        let (Ok(Some(rank)), Some(moved)) = (rank_for_move(&ranks, from, to), items.get(from))
        else {
            return Ok(());
        };

        self.engine
            .record(
                "set_items",
                &moved.id,
                "upsert",
                object(json!({ "rank": rank })),
            )
            .await
    }

    pub async fn update_item(&self, id: &str, changes: Map<String, Value>) -> Result<(), DbError> {
        self.engine.record("set_items", id, "upsert", changes).await
    }

    pub async fn remove_item(&self, id: &str) -> Result<(), DbError> {
        self.engine
            .record("set_items", id, "delete", Map::new())
            .await
    }
}

/// A JSON array column read as the list it holds. Anything unreadable is an empty list: a
/// member list nobody can parse must not stop a set opening.
pub fn parse_list(json: Option<&str>) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(json.unwrap_or("")).unwrap_or_default()
}
