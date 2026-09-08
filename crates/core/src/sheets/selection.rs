//! Choosing which sheet to show (business rule 4).
//!
//! A musician opening a song at their key wants their part in that key, and if it does not exist
//! they want to be told what they are looking at instead — never a blank screen. So selection
//! always returns something when anything exists, and says how far it had to fall back.

use serde::{Deserialize, Serialize};

use crate::chart::notes::Key;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Lead,
    Piano,
    Vocal,
    Guitar,
    Bass,
    Lyrics,
    Other,
}

pub const PARTS: [Part; 7] = [
    Part::Lead,
    Part::Piano,
    Part::Vocal,
    Part::Guitar,
    Part::Bass,
    Part::Lyrics,
    Part::Other,
];

impl Part {
    pub fn as_str(self) -> &'static str {
        match self {
            Part::Lead => "lead",
            Part::Piano => "piano",
            Part::Vocal => "vocal",
            Part::Guitar => "guitar",
            Part::Bass => "bass",
            Part::Lyrics => "lyrics",
            Part::Other => "other",
        }
    }

    pub fn parse(text: &str) -> Option<Part> {
        PARTS.into_iter().find(|part| part.as_str() == text)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fallback {
    /// The key and the part asked for.
    Exact,
    /// Right key, a different part.
    OtherPart,
    /// A sheet tagged as fitting any key.
    AnyKey,
    /// The preferred part, in the closest key.
    NearestKey,
    /// Nothing matched; the first sheet by position.
    First,
}

/// The half of a sheet row that selection reads. Both shells map their own row onto this rather
/// than this crate knowing what a database row looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SheetChoice<'a> {
    pub id: &'a str,
    pub sheet_key: Option<&'a str>,
    pub part: Option<&'a str>,
    pub position: i64,
    pub deleted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection<'a> {
    pub sheet: SheetChoice<'a>,
    pub fallback: Fallback,
}

/// Distance around the circle of fifths, 0–6. Bb is one step from F and six from B, which is
/// what "nearest key" means to a musician — not the semitone distance.
pub fn fifths_distance(a: &Key, b: &Key) -> i16 {
    let steps = ((b.tonic.pitch_class() - a.tonic.pitch_class()) * 7).rem_euclid(12);

    steps.min(12 - steps)
}

/// Business rule 5: capo never enters into this. A capo changes the shapes played, not the key
/// the band sounds in, so the sheet stays in the sounding key.
pub fn select_sheet<'a>(
    sheets: &[SheetChoice<'a>],
    key: Option<&Key>,
    part: Option<Part>,
) -> Option<Selection<'a>> {
    let mut usable: Vec<SheetChoice<'a>> = sheets.iter().copied().filter(|s| !s.deleted).collect();

    usable.sort_by_key(|sheet| sheet.position);

    if usable.is_empty() {
        return None;
    }

    let in_key = |sheet: &SheetChoice| match (key, sheet.sheet_key.and_then(Key::parse)) {
        (Some(wanted), Some(sheet_key)) => {
            sheet_key.tonic.pitch_class() == wanted.tonic.pitch_class()
                && sheet_key.minor == wanted.minor
        }
        _ => false,
    };
    let is_part = |sheet: &SheetChoice| part.is_none_or(|part| sheet.part == Some(part.as_str()));

    let found = |fallback: Fallback, sheet: Option<&SheetChoice<'a>>| {
        sheet.map(|sheet| Selection {
            sheet: *sheet,
            fallback,
        })
    };

    if let Some(selection) = found(
        Fallback::Exact,
        usable.iter().find(|sheet| in_key(sheet) && is_part(sheet)),
    ) {
        return Some(selection);
    }

    if let Some(selection) = found(Fallback::OtherPart, usable.iter().find(|s| in_key(s))) {
        return Some(selection);
    }

    // A sheet with no key is a lyrics sheet or a chart that suits any key; it beats showing a
    // sheet in the wrong key.
    if let Some(selection) = found(
        Fallback::AnyKey,
        usable
            .iter()
            .find(|sheet| sheet.sheet_key.is_none() && is_part(sheet)),
    ) {
        return Some(selection);
    }

    if let Some(wanted) = key {
        let nearest = usable
            .iter()
            .filter(|sheet| is_part(sheet))
            .filter_map(|sheet| Some((sheet, Key::parse(sheet.sheet_key?)?)))
            .min_by_key(|(_, sheet_key)| fifths_distance(wanted, sheet_key))
            .map(|(sheet, _)| sheet);

        if let Some(selection) = found(Fallback::NearestKey, nearest) {
            return Some(selection);
        }
    }

    found(Fallback::First, usable.first())
}

impl Selection<'_> {
    /// What the banner over the viewer says when the sheet is not the one that was asked for.
    pub fn explain(&self, key: Option<&Key>, part: Option<Part>) -> Option<String> {
        let sheet_key = self.sheet.sheet_key.unwrap_or("any key");
        let sheet_part = self.sheet.part.unwrap_or("available");

        Some(match self.fallback {
            Fallback::Exact => return None,
            Fallback::OtherPart => format!(
                "No {} sheet in this key — showing the {sheet_part} sheet.",
                part.map_or("preferred", Part::as_str)
            ),
            Fallback::AnyKey => "This sheet is not tied to a key.".to_owned(),
            Fallback::NearestKey => format!(
                "No sheet in {} — showing {sheet_key}, the nearest key available.",
                key.map_or("that key".to_owned(), Key::to_string)
            ),
            Fallback::First => format!(
                "Showing the only sheet attached ({}, {sheet_key}).",
                self.sheet.part.unwrap_or("other")
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Key {
        Key::parse(text).expect("a key")
    }

    fn sheet<'a>(id: &'a str, sheet_key: Option<&'a str>, part: &'a str) -> SheetChoice<'a> {
        SheetChoice {
            id,
            sheet_key,
            part: Some(part),
            position: 0,
            deleted: false,
        }
    }

    fn chosen<'a>(selection: &Option<Selection<'a>>) -> Option<(&'a str, Fallback)> {
        selection.map(|selection| (selection.sheet.id, selection.fallback))
    }

    /// Business rule 4, in order.
    #[test]
    fn prefers_the_exact_key_and_the_readers_part() {
        let sheets = [
            sheet("a", Some("G"), "piano"),
            sheet("b", Some("G"), "lead"),
            sheet("c", Some("Bb"), "piano"),
        ];

        assert_eq!(
            chosen(&select_sheet(&sheets, Some(&key("G")), Some(Part::Piano))),
            Some(("a", Fallback::Exact))
        );
    }

    #[test]
    fn falls_back_to_another_part_in_the_same_key_before_another_key() {
        let sheets = [
            sheet("b", Some("G"), "lead"),
            sheet("c", Some("Bb"), "piano"),
        ];

        assert_eq!(
            chosen(&select_sheet(&sheets, Some(&key("G")), Some(Part::Piano))),
            Some(("b", Fallback::OtherPart))
        );
    }

    #[test]
    fn uses_a_keyless_sheet_before_a_sheet_in_the_wrong_key() {
        let sheets = [
            sheet("lyrics", None, "piano"),
            sheet("c", Some("Bb"), "piano"),
        ];

        assert_eq!(
            chosen(&select_sheet(&sheets, Some(&key("G")), Some(Part::Piano))),
            Some(("lyrics", Fallback::AnyKey))
        );
    }

    #[test]
    fn falls_back_to_the_nearest_key_around_the_circle_of_fifths() {
        let sheets = [
            sheet("far", Some("B"), "piano"),
            sheet("near", Some("D"), "piano"),
        ];

        assert_eq!(
            chosen(&select_sheet(&sheets, Some(&key("G")), Some(Part::Piano))),
            Some(("near", Fallback::NearestKey))
        );
    }

    #[test]
    fn shows_something_rather_than_nothing() {
        let only = SheetChoice {
            position: 3,
            ..sheet("only", Some("Eb"), "bass")
        };

        assert_eq!(
            chosen(&select_sheet(&[only], Some(&key("G")), Some(Part::Piano))),
            Some(("only", Fallback::First))
        );
        assert_eq!(
            chosen(&select_sheet(&[], Some(&key("G")), Some(Part::Piano))),
            None
        );
    }

    #[test]
    fn ignores_deleted_sheets() {
        let gone = SheetChoice {
            deleted: true,
            ..sheet("gone", Some("G"), "piano")
        };
        let sheets = [gone, sheet("b", Some("Bb"), "piano")];

        assert_eq!(
            chosen(&select_sheet(&sheets, Some(&key("G")), Some(Part::Piano))).map(|found| found.0),
            Some("b")
        );
    }

    /// Position is the order the musician arranged; the first by position is the default.
    #[test]
    fn reads_the_order_the_musician_arranged() {
        let sheets = [
            SheetChoice {
                position: 2,
                ..sheet("second", Some("G"), "piano")
            },
            SheetChoice {
                position: 1,
                ..sheet("first", Some("G"), "piano")
            },
        ];

        assert_eq!(
            chosen(&select_sheet(&sheets, Some(&key("G")), Some(Part::Piano))),
            Some(("first", Fallback::Exact))
        );
    }

    #[test]
    fn measures_distance_in_fifths_not_semitones() {
        assert_eq!(fifths_distance(&key("C"), &key("G")), 1);
        assert_eq!(fifths_distance(&key("C"), &key("F")), 1);
        assert_eq!(fifths_distance(&key("C"), &key("D")), 2);
        assert_eq!(fifths_distance(&key("C"), &key("C#")), 5);
        assert_eq!(fifths_distance(&key("C"), &key("F#")), 6);
        assert_eq!(fifths_distance(&key("G"), &key("G")), 0);
    }

    /// The banner exists so a musician is never looking at the wrong sheet without being told.
    #[test]
    fn says_nothing_on_an_exact_match_and_something_on_every_other() {
        let exact = select_sheet(
            &[sheet("a", Some("G"), "piano")],
            Some(&key("G")),
            Some(Part::Piano),
        )
        .unwrap();

        assert_eq!(exact.explain(Some(&key("G")), Some(Part::Piano)), None);

        let nearest = select_sheet(
            &[sheet("a", Some("D"), "piano")],
            Some(&key("G")),
            Some(Part::Piano),
        )
        .unwrap();

        assert_eq!(
            nearest
                .explain(Some(&key("G")), Some(Part::Piano))
                .as_deref(),
            Some("No sheet in G — showing D, the nearest key available.")
        );
    }
}
