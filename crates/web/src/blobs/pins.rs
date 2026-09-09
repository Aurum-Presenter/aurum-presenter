//! What this device is keeping on purpose, and what each one costs.
//!
//! Read when a pinned download will not fit. The app never decides which of a musician's pins
//! matters least, so it shows them, largest first, and lets them choose (business rule 10).

use aurum_core::blobs::policy::{
    CachedSheet, HeldPin, PinSources, PinnedSet, PinnedSong, SetSong, held_pins,
};

use crate::db::Database;
use crate::db::records::{BlobRecord, Preference, Set, SetItem, Sheet, Song, alive};
use crate::prefs::song::SongPrefs;

pub async fn held(db: &Database, user_id: &str) -> Vec<HeldPin> {
    let sets: Vec<Set> = alive(db.all("sets").await.unwrap_or_default());
    let items: Vec<SetItem> = alive(db.all("set_items").await.unwrap_or_default());
    let sheets: Vec<Sheet> = alive(db.all("sheets").await.unwrap_or_default());
    let songs: Vec<Song> = alive(db.all("songs").await.unwrap_or_default());
    let blobs: Vec<BlobRecord> = db.all("blobs").await.unwrap_or_default();

    // Bytes are counted from what is actually on the device, by the song the file belongs to.
    let cached: Vec<CachedSheet<'_>> = blobs
        .iter()
        .filter_map(|blob| {
            let sheet = sheets.iter().find(|sheet| sheet.id == blob.sheet_id)?;

            Some(CachedSheet {
                song_id: sheet.song_id.as_str(),
                bytes: blob.size.max(0) as u64,
            })
        })
        .collect();

    let pinned: Vec<(String, SongPrefs)> = alive(
        db.all::<Preference>("preferences")
            .await
            .unwrap_or_default(),
    )
    .into_iter()
    .filter(|row| row.user_id == user_id && row.name == "chart")
    .filter_map(|row| {
        let scope = row.scope_id?;
        let prefs: SongPrefs = serde_json::from_str(row.value.as_deref()?).ok()?;

        prefs.pinned.then_some((scope, prefs))
    })
    .collect();

    let sources = PinSources {
        sets: sets
            .iter()
            .map(|set| PinnedSet {
                id: &set.id,
                name: &set.name,
                pinned: set.pinned == 1,
                scheduled_for: set.scheduled_for.as_deref(),
            })
            .collect(),
        items: items
            .iter()
            .map(|item| SetSong {
                set_id: &item.set_id,
                song_id: item.song_id.as_deref(),
            })
            .collect(),
        cached,
        pinned_songs: pinned
            .iter()
            .filter_map(|(song_id, _)| {
                let song = songs.iter().find(|song| &song.id == song_id)?;

                Some(PinnedSong {
                    song_id: song.id.as_str(),
                    title: song.title.as_str(),
                })
            })
            .collect(),
    };

    held_pins(&sources, crate::now_ms())
}
