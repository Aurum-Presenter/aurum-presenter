//! Which sheet files this device keeps.
//!
//! The rule the features agree on: what is coming up, and what the user asked to keep. A set
//! inside its window brings the sheets for the keys that set will actually be played in — not
//! every sheet of every song in it, which on a phone is the difference between a few megabytes
//! and a few hundred.

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::chart::notes::Key;
use crate::sheets::selection::{Fallback, Part, SheetChoice, select_sheet};
use crate::time;

/// A set within this many days is kept without anybody asking.
pub const AUTO_PIN_DAYS: i64 = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SheetWant<'a> {
    pub song_id: &'a str,
    pub key: Option<Key>,
}

/// A sheet as the pin policy sees it: which song it belongs to, and whether it has a file at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SheetFile<'a> {
    pub choice: SheetChoice<'a>,
    pub song_id: &'a str,
    /// A sheet row with no content hash has nothing to download yet.
    pub uploaded: bool,
}

pub fn wanted_sheets<'a>(
    sheets: &[SheetFile<'a>],
    wants: &[SheetWant<'a>],
    part: Option<Part>,
) -> BTreeSet<&'a str> {
    let mut wanted = BTreeSet::new();

    for want in wants {
        let for_song: Vec<&SheetFile<'a>> = sheets
            .iter()
            .filter(|sheet| sheet.song_id == want.song_id && !sheet.choice.deleted)
            .collect();
        let choices: Vec<SheetChoice<'a>> = for_song.iter().map(|sheet| sheet.choice).collect();
        let chosen = select_sheet(&choices, want.key.as_ref(), part);

        if let Some(selection) = &chosen
            && for_song
                .iter()
                .any(|sheet| sheet.choice.id == selection.sheet.id && sheet.uploaded)
        {
            wanted.insert(selection.sheet.id);
        }

        // A song pinned deliberately keeps every part in that key, because the person who
        // pinned it does not necessarily know which part they will need on the night.
        let Some(selection) = chosen.filter(|found| found.fallback == Fallback::Exact) else {
            continue;
        };

        for sheet in &for_song {
            if sheet.uploaded
                && sheet.choice.sheet_key.is_some()
                && sheet.choice.sheet_key == selection.sheet.sheet_key
            {
                wanted.insert(sheet.choice.id);
            }
        }
    }

    wanted
}

/// A set is kept when somebody pinned it, or when its date is close enough that they will need
/// it before they next have signal.
pub fn is_auto_pinned(pinned: bool, scheduled_for: Option<&str>, now_ms: i64) -> bool {
    if pinned {
        return true;
    }

    let Some(when) = scheduled_for.and_then(time::parse) else {
        return false;
    };
    let days = time::days_between(now_ms, when);

    // Yesterday's set is still held: a service that ran this morning is often still being
    // talked about, and re-downloading it costs more than keeping it one more day.
    (-1..=AUTO_PIN_DAYS).contains(&days)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinKind {
    Set,
    Song,
}

/// Why something is here: asked for by hand, or kept because the date is close.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PinReason {
    Pinned,
    ComingUp,
}

/// What this device is keeping on purpose, and what each one costs.
///
/// Read when a pinned download will not fit: the app never decides which of a musician's pins
/// matters least, so it shows them, largest first, and lets them choose (business rule 10).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldPin {
    pub kind: PinKind,
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub reason: PinReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinnedSet<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub pinned: bool,
    pub scheduled_for: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetSong<'a> {
    pub set_id: &'a str,
    pub song_id: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachedSheet<'a> {
    pub song_id: &'a str,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinnedSong<'a> {
    pub song_id: &'a str,
    pub title: &'a str,
}

#[derive(Clone, Debug, Default)]
pub struct PinSources<'a> {
    pub sets: Vec<PinnedSet<'a>>,
    pub items: Vec<SetSong<'a>>,
    pub cached: Vec<CachedSheet<'a>>,
    /// Songs the user pinned by hand, in the order the preferences were read.
    pub pinned_songs: Vec<PinnedSong<'a>>,
}

/// What is held, what it costs, and what can be let go — largest first, because that is the
/// order somebody looking to free space reads in.
pub fn held_pins(sources: &PinSources<'_>, now_ms: i64) -> Vec<HeldPin> {
    let mut bytes_by_song: HashMap<&str, u64> = HashMap::new();

    for record in &sources.cached {
        *bytes_by_song.entry(record.song_id).or_default() += record.bytes;
    }

    let mut held: Vec<HeldPin> = Vec::new();

    for set in &sources.sets {
        if !is_auto_pinned(set.pinned, set.scheduled_for, now_ms) {
            continue;
        }

        let songs: BTreeSet<&str> = sources
            .items
            .iter()
            .filter(|item| item.set_id == set.id)
            .filter_map(|item| item.song_id)
            .collect();

        held.push(HeldPin {
            kind: PinKind::Set,
            id: set.id.to_owned(),
            name: set.name.to_owned(),
            bytes: songs
                .iter()
                .map(|song| bytes_by_song.get(song).copied().unwrap_or_default())
                .sum(),
            reason: if set.pinned {
                PinReason::Pinned
            } else {
                PinReason::ComingUp
            },
        });
    }

    for song in &sources.pinned_songs {
        held.push(HeldPin {
            kind: PinKind::Song,
            id: song.song_id.to_owned(),
            name: song.title.to_owned(),
            bytes: bytes_by_song.get(song.song_id).copied().unwrap_or_default(),
            reason: PinReason::Pinned,
        });
    }

    held.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    held
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-09-06T12:00:00.000Z";

    fn now() -> i64 {
        time::parse(NOW).unwrap()
    }

    fn sheet<'a>(
        id: &'a str,
        song_id: &'a str,
        key: Option<&'a str>,
        part: &'a str,
    ) -> SheetFile<'a> {
        SheetFile {
            choice: SheetChoice {
                id,
                sheet_key: key,
                part: Some(part),
                position: 0,
                deleted: false,
            },
            song_id,
            uploaded: true,
        }
    }

    fn key(text: &str) -> Option<Key> {
        Key::parse(text)
    }

    #[test]
    fn keeps_the_sheet_the_set_will_actually_be_played_from() {
        let sheets = [
            sheet("grace-g-piano", "grace", Some("G"), "piano"),
            sheet("grace-bb-piano", "grace", Some("Bb"), "piano"),
            sheet("other", "noel", Some("G"), "piano"),
        ];
        let wants = [SheetWant {
            song_id: "grace",
            key: key("G"),
        }];

        assert_eq!(
            wanted_sheets(&sheets, &wants, Some(Part::Piano)),
            ["grace-g-piano"].into_iter().collect()
        );
    }

    /// Business rule: a deliberate pin keeps every part in that key, not only the reader's.
    #[test]
    fn keeps_every_part_in_the_key_it_matched_exactly() {
        let sheets = [
            sheet("g-piano", "grace", Some("G"), "piano"),
            sheet("g-lead", "grace", Some("G"), "lead"),
            sheet("bb-lead", "grace", Some("Bb"), "lead"),
        ];
        let wants = [SheetWant {
            song_id: "grace",
            key: key("G"),
        }];

        assert_eq!(
            wanted_sheets(&sheets, &wants, Some(Part::Piano)),
            ["g-lead", "g-piano"].into_iter().collect()
        );
    }

    /// A fallback is not an agreement: it brings the one sheet being shown, not the whole key.
    #[test]
    fn a_fallback_brings_only_the_sheet_it_fell_back_to() {
        let sheets = [
            sheet("d-piano", "grace", Some("D"), "piano"),
            sheet("d-lead", "grace", Some("D"), "lead"),
        ];
        let wants = [SheetWant {
            song_id: "grace",
            key: key("G"),
        }];

        assert_eq!(
            wanted_sheets(&sheets, &wants, Some(Part::Piano)),
            ["d-piano"].into_iter().collect()
        );
    }

    #[test]
    fn wants_nothing_from_a_sheet_row_with_no_file_behind_it() {
        let sheets = [SheetFile {
            uploaded: false,
            ..sheet("not-yet", "grace", Some("G"), "piano")
        }];
        let wants = [SheetWant {
            song_id: "grace",
            key: key("G"),
        }];

        assert!(wanted_sheets(&sheets, &wants, Some(Part::Piano)).is_empty());
    }

    #[test]
    fn holds_a_set_that_is_pinned_or_close_and_lets_the_rest_go() {
        assert!(
            is_auto_pinned(true, Some("2020-01-01"), now()),
            "pinned by hand"
        );
        assert!(
            is_auto_pinned(false, Some("2026-09-08"), now()),
            "this Sunday"
        );
        assert!(
            is_auto_pinned(false, Some("2026-09-05"), now()),
            "yesterday"
        );
        assert!(
            is_auto_pinned(false, Some("2026-09-20"), now()),
            "the last day of the window"
        );
        assert!(
            !is_auto_pinned(false, Some("2026-09-21"), now()),
            "past the window"
        );
        assert!(
            !is_auto_pinned(false, Some("2026-09-04"), now()),
            "two days ago"
        );
        assert!(!is_auto_pinned(false, None, now()), "no date at all");
        assert!(!is_auto_pinned(false, Some("not a date"), now()));
    }

    fn sources() -> PinSources<'static> {
        PinSources {
            sets: vec![
                PinnedSet {
                    id: "coming-up",
                    name: "Sunday morning",
                    pinned: false,
                    scheduled_for: Some("2026-09-08"),
                },
                PinnedSet {
                    id: "kept",
                    name: "Carols",
                    pinned: true,
                    scheduled_for: Some("2025-12-24"),
                },
                PinnedSet {
                    id: "old",
                    name: "Last Easter",
                    pinned: false,
                    scheduled_for: Some("2026-04-05"),
                },
            ],
            items: vec![
                SetSong {
                    set_id: "coming-up",
                    song_id: Some("grace"),
                },
                SetSong {
                    set_id: "kept",
                    song_id: Some("noel"),
                },
                SetSong {
                    set_id: "old",
                    song_id: Some("grace"),
                },
            ],
            cached: vec![
                CachedSheet {
                    song_id: "grace",
                    bytes: 4 * 1024 * 1024,
                },
                CachedSheet {
                    song_id: "noel",
                    bytes: 12 * 1024 * 1024,
                },
                CachedSheet {
                    song_id: "thine",
                    bytes: 1024 * 1024,
                },
            ],
            pinned_songs: vec![PinnedSong {
                song_id: "thine",
                title: "Be Thou My Vision",
            }],
        }
    }

    /// Offline-storage acceptance criterion 6: a device that fills up names what is holding the
    /// room rather than choosing for the user.
    #[test]
    fn lists_what_is_kept_on_purpose_biggest_first() {
        let held = held_pins(&sources(), now());

        assert_eq!(
            held.iter()
                .map(|pin| format!("{} {}", pin.name, pin.bytes / 1024 / 1024))
                .collect::<Vec<_>>(),
            ["Carols 12", "Sunday morning 4", "Be Thou My Vision 1"]
        );
    }

    #[test]
    fn says_why_each_of_them_is_here() {
        let held = held_pins(&sources(), now());
        let reason = |name: &str| {
            held.iter()
                .find(|pin| pin.name == name)
                .map(|pin| pin.reason)
        };

        assert_eq!(reason("Carols"), Some(PinReason::Pinned));
        assert_eq!(reason("Sunday morning"), Some(PinReason::ComingUp));
        assert_eq!(reason("Be Thou My Vision"), Some(PinReason::Pinned));
    }

    #[test]
    fn leaves_out_a_set_that_is_neither_pinned_nor_coming_up() {
        assert!(
            !held_pins(&sources(), now())
                .iter()
                .any(|pin| pin.name == "Last Easter")
        );
    }
}
