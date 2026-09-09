//! What this device should be holding, read out of the local database.
//!
//! Two sources: the sets that are coming up (or pinned), each in the key it will be played in,
//! and the songs the user has pinned by hand — less anything this device has been told to let
//! go of, which beats both.

use std::collections::{BTreeSet, HashMap};

use aurum_core::blobs::policy::{SheetFile, SheetWant, is_auto_pinned, wanted_sheets};
use aurum_core::chart::effective_key::KeyInputs;
use aurum_core::sheets::selection::{Part, SheetChoice};
use serde::{Deserialize, Serialize};

use crate::app::storage;
use crate::db::Database;
use crate::db::records::{Arrangement, Preference, Set, SetItem, Sheet, Song, alive};
use crate::prefs::song::SongPrefs;

/// What this device has been told not to keep.
///
/// Releasing is a decision about *this* device — a laptop with a full disk is not a reason for
/// the band's phones to stop holding Sunday's set — so it is local and never synced. It beats
/// every reason a file would otherwise be kept. Nothing is deleted by releasing; the files
/// simply stop being protected and the eviction pass may reclaim them.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct Released {
    #[serde(default)]
    pub sets: Vec<String>,
    #[serde(default)]
    pub songs: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Set,
    Song,
}

pub fn released(workspace_id: &str) -> Released {
    storage::read(&storage::released(workspace_id))
        .and_then(|held| serde_json::from_str(&held).ok())
        .unwrap_or_default()
}

pub fn release(workspace_id: &str, kind: Kind, id: &str) {
    let mut current = released(workspace_id);
    let list = match kind {
        Kind::Set => &mut current.sets,
        Kind::Song => &mut current.songs,
    };

    if !list.iter().any(|held| held == id) {
        list.push(id.to_owned());
    }

    write(workspace_id, &current);
}

pub fn keep_again(workspace_id: &str, kind: Kind, id: &str) {
    let mut current = released(workspace_id);
    let list = match kind {
        Kind::Set => &mut current.sets,
        Kind::Song => &mut current.songs,
    };

    list.retain(|held| held != id);
    write(workspace_id, &current);
}

fn write(workspace_id: &str, value: &Released) {
    // No storage means no release list; the pin policy simply keeps what it would have kept.
    if let Ok(held) = serde_json::to_string(value) {
        storage::write(&storage::released(workspace_id), &held);
    }
}

/// Everything the decision reads. Separated from the reading so the decision can be tested.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WantedRows {
    pub sets: Vec<Set>,
    pub items: Vec<SetItem>,
    pub sheets: Vec<Sheet>,
    pub songs: Vec<Song>,
    pub arrangements: Vec<Arrangement>,
    pub prefs: HashMap<String, SongPrefs>,
}

pub fn decide(
    rows: &WantedRows,
    part: Option<Part>,
    released: &Released,
    now_ms: i64,
) -> BTreeSet<String> {
    // The key a sheet will actually be read in, which is what the selection is made against.
    let key_for = |song_id: &str, override_key: Option<&str>| {
        let song = rows.songs.iter().find(|song| song.id == song_id);
        let preference = rows.prefs.get(song_id);
        let for_song: Vec<&Arrangement> = rows
            .arrangements
            .iter()
            .filter(|row| row.song_id == song_id)
            .collect();

        let arrangement = for_song
            .iter()
            .find(|row| {
                Some(row.id.as_str())
                    == preference.and_then(|prefs| prefs.arrangement_id.as_deref())
            })
            .or_else(|| for_song.iter().find(|row| row.is_default == 1))
            .or(for_song.first());

        KeyInputs {
            set_override: override_key,
            preferred: preference.and_then(|prefs| prefs.preferred_key.as_deref()),
            arrangement_default: arrangement.and_then(|row| row.default_key.as_deref()),
            song_original: song.and_then(|song| song.original_key.as_deref()),
        }
        .effective()
        .key
    };

    let kept_sets: BTreeSet<&str> = rows
        .sets
        .iter()
        .filter(|set| is_auto_pinned(set.pinned == 1, set.scheduled_for.as_deref(), now_ms))
        .filter(|set| !released.sets.iter().any(|held| held == &set.id))
        .map(|set| set.id.as_str())
        .collect();

    let mut wants: Vec<SheetWant<'_>> = Vec::new();

    for item in &rows.items {
        let Some(song_id) = item.song_id.as_deref() else {
            continue;
        };

        if !kept_sets.contains(item.set_id.as_str())
            || released.songs.iter().any(|held| held == song_id)
        {
            continue;
        }

        wants.push(SheetWant {
            song_id,
            key: key_for(song_id, item.key_override.as_deref()),
        });
    }

    for (song_id, preference) in &rows.prefs {
        if preference.pinned && !released.songs.iter().any(|held| held == song_id) {
            wants.push(SheetWant {
                song_id,
                key: key_for(song_id, None),
            });
        }
    }

    let files: Vec<SheetFile<'_>> = rows
        .sheets
        .iter()
        .map(|sheet| SheetFile {
            choice: SheetChoice {
                id: &sheet.id,
                sheet_key: sheet.sheet_key.as_deref(),
                part: sheet.part.as_deref(),
                position: sheet.position,
                deleted: sheet.sync.deleted_at.is_some(),
            },
            song_id: &sheet.song_id,
            uploaded: sheet.sha256.is_some(),
        })
        .collect();

    wanted_sheets(&files, &wants, part)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// The same decision, against the database.
pub async fn compute(
    db: &Database,
    user_id: &str,
    part: Option<Part>,
    released: &Released,
) -> BTreeSet<String> {
    let prefs = alive(
        db.all::<Preference>("preferences")
            .await
            .unwrap_or_default(),
    )
    .into_iter()
    .filter(|row| row.user_id == user_id && row.name == "chart")
    .filter_map(|row| {
        let scope = row.scope_id?;
        let value = serde_json::from_str(row.value.as_deref()?).ok()?;

        Some((scope, value))
    })
    .collect();

    let rows = WantedRows {
        sets: alive(db.all("sets").await.unwrap_or_default()),
        items: alive(db.all("set_items").await.unwrap_or_default()),
        sheets: alive(db.all("sheets").await.unwrap_or_default()),
        songs: alive(db.all("songs").await.unwrap_or_default()),
        arrangements: alive(db.all("arrangements").await.unwrap_or_default()),
        prefs,
    };

    decide(&rows, part, released, crate::now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Midday on 2026-09-09, which is what the dates below are measured from.
    const NOW: i64 = 1_788_912_000_000;

    fn rows() -> WantedRows {
        WantedRows {
            sets: vec![Set {
                id: "set".to_owned(),
                name: "Sunday".to_owned(),
                scheduled_for: Some("2026-09-13".to_owned()),
                ..Set::default()
            }],
            items: vec![SetItem {
                id: "item".to_owned(),
                set_id: "set".to_owned(),
                rank: "a".to_owned(),
                song_id: Some("song".to_owned()),
                ..SetItem::default()
            }],
            sheets: vec![Sheet {
                id: "sheet".to_owned(),
                song_id: "song".to_owned(),
                sheet_key: Some("G".to_owned()),
                part: Some("lead".to_owned()),
                sha256: Some("abc".to_owned()),
                ..Sheet::default()
            }],
            songs: vec![Song {
                id: "song".to_owned(),
                title: "Cornerstone".to_owned(),
                original_key: Some("G".to_owned()),
                ..Song::default()
            }],
            arrangements: Vec::new(),
            prefs: HashMap::new(),
        }
    }

    #[test]
    fn a_set_that_is_coming_up_brings_its_sheets() {
        assert!(decide(&rows(), None, &Released::default(), NOW).contains("sheet"));
    }

    #[test]
    fn a_set_long_past_does_not() {
        let mut rows = rows();

        rows.sets[0].scheduled_for = Some("2025-01-01".to_owned());

        assert!(decide(&rows, None, &Released::default(), NOW).is_empty());
    }

    /// Releasing is about this device and beats every reason the file would be kept.
    #[test]
    fn a_released_set_is_let_go_of_even_though_it_is_this_sunday() {
        let released = Released {
            sets: vec!["set".to_owned()],
            songs: Vec::new(),
        };

        assert!(decide(&rows(), None, &released, NOW).is_empty());
    }

    #[test]
    fn a_released_song_is_let_go_of_inside_a_set_that_is_kept() {
        let released = Released {
            sets: Vec::new(),
            songs: vec!["song".to_owned()],
        };

        assert!(decide(&rows(), None, &released, NOW).is_empty());
    }

    #[test]
    fn a_pinned_song_is_kept_with_no_set_at_all() {
        let mut rows = rows();

        rows.sets.clear();
        rows.items.clear();
        rows.prefs.insert(
            "song".to_owned(),
            SongPrefs {
                pinned: true,
                ..SongPrefs::default()
            },
        );

        assert!(decide(&rows, None, &Released::default(), NOW).contains("sheet"));
    }

    /// A row with no file behind it has nothing to download, so it is not wanted.
    #[test]
    fn a_sheet_that_has_never_been_uploaded_is_not_wanted() {
        let mut rows = rows();

        rows.sheets[0].sha256 = None;

        assert!(decide(&rows, None, &Released::default(), NOW).is_empty());
    }

    /// The set's key decides which sheet is wanted, not the song's — that is the whole reason
    /// the key is resolved here rather than at download time.
    #[test]
    fn the_sets_key_decides_which_sheet_comes_down() {
        let mut rows = rows();

        rows.sheets.push(Sheet {
            id: "in-a".to_owned(),
            song_id: "song".to_owned(),
            sheet_key: Some("A".to_owned()),
            part: Some("lead".to_owned()),
            position: 1,
            sha256: Some("def".to_owned()),
            ..Sheet::default()
        });

        rows.items[0].key_override = Some("A".to_owned());

        let wanted = decide(&rows, None, &Released::default(), NOW);

        assert!(
            wanted.contains("in-a"),
            "the sheet in the key it will be played in"
        );
        assert!(!wanted.contains("sheet"));
    }
}
