//! What a song has to look like before it is saved, and the two shapes a person types by hand.
//!
//! This is client-side validation, for immediate feedback — the database re-checks what it can.
//! It lives here so the server's answer and the form's answer are the same sentence.

use serde::{Deserialize, Serialize};

pub const MAX_TITLE: usize = 200;

/// The range a tempo can plausibly be. Outside it, somebody typed the year.
pub const TEMPO_RANGE: std::ops::RangeInclusive<i64> = 20..=300;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongInput {
    pub title: String,
    pub ccli_number: Option<String>,
    pub tempo: Option<i64>,
}

pub fn validate_song(input: &SongInput) -> Vec<String> {
    let mut problems = Vec::new();
    let title = input.title.trim();

    if title.is_empty() {
        problems.push("A title is required.".to_owned());
    }

    if title.chars().count() > MAX_TITLE {
        problems.push(format!("A title is at most {MAX_TITLE} characters."));
    }

    if input.tempo.is_some_and(|tempo| !TEMPO_RANGE.contains(&tempo)) {
        problems.push(format!(
            "Tempo is between {} and {} bpm.",
            TEMPO_RANGE.start(),
            TEMPO_RANGE.end()
        ));
    }

    if input
        .ccli_number
        .as_deref()
        .is_some_and(|number| !number.is_empty() && !number.bytes().all(|b| b.is_ascii_digit()))
    {
        problems.push("A CCLI number is digits only.".to_owned());
    }

    problems
}

/// `mm:ss` in, seconds out. Anything else is `None` rather than a guess — a duration the app
/// guessed wrong is worse on a run sheet than a duration it left blank.
pub fn parse_duration(text: &str) -> Option<i64> {
    let (minutes, seconds) = text.trim().split_once(':')?;

    if !(1..=3).contains(&minutes.len()) || seconds.len() != 2 {
        return None;
    }

    let minutes: i64 = minutes.parse().ok()?;
    let seconds: i64 = seconds.parse().ok()?;

    (seconds < 60).then_some(minutes * 60 + seconds)
}

pub fn format_duration(seconds: Option<i64>) -> String {
    match seconds {
        Some(seconds) => format!("{}:{:02}", seconds / 60, seconds % 60),
        None => String::new(),
    }
}

/// True when re-parenting `id` under `parent_id` would close a loop — including the degenerate
/// case of dropping a folder onto itself.
pub fn would_cycle(folders: &[(&str, Option<&str>)], id: &str, parent_id: Option<&str>) -> bool {
    let Some(parent_id) = parent_id else {
        return false;
    };

    let mut walker = Some(parent_id);
    let mut seen: Vec<&str> = Vec::new();

    // The walk is bounded by `seen` as well as by reaching the root, because the stored tree may
    // already contain a loop that two offline moves created between them.
    while let Some(current) = walker.filter(|current| !seen.contains(current)) {
        if current == id {
            return true;
        }

        seen.push(current);
        walker = folders
            .iter()
            .find(|(folder, _)| *folder == current)
            .and_then(|(_, parent)| *parent);
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(title: &str) -> SongInput {
        SongInput {
            title: title.to_owned(),
            ..SongInput::default()
        }
    }

    #[test]
    fn a_song_needs_a_title_and_not_much_else() {
        assert!(validate_song(&song("Amazing Grace")).is_empty());
        assert_eq!(validate_song(&song("   ")), ["A title is required."]);
        assert_eq!(
            validate_song(&song(&"a".repeat(MAX_TITLE + 1))),
            [format!("A title is at most {MAX_TITLE} characters.")]
        );
        // Counted in characters, so an accented title is not shorter than it looks.
        assert!(validate_song(&song(&"é".repeat(MAX_TITLE))).is_empty());
    }

    #[test]
    fn refuses_a_tempo_that_is_a_year_and_a_ccli_number_that_is_a_sentence() {
        let with = |tempo: Option<i64>, ccli: Option<&str>| {
            validate_song(&SongInput {
                tempo,
                ccli_number: ccli.map(str::to_owned),
                ..song("X")
            })
        };

        assert!(with(Some(72), Some("22025")).is_empty());
        assert!(with(None, Some("")).is_empty(), "an empty field is not a wrong one");
        assert_eq!(with(Some(2026), None), ["Tempo is between 20 and 300 bpm."]);
        assert_eq!(with(Some(19), None).len(), 1);
        assert_eq!(with(None, Some("CCLI 22025")), ["A CCLI number is digits only."]);
    }

    #[test]
    fn reads_and_writes_a_duration_the_way_a_run_sheet_does() {
        assert_eq!(parse_duration("4:05"), Some(245));
        assert_eq!(parse_duration(" 12:00 "), Some(720));
        assert_eq!(format_duration(Some(245)), "4:05");
        assert_eq!(format_duration(Some(60)), "1:00");
        assert_eq!(format_duration(None), "");
        assert_eq!(parse_duration(&format_duration(Some(245))), Some(245));
    }

    #[test]
    fn guesses_nothing_from_something_that_is_not_a_duration() {
        assert_eq!(parse_duration("4:5"), None);
        assert_eq!(parse_duration("4:60"), None);
        assert_eq!(parse_duration("245"), None);
        assert_eq!(parse_duration("four minutes"), None);
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn refuses_a_move_that_would_close_a_loop() {
        let folders = [
            ("root", None),
            ("hymns", Some("root")),
            ("advent", Some("hymns")),
        ];

        assert!(would_cycle(&folders, "root", Some("advent")), "onto its own descendant");
        assert!(would_cycle(&folders, "hymns", Some("hymns")), "onto itself");
        assert!(!would_cycle(&folders, "advent", Some("root")), "further up is fine");
        assert!(!would_cycle(&folders, "hymns", None), "to the top is always fine");
    }

    /// Two offline moves can leave a loop already in the tree; the check must not spin on it.
    #[test]
    fn terminates_on_a_tree_that_is_already_looped() {
        let folders = [("a", Some("b")), ("b", Some("a"))];

        assert!(!would_cycle(&folders, "c", Some("a")));
    }
}
