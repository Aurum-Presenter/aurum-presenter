//! Writing to the library.
//!
//! Every method here goes through the sync engine rather than the database directly, which is
//! what makes an edit durable and pushable in one step. The validation and the cycle check live
//! in `aurum-core`, so the form and the server agree about what a song may look like.

use serde_json::{Map, Value, json};
use wasm_bindgen::JsValue;

use crate::db::records::{Arrangement, Folder, Record, Song, SongPlacement, alive};
use crate::db::{Database, DbError};
use crate::sync::SyncEngine;

/// What a form hands back. Absent fields are cleared, not left alone — the form shows every one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SongInput {
    pub title: String,
    pub folder_id: Option<String>,
    pub artist: Option<String>,
    pub authors: Option<String>,
    pub subtitle: Option<String>,
    pub ccli_number: Option<String>,
    pub copyright: Option<String>,
    pub notes: Option<String>,
    pub original_key: Option<String>,
    pub tempo: Option<i64>,
    pub time_signature: Option<String>,
    pub duration_sec: Option<i64>,
    pub tags: Vec<String>,
    pub alt_titles: Vec<String>,
}

/// What a folder deletion does with the songs inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderSongs {
    MoveToParent,
    Archive,
}

#[derive(Clone)]
pub struct Library {
    db: Database,
    engine: SyncEngine,
}

impl Library {
    pub fn new(db: Database, engine: SyncEngine) -> Library {
        Library { db, engine }
    }

    // -- Songs ---------------------------------------------------------------------------------

    pub async fn create_song(&self, input: &SongInput) -> Result<String, DbError> {
        // UUIDv7 on the device: a song created offline keeps the identity it was born with, so
        // the eventual push is an upsert and never a second copy.
        let id = crate::new_id();
        let mut payload = payload_of(input);
        payload.insert("archived".to_owned(), json!(0));

        self.engine.record("songs", &id, "upsert", payload).await?;

        Ok(id)
    }

    pub async fn update_song(&self, id: &str, input: &SongInput) -> Result<(), DbError> {
        self.engine
            .record("songs", id, "upsert", payload_of(input))
            .await
    }

    pub async fn set_archived(&self, id: &str, archived: bool) -> Result<(), DbError> {
        self.engine
            .record(
                "songs",
                id,
                "upsert",
                field("archived", json!(i64::from(archived))),
            )
            .await
    }

    pub async fn move_song(&self, id: &str, folder_id: Option<&str>) -> Result<(), DbError> {
        self.engine
            .record("songs", id, "upsert", field("folder_id", json!(folder_id)))
            .await
    }

    /// Soft delete: it replicates, and Trash can put it back for thirty days.
    pub async fn delete_song(&self, id: &str) -> Result<(), DbError> {
        self.engine.record("songs", id, "delete", Map::new()).await
    }

    /// Undelete is an upsert, not a flag flip: the server treats a tombstone as final for a
    /// concurrent *edit*, but an explicit restore is a new write that clears it.
    pub async fn restore_song(&self, id: &str) -> Result<(), DbError> {
        let Some(song) = self.db.get::<Song>("songs", id).await? else {
            return Ok(());
        };

        self.engine
            .record("songs", id, "upsert", field("title", json!(song.title)))
            .await
    }

    pub async fn duplicate_song(&self, id: &str) -> Result<Option<String>, DbError> {
        let Some(song) = self.db.get::<Song>("songs", id).await? else {
            return Ok(None);
        };

        let copy = self
            .create_song(&SongInput {
                title: format!("{} (copy)", song.title),
                folder_id: song.folder_id.clone(),
                artist: song.artist.clone(),
                authors: song.authors.clone(),
                subtitle: song.subtitle.clone(),
                ccli_number: song.ccli_number.clone(),
                copyright: song.copyright.clone(),
                notes: song.notes.clone(),
                original_key: song.original_key.clone(),
                tempo: song.tempo,
                time_signature: song.time_signature.clone(),
                duration_sec: song.duration_sec,
                tags: list_of(song.tags.as_deref()),
                alt_titles: list_of(song.alt_titles.as_deref()),
            })
            .await?;

        for arrangement in alive(self.arrangements_of(id).await?) {
            self.engine
                .record(
                    "arrangements",
                    &crate::new_id(),
                    "upsert",
                    object(json!({
                        "song_id": copy,
                        "name": arrangement.name,
                        "body": arrangement.body,
                        "default_key": arrangement.default_key,
                        "is_default": arrangement.is_default,
                        "position": arrangement.position,
                        "source_notation": arrangement.source_notation,
                        "capo_hint": arrangement.capo_hint,
                    })),
                )
                .await?;
        }

        Ok(Some(copy))
    }

    // -- Folders -------------------------------------------------------------------------------

    pub async fn create_folder(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<String, DbError> {
        let id = crate::new_id();
        let siblings = self.folders_under(parent_id).await?;

        self.engine
            .record(
                "folders",
                &id,
                "upsert",
                object(json!({
                    "name": name.trim(),
                    "parent_id": parent_id,
                    "position": siblings.len(),
                })),
            )
            .await?;

        Ok(id)
    }

    pub async fn rename_folder(&self, id: &str, name: &str) -> Result<(), DbError> {
        self.engine
            .record("folders", id, "upsert", field("name", json!(name.trim())))
            .await
    }

    /// Refuses to hang a folder off its own descendant, which would strand the whole subtree.
    pub async fn move_folder(&self, id: &str, parent_id: Option<&str>) -> Result<(), MoveError> {
        let folders = alive(self.db.all::<Folder>("folders").await?);
        let pairs: Vec<(&str, Option<&str>)> = folders
            .iter()
            .map(|folder| (folder.id.as_str(), folder.parent_id.as_deref()))
            .collect();

        if aurum_core::library::validation::would_cycle(&pairs, id, parent_id) {
            return Err(MoveError::WouldCycle);
        }

        self.engine
            .record(
                "folders",
                id,
                "upsert",
                field("parent_id", json!(parent_id)),
            )
            .await?;

        Ok(())
    }

    /// Deleting a folder never hard-deletes a song. The caller has already asked which of the
    /// two outcomes the user wants, because silently archiving twelve songs is not recoverable
    /// by anyone who did not expect it.
    pub async fn delete_folder(&self, id: &str, songs: FolderSongs) -> Result<(), DbError> {
        let folder = self.db.get::<Folder>("folders", id).await?;
        let parent = folder.and_then(|folder| folder.parent_id);
        let contained = alive(
            self.db
                .by_index::<Song>("songs", "folder_id", &JsValue::from_str(id))
                .await?,
        );

        for song in contained {
            let mut payload = field("folder_id", json!(parent));

            if songs == FolderSongs::Archive {
                payload.insert("archived".to_owned(), json!(1));
            }

            self.engine
                .record("songs", &song.id, "upsert", payload)
                .await?;
        }

        for child in self.folders_under(Some(id)).await? {
            self.engine
                .record(
                    "folders",
                    &child.id,
                    "upsert",
                    field("parent_id", json!(parent)),
                )
                .await?;
        }

        self.engine
            .record("folders", id, "delete", Map::new())
            .await
    }

    /// A song can be filed in more than one folder. Adding the same placement twice writes the
    /// row it already has rather than a second one.
    pub async fn add_placement(&self, song_id: &str, folder_id: &str) -> Result<(), DbError> {
        let existing = alive(
            self.db
                .by_index::<SongPlacement>(
                    "song_placements",
                    "song_id",
                    &JsValue::from_str(song_id),
                )
                .await?,
        )
        .into_iter()
        .find(|placement| placement.folder_id == folder_id);

        let id = existing
            .map(|placement| placement.id)
            .unwrap_or_else(crate::new_id);

        self.engine
            .record(
                "song_placements",
                &id,
                "upsert",
                object(json!({ "song_id": song_id, "folder_id": folder_id })),
            )
            .await
    }

    pub async fn remove_placement(&self, placement_id: &str) -> Result<(), DbError> {
        self.engine
            .record("song_placements", placement_id, "delete", Map::new())
            .await
    }

    // -- Reads ---------------------------------------------------------------------------------

    pub async fn arrangements_of(&self, song_id: &str) -> Result<Vec<Arrangement>, DbError> {
        self.db
            .by_index("arrangements", "song_id", &JsValue::from_str(song_id))
            .await
    }

    async fn folders_under(&self, parent_id: Option<&str>) -> Result<Vec<Folder>, DbError> {
        Ok(alive(self.db.all::<Folder>("folders").await?)
            .into_iter()
            .filter(|folder| folder.parent_id.as_deref() == parent_id)
            .collect())
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum MoveError {
    #[error("A folder cannot be moved inside itself.")]
    WouldCycle,
    #[error(transparent)]
    Database(#[from] DbError),
}

/// Songs a folder shows: its home songs plus anything placed there as a second home.
pub fn songs_in_folder(
    songs: &[Song],
    placements: &[SongPlacement],
    folder_id: Option<&str>,
) -> Vec<Song> {
    let placed: Vec<&str> = placements
        .iter()
        .filter(|placement| {
            placement.folder_id.as_str() == folder_id.unwrap_or_default() && !placement.is_deleted()
        })
        .map(|placement| placement.song_id.as_str())
        .collect();

    songs
        .iter()
        .filter(|song| song.folder_id.as_deref() == folder_id || placed.contains(&song.id.as_str()))
        .cloned()
        .collect()
}

/// A JSON array column, read as the list it holds. Anything unreadable is an empty list rather
/// than an error: a tag list nobody can parse must not stop a song opening.
pub fn list_of(json: Option<&str>) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(json.unwrap_or("")).unwrap_or_default()
}

fn payload_of(input: &SongInput) -> Map<String, Value> {
    object(json!({
        "title": input.title.trim(),
        "folder_id": input.folder_id,
        "artist": blank(input.artist.as_deref()),
        "authors": blank(input.authors.as_deref()),
        "subtitle": blank(input.subtitle.as_deref()),
        "ccli_number": blank(input.ccli_number.as_deref()),
        "copyright": blank(input.copyright.as_deref()),
        "notes": blank(input.notes.as_deref()),
        "original_key": blank(input.original_key.as_deref()),
        "tempo": input.tempo,
        "time_signature": blank(input.time_signature.as_deref()),
        "duration_sec": input.duration_sec,
        "tags": serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".to_owned()),
        "alt_titles": serde_json::to_string(&input.alt_titles).unwrap_or_else(|_| "[]".to_owned()),
    }))
}

/// A field a person left blank is null, not an empty string: the difference shows up in every
/// "has an artist" check the app makes.
fn blank(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn field(name: &str, value: Value) -> Map<String, Value> {
    let mut payload = Map::new();
    payload.insert(name.to_owned(), value);

    payload
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::records::Sync;

    fn song(id: &str, folder: Option<&str>) -> Song {
        Song {
            id: id.to_owned(),
            folder_id: folder.map(str::to_owned),
            ..Song::default()
        }
    }

    fn placement(song: &str, folder: &str, deleted: bool) -> SongPlacement {
        SongPlacement {
            id: format!("{song}-{folder}"),
            song_id: song.to_owned(),
            folder_id: folder.to_owned(),
            sync: Sync {
                deleted_at: deleted.then(|| "2026-09-08T00:00:00.000Z".to_owned()),
                ..Sync::default()
            },
        }
    }

    #[test]
    fn a_folder_shows_its_own_songs_and_the_ones_placed_there() {
        let songs = [song("home", Some("f1")), song("visitor", Some("f2"))];
        let placements = [placement("visitor", "f1", false)];

        let listed = songs_in_folder(&songs, &placements, Some("f1"));

        assert_eq!(
            listed
                .iter()
                .map(|song| song.id.as_str())
                .collect::<Vec<_>>(),
            ["home", "visitor"]
        );
    }

    #[test]
    fn a_removed_placement_stops_showing_the_song() {
        let songs = [song("visitor", Some("f2"))];
        let placements = [placement("visitor", "f1", true)];

        assert!(songs_in_folder(&songs, &placements, Some("f1")).is_empty());
    }

    #[test]
    fn unfiled_is_a_folder_too() {
        let songs = [song("loose", None), song("filed", Some("f1"))];

        let listed = songs_in_folder(&songs, &[], None);

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "loose");
    }

    /// A tag list nobody can parse must not stop a song opening.
    #[test]
    fn a_list_column_that_cannot_be_read_is_empty_rather_than_fatal() {
        assert_eq!(list_of(Some(r#"["hymn","slow"]"#)), ["hymn", "slow"]);
        assert!(list_of(Some("not json")).is_empty());
        assert!(list_of(Some("")).is_empty());
        assert!(list_of(None).is_empty());
    }

    #[test]
    fn a_field_left_blank_is_null_rather_than_an_empty_string() {
        let payload = payload_of(&SongInput {
            title: "  Amazing Grace  ".to_owned(),
            artist: Some("   ".to_owned()),
            authors: Some(" John Newton ".to_owned()),
            ..SongInput::default()
        });

        assert_eq!(payload["title"], json!("Amazing Grace"));
        assert_eq!(payload["artist"], Value::Null);
        assert_eq!(payload["authors"], json!("John Newton"));
        assert_eq!(payload["tags"], json!("[]"), "an empty list, not null");
    }
}
