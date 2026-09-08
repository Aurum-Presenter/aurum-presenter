//! The records the device holds.
//!
//! These mirror the server's columns exactly, because that is what sync moves: a row arrives as
//! the server stored it, and is written to the store of the same name. The one liberty taken is
//! that JSON-encoded columns stay `String` here, as they are in the database — they are read
//! whole and never queried across rows.

use serde::{Deserialize, Serialize};

/// Columns the server owns. A local edit never sets these; they arrive on the next pull.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Sync {
    pub updated_at: String,
    pub change_seq: i64,
    pub deleted_at: Option<String>,
    pub updated_by: Option<String>,
}

/// Every synced record carries the server's columns and can say whether it is a tombstone.
pub trait Record {
    fn id(&self) -> &str;
    fn sync(&self) -> &Sync;

    fn is_deleted(&self) -> bool {
        self.sync().deleted_at.is_some()
    }
}

macro_rules! record {
    ($name:ident) => {
        impl Record for $name {
            fn id(&self) -> &str {
                &self.id
            }

            fn sync(&self) -> &Sync {
                &self.sync
            }
        }
    };
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Folder {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub position: i64,
    #[serde(flatten)]
    pub sync: Sync,
}
record!(Folder);

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Song {
    pub id: String,
    /// The home folder. `None` is "unfiled", which is a permanent state, not a broken one.
    pub folder_id: Option<String>,
    pub title: String,
    pub subtitle: Option<String>,
    pub authors: Option<String>,
    pub ccli_number: Option<String>,
    pub copyright: Option<String>,
    pub notes: Option<String>,
    pub original_key: Option<String>,
    pub tempo: Option<i64>,
    pub time_signature: Option<String>,
    /// A JSON array, stored whole: read whole and never queried across songs.
    pub tags: Option<String>,
    pub artist: Option<String>,
    /// A JSON array: the other names a congregation knows this song by.
    pub alt_titles: Option<String>,
    pub duration_sec: Option<i64>,
    /// Out of the way, not gone. Excluded from lists and search unless asked for.
    pub archived: i64,
    #[serde(flatten)]
    pub sync: Sync,
}
record!(Song);

/// A folder a song appears in besides its home folder.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SongPlacement {
    pub id: String,
    pub song_id: String,
    pub folder_id: String,
    #[serde(flatten)]
    pub sync: Sync,
}
record!(SongPlacement);

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Arrangement {
    pub id: String,
    pub song_id: String,
    pub name: String,
    /// ChordPro. The canonical chart storage — never a PDF.
    pub body: String,
    pub default_key: Option<String>,
    pub is_default: i64,
    pub position: i64,
    /// What was pasted, before conversion.
    pub source_notation: String,
    /// The pre-conversion text, kept for one undo and for debugging a bad conversion.
    pub source_text: Option<String>,
    /// The arranger's suggested capo, 0–11. A reader's own capo lives in their preferences.
    pub capo_hint: Option<i64>,
    #[serde(flatten)]
    pub sync: Sync,
}
record!(Arrangement);

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Sheet {
    pub id: String,
    pub song_id: String,
    /// One of the twelve keys, or `None` for a sheet that suits any key.
    pub sheet_key: Option<String>,
    pub part: Option<String>,
    pub position: i64,
    /// `None` means the sheet applies to every arrangement of the song.
    pub arrangement_id: Option<String>,
    pub label: Option<String>,
    pub filename: Option<String>,
    /// `None` until the file has been uploaded. That is the "not downloaded" state, not an error.
    pub sha256: Option<String>,
    pub size: Option<i64>,
    pub page_count: Option<i64>,
    pub mime_type: String,
    pub uploaded_at: Option<String>,
    /// When this sheet's file was last replaced by one with a different number of pages. Marks
    /// made before that instant are kept and flagged — normalised coordinates survive a zoom and
    /// a rotate, but not pages moving underneath them.
    pub pages_changed_at: Option<String>,
    #[serde(flatten)]
    pub sync: Sync,
}
record!(Sheet);

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Annotation {
    pub id: String,
    pub sheet_id: String,
    pub page: i64,
    pub strokes: String,
    pub scope: String,
    pub author_id: String,
    #[serde(flatten)]
    pub sync: Sync,
}
record!(Annotation);

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Set {
    pub id: String,
    pub name: String,
    /// The service or gig date. `None` is allowed, but it is what drives the 14-day auto-pin.
    pub scheduled_for: Option<String>,
    pub notes: Option<String>,
    pub venue: Option<String>,
    /// A JSON array of user ids. Display only — being named on a set grants nothing.
    pub assigned_members: Option<String>,
    pub pinned: i64,
    #[serde(flatten)]
    pub sync: Sync,
}
record!(Set);

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SetItem {
    pub id: String,
    pub set_id: String,
    /// A fractional rank, so two offline reorders merge by string order.
    pub rank: String,
    /// An item is a song, or it carries its own content. Never both, never neither.
    pub song_id: Option<String>,
    pub item_type: Option<String>,
    pub content: Option<String>,
    /// Kept so a set stays readable after its song is deleted.
    pub title_snapshot: Option<String>,
    pub key_override: Option<String>,
    pub capo_override: Option<i64>,
    pub arrangement_id: Option<String>,
    pub sheet_part_override: Option<String>,
    /// A JSON array of section indices actually played; `None` means the whole chart.
    pub sections: Option<String>,
    pub note: Option<String>,
    #[serde(flatten)]
    pub sync: Sync,
}
record!(SetItem);

/// How the audience screen looks. Shared by the workspace, like the songs are.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct PresenterTheme {
    pub id: String,
    pub name: String,
    pub is_default: i64,
    pub font_family: String,
    pub font_size_vh: f64,
    pub text_color: String,
    pub background_kind: String,
    pub background_value: String,
    pub align: String,
    pub safe_area_pct: f64,
    pub show_section_labels: i64,
    #[serde(flatten)]
    pub sync: Sync,
}
record!(PresenterTheme);

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Preference {
    pub id: String,
    pub user_id: String,
    pub scope_type: String,
    pub scope_id: Option<String>,
    pub name: String,
    pub value: Option<String>,
    #[serde(flatten)]
    pub sync: Sync,
}
record!(Preference);

// -- Local only ------------------------------------------------------------------------------

/// A durable local mutation, replayed to the server when connectivity returns.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct OutboxOp {
    /// Generated by the store, which is what gives the outbox its order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    pub op_id: String,
    pub table: String,
    pub record_id: String,
    pub op: String,
    pub payload: serde_json::Value,
    /// The `updated_at` this device last saw for the record. The server uses it to decide
    /// whether anything changed underneath the edit, and so whether a conflict is warranted.
    pub base_updated_at: Option<String>,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SyncState {
    pub key: String,
    pub change_seq: i64,
    pub last_pull_at: Option<String>,
    pub last_push_at: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ConflictRecord {
    pub id: String,
    pub table: String,
    pub record_id: String,
    pub field: String,
    pub losing_value: Option<String>,
    pub at: String,
    pub reviewed_at: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BlobRecord {
    pub sheet_id: String,
    pub sha256: String,
    pub size: i64,
    pub pin_reason: String,
    pub cached_at: String,
}

/// A file waiting to be uploaded. Kept until the server has it, so nothing is lost.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct UploadRecord {
    pub sheet_id: String,
    pub sha256: String,
    pub size: i64,
    pub filename: String,
    pub page_count: Option<i64>,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub queued_at: String,
}

/// A running session, mirrored locally on every revision so a control window that crashes can
/// offer to resume. Never synced: a session is local and ephemeral by design.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LiveSession {
    pub session_id: String,
    pub state: serde_json::Value,
    pub updated_at: String,
}

/// What was shown and when. The last twenty sessions, on this device only.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionLogEntry {
    pub session_id: String,
    pub set_id: Option<String>,
    pub set_name: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub song_ids: Vec<String>,
    pub events: Vec<SessionLogEvent>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionLogEvent {
    pub at: String,
    pub index: usize,
    pub title: String,
}

/// The rows a list wants: everything that is not a tombstone.
pub fn alive<T: Record>(records: Vec<T>) -> Vec<T> {
    records
        .into_iter()
        .filter(|record| !record.is_deleted())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tombstone_is_not_in_a_list() {
        let deleted = Song {
            id: "gone".to_owned(),
            sync: Sync {
                deleted_at: Some("2026-09-08T00:00:00.000Z".to_owned()),
                ..Sync::default()
            },
            ..Song::default()
        };
        let kept = Song {
            id: "here".to_owned(),
            ..Song::default()
        };

        assert!(deleted.is_deleted());
        assert!(!kept.is_deleted());

        let listed = alive(vec![deleted, kept]);

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "here");
    }

    /// The server's columns are part of the record, not a nested object: a row arrives flat and
    /// is written flat.
    #[test]
    fn the_servers_columns_sit_alongside_the_rest() {
        let json = serde_json::json!({
            "id": "s1",
            "title": "Amazing Grace",
            "archived": 0,
            "updated_at": "2026-09-08T00:00:00.000Z",
            "change_seq": 7,
            "deleted_at": null,
            "updated_by": "ada",
        });

        let song: Song = serde_json::from_value(json).expect("a song");

        assert_eq!(song.title, "Amazing Grace");
        assert_eq!(song.sync.change_seq, 7);
        assert_eq!(song.sync.updated_by.as_deref(), Some("ada"));

        let back = serde_json::to_value(&song).expect("json");

        assert_eq!(back["change_seq"], 7, "and goes back out flat");
    }
}
