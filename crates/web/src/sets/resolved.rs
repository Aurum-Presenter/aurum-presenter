//! A set, resolved into what each item actually shows.
//!
//! The set's key override wins over the member's own preferred key (business rule 4) — that is
//! the whole point of a band deciding a key for the night — and everything below it stays
//! personal. The resolution itself is a pure function of the rows, so it is tested as one.

use std::collections::HashMap;

use aurum_core::chart::effective_key::{KeyInputs, KeySource};
use aurum_core::chart::notes::Key;
use leptos::prelude::*;

use crate::app::use_workspace;
use crate::db::live::live_query;
// Aliased: `Set` is also the reactive trait that gives a signal its `set` method.
use crate::db::records::{Arrangement, Preference, Set as SetRecord, SetItem, Song, alive};
use crate::prefs::song::SongPrefs;

/// One line of a set, with everything the screen needs already decided.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedItem {
    pub item: SetItem,
    pub song: Option<Song>,
    pub arrangement: Option<Arrangement>,
    /// The key the chart is written in, which transposition measures from.
    pub written: Option<Key>,
    /// The key this reader sees.
    pub key: Option<Key>,
    pub source: KeySource,
    pub capo: i64,
    pub title: String,
    /// The item points at a song that has since been deleted. The line stays: a running order
    /// with a hole in it is worse than one that says what is missing.
    pub missing: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedSet {
    pub set: Option<SetRecord>,
    pub items: Vec<ResolvedItem>,
}

/// Everything read out of the database for one set, before any of it is resolved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SetRows {
    pub set: Option<SetRecord>,
    pub items: Vec<SetItem>,
    pub songs: Vec<Song>,
    pub arrangements: Vec<Arrangement>,
    /// This member's chart preferences, by song.
    pub prefs: HashMap<String, SongPrefs>,
}

fn content_title(item: &SetItem) -> String {
    let Some(kind) = item.item_type.as_deref() else {
        return "Untitled".to_owned();
    };

    let first = item
        .content
        .as_deref()
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .trim();

    if first.is_empty() {
        kind.to_owned()
    } else {
        first.to_owned()
    }
}

/// The rule, separated from the reading so it can be tested without a browser.
pub fn resolve(rows: &SetRows) -> ResolvedSet {
    let items = rows
        .items
        .iter()
        .map(|item| {
            let song = item
                .song_id
                .as_deref()
                .and_then(|id| rows.songs.iter().find(|song| song.id == id));
            let present = song.is_some_and(|song| song.sync.deleted_at.is_none());
            let preference = item.song_id.as_deref().and_then(|id| rows.prefs.get(id));

            let for_song: Vec<&Arrangement> = rows
                .arrangements
                .iter()
                .filter(|row| Some(row.song_id.as_str()) == item.song_id.as_deref())
                .collect();

            // The same order the song page opens in, so a set and a song never disagree about
            // which arrangement "this song" means.
            let arrangement = for_song
                .iter()
                .find(|row| Some(row.id.as_str()) == item.arrangement_id.as_deref())
                .or_else(|| {
                    for_song.iter().find(|row| {
                        Some(row.id.as_str())
                            == preference.and_then(|prefs| prefs.arrangement_id.as_deref())
                    })
                })
                .or_else(|| for_song.iter().find(|row| row.is_default == 1))
                .or(for_song.first())
                .copied();

            let inputs = KeyInputs {
                set_override: item.key_override.as_deref(),
                preferred: preference.and_then(|prefs| prefs.preferred_key.as_deref()),
                arrangement_default: arrangement.and_then(|row| row.default_key.as_deref()),
                song_original: song.and_then(|song| song.original_key.as_deref()),
            };

            let effective = inputs.effective();

            ResolvedItem {
                song: present.then(|| song.cloned()).flatten(),
                arrangement: present.then(|| arrangement.cloned()).flatten(),
                written: inputs.source_key(),
                key: effective.key,
                source: effective.source,
                capo: item
                    .capo_override
                    .or_else(|| preference.and_then(|prefs| prefs.capo))
                    .or_else(|| arrangement.and_then(|row| row.capo_hint))
                    .unwrap_or(0),
                title: item
                    .title_snapshot
                    .clone()
                    .or_else(|| song.map(|song| song.title.clone()))
                    .unwrap_or_else(|| content_title(item)),
                missing: item.song_id.is_some() && !present,
                item: item.clone(),
            }
        })
        .collect();

    ResolvedSet {
        set: rows.set.clone(),
        items,
    }
}

/// The live version: re-resolves whenever anything it read is written.
pub fn use_resolved_set(set_id: Signal<String>) -> Signal<ResolvedSet> {
    let context = use_workspace();
    let db = context.db;
    let me = context.me;

    let rows = live_query(
        &["sets", "set_items", "songs", "arrangements", "preferences"],
        move || {
            let db = db.get();
            let set_id = set_id.get();
            let user_id = me.get().id;

            async move {
                let Some(db) = db else {
                    return SetRows::default();
                };

                let mut items = alive(
                    db.by_index::<SetItem>("set_items", "set_id", &set_id.as_str().into())
                        .await
                        .unwrap_or_default(),
                );

                items.sort_by(|left, right| left.rank.cmp(&right.rank));

                // One read of this member's chart preferences, rather than one per item.
                let prefs = alive(
                    db.all::<Preference>("preferences")
                        .await
                        .unwrap_or_default(),
                )
                .into_iter()
                .filter(|row| row.user_id == user_id)
                .filter(|row| row.name == "chart")
                .filter_map(|row| {
                    let scope = row.scope_id?;
                    let value = serde_json::from_str(row.value.as_deref()?).ok()?;

                    Some((scope, value))
                })
                .collect();

                SetRows {
                    set: db.get("sets", &set_id).await.unwrap_or_default(),
                    items,
                    songs: db.all("songs").await.unwrap_or_default(),
                    arrangements: alive(db.all("arrangements").await.unwrap_or_default()),
                    prefs,
                }
            }
        },
    );

    Signal::derive(move || resolve(&rows.get().unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str, key: Option<&str>) -> Song {
        Song {
            id: id.to_owned(),
            title: format!("Song {id}"),
            original_key: key.map(str::to_owned),
            ..Song::default()
        }
    }

    fn arrangement(id: &str, song_id: &str, is_default: i64) -> Arrangement {
        Arrangement {
            id: id.to_owned(),
            song_id: song_id.to_owned(),
            name: id.to_owned(),
            is_default,
            ..Arrangement::default()
        }
    }

    fn item(id: &str, song_id: Option<&str>) -> SetItem {
        SetItem {
            id: id.to_owned(),
            set_id: "set".to_owned(),
            rank: "a".to_owned(),
            song_id: song_id.map(str::to_owned),
            ..SetItem::default()
        }
    }

    fn rows(items: Vec<SetItem>) -> SetRows {
        SetRows {
            set: Some(SetRecord {
                id: "set".to_owned(),
                ..SetRecord::default()
            }),
            items,
            songs: vec![song("s1", Some("G"))],
            arrangements: vec![arrangement("a1", "s1", 1)],
            prefs: HashMap::new(),
        }
    }

    #[test]
    fn without_an_override_the_song_decides() {
        let resolved = resolve(&rows(vec![item("i1", Some("s1"))]));

        assert_eq!(
            resolved.items[0].key.map(|key| key.to_string()),
            Some("G".to_owned())
        );
        assert_eq!(resolved.items[0].source, KeySource::Song);
    }

    /// Business rule 4: the band's decision for the night beats the member's own key.
    #[test]
    fn a_set_override_beats_a_personal_preference() {
        let mut rows = rows(vec![SetItem {
            key_override: Some("A".to_owned()),
            ..item("i1", Some("s1"))
        }]);

        rows.prefs.insert(
            "s1".to_owned(),
            SongPrefs {
                preferred_key: Some("D".to_owned()),
                ..SongPrefs::default()
            },
        );

        let resolved = resolve(&rows);

        assert_eq!(
            resolved.items[0].key.map(|key| key.to_string()),
            Some("A".to_owned())
        );
        assert_eq!(resolved.items[0].source, KeySource::Set);
    }

    #[test]
    fn a_personal_preference_still_wins_where_the_set_says_nothing() {
        let mut rows = rows(vec![item("i1", Some("s1"))]);

        rows.prefs.insert(
            "s1".to_owned(),
            SongPrefs {
                preferred_key: Some("D".to_owned()),
                ..SongPrefs::default()
            },
        );

        assert_eq!(
            resolve(&rows).items[0].key.map(|key| key.to_string()),
            Some("D".to_owned()),
        );
    }

    /// The capo follows the same shape: the set's, then the member's, then the arranger's hint.
    #[test]
    fn the_capo_follows_the_same_order() {
        let mut rows = rows(vec![item("i1", Some("s1"))]);

        rows.arrangements[0].capo_hint = Some(3);
        assert_eq!(resolve(&rows).items[0].capo, 3);

        rows.prefs.insert(
            "s1".to_owned(),
            SongPrefs {
                capo: Some(1),
                ..SongPrefs::default()
            },
        );
        assert_eq!(resolve(&rows).items[0].capo, 1);

        rows.items[0].capo_override = Some(0);
        assert_eq!(
            resolve(&rows).items[0].capo,
            0,
            "a set may say 'no capo' and mean it"
        );
    }

    #[test]
    fn a_deleted_song_leaves_the_line_and_says_so() {
        let mut rows = rows(vec![SetItem {
            title_snapshot: Some("Cornerstone".to_owned()),
            ..item("i1", Some("s1"))
        }]);

        rows.songs[0].sync.deleted_at = Some("2026-09-01T00:00:00.000Z".to_owned());

        let resolved = resolve(&rows);

        assert!(resolved.items[0].missing);
        assert!(resolved.items[0].song.is_none());
        assert_eq!(
            resolved.items[0].title, "Cornerstone",
            "the running order still reads"
        );
    }

    #[test]
    fn an_item_that_is_not_a_song_titles_itself_from_its_content() {
        let resolved = resolve(&rows(vec![SetItem {
            item_type: Some("scripture".to_owned()),
            content: Some("Psalm 23\nThe Lord is my shepherd".to_owned()),
            ..item("i1", None)
        }]));

        assert_eq!(resolved.items[0].title, "Psalm 23");
        assert!(!resolved.items[0].missing);
    }

    #[test]
    fn an_empty_item_falls_back_to_its_kind() {
        let resolved = resolve(&rows(vec![SetItem {
            item_type: Some("prayer".to_owned()),
            ..item("i1", None)
        }]));

        assert_eq!(resolved.items[0].title, "prayer");
    }

    /// The set's own arrangement choice comes before the member's, which comes before the
    /// song's default — so a set can pin the acoustic version for everybody.
    #[test]
    fn the_sets_arrangement_choice_comes_first() {
        let mut rows = rows(vec![SetItem {
            arrangement_id: Some("a2".to_owned()),
            ..item("i1", Some("s1"))
        }]);

        rows.arrangements.push(arrangement("a2", "s1", 0));

        assert_eq!(
            resolve(&rows).items[0]
                .arrangement
                .as_ref()
                .map(|row| row.id.as_str()),
            Some("a2"),
        );
    }
}
