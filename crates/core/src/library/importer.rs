//! Import of the files a band already owns: ChordPro exports and plain chords-over-lyrics text.
//!
//! A partial import is never rolled back (business rule 6). Twenty-eight songs in and two error
//! messages is a good afternoon; twenty-eight songs thrown away because two files were odd is
//! not.

use serde::{Deserialize, Serialize};

use crate::chart::chordpro::Chart;
use crate::chart::over_lyrics::{Notation, detect_notation, to_chord_pro};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedSong {
    pub title: String,
    pub artist: Option<String>,
    pub original_key: Option<String>,
    pub tempo: Option<i64>,
    pub time_signature: Option<String>,
    pub body: String,
    pub source_notation: Notation,
    /// The text exactly as it arrived, kept for one undo when it had to be converted.
    pub source_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportResult {
    pub filename: String,
    pub song: Option<ImportedSong>,
    pub error: Option<String>,
}

pub fn import_file(filename: &str, text: &str) -> ImportResult {
    let fail = |error: &str| ImportResult {
        filename: filename.to_owned(),
        song: None,
        error: Some(error.to_owned()),
    };

    if text.trim().is_empty() {
        return fail("The file is empty.");
    }

    // Control characters no chart contains, but a PDF or an image dropped in with them does.
    if text
        .chars()
        .take(2000)
        .any(|character| character.is_control() && !"\t\n\r".contains(character))
    {
        return fail("This is not a text file.");
    }

    let notation = detect_notation(text);
    let body = match notation {
        Notation::ChordPro => text.to_owned(),
        _ => to_chord_pro(text),
    };
    let chart = Chart::parse(&body);

    if let Some(error) = &chart.error {
        return fail(&format!("Line {}: {}", error.line, error.message));
    }

    let title = chart
        .meta
        .title
        .clone()
        .unwrap_or_else(|| title_from_filename(filename));

    if title.trim().is_empty() {
        return fail("No title in the file, and none could be taken from its name.");
    }

    let converted = notation != Notation::ChordPro;

    ImportResult {
        filename: filename.to_owned(),
        error: None,
        song: Some(ImportedSong {
            title: title.trim().to_owned(),
            artist: chart.meta.artist.clone(),
            original_key: chart.meta.key.clone().or_else(|| first_chord_key(&chart)),
            tempo: chart.meta.tempo.as_deref().and_then(leading_number),
            time_signature: chart.meta.time.clone(),
            body,
            source_notation: if converted {
                Notation::OverLyrics
            } else {
                Notation::ChordPro
            },
            source_text: converted.then(|| text.to_owned()),
        }),
    }
}

/// "01 - Amazing Grace.chopro" becomes "Amazing Grace".
pub fn title_from_filename(filename: &str) -> String {
    let stem = match filename.rsplit_once('.') {
        Some((stem, extension))
            if (1..=8).contains(&extension.len())
                && extension.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            stem
        }
        _ => filename,
    };

    let digits = stem.chars().take_while(char::is_ascii_digit).count();
    let separated = stem[digits..].trim_start_matches([' ', '.', '_', '-']);
    let stem = if digits > 0 && separated.len() < stem.len() - digits {
        separated
    } else {
        stem
    };

    stem.replace('_', " ").trim().to_owned()
}

/// With no `{key}` directive, the first chord is the best guess a chart can offer — and it is
/// right far more often than it is wrong, because charts start on the one.
fn first_chord_key(chart: &Chart) -> Option<String> {
    // The first *chord*, not the first bracket: a chart that opens `[N.C.]` or with a token
    // nobody could read still has a key, further down.
    let chord = chart
        .sections
        .iter()
        .flat_map(|section| &section.lines)
        .flat_map(|line| &line.segments)
        .find_map(|segment| segment.chord.as_ref()?.chord.as_ref())?;

    let minor = chord
        .quality
        .strip_prefix("min")
        .or_else(|| chord.quality.strip_prefix('m'))
        .is_some_and(|rest| !rest.starts_with(|c: char| c.is_ascii_lowercase()));

    Some(format!("{}{}", chord.root, if minor { "m" } else { "" }))
}

/// `"72"` and `"72 bpm"` are both a tempo; `"fast"` is not.
fn leading_number(text: &str) -> Option<i64> {
    let text = text.trim_start();
    let digits: String = text
        .strip_prefix('-')
        .map_or(text, |rest| rest)
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();

    if digits.is_empty() {
        return None;
    }

    let value: i64 = digits.parse().ok()?;

    Some(if text.starts_with('-') { -value } else { value })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn imported(filename: &str, text: &str) -> ImportedSong {
        import_file(filename, text)
            .song
            .expect("the file to import")
    }

    #[test]
    fn reads_a_chord_pro_export_as_it_stands() {
        let song = imported(
            "grace.chopro",
            "{title: Amazing Grace}\n{artist: John Newton}\n{key: G}\n{tempo: 72}\n{time: 3/4}\n[G]Amazing",
        );

        assert_eq!(song.title, "Amazing Grace");
        assert_eq!(song.artist.as_deref(), Some("John Newton"));
        assert_eq!(song.original_key.as_deref(), Some("G"));
        assert_eq!(song.tempo, Some(72));
        assert_eq!(song.time_signature.as_deref(), Some("3/4"));
        assert_eq!(song.source_notation, Notation::ChordPro);
        // Nothing was converted, so there is nothing to undo back to.
        assert_eq!(song.source_text, None);
    }

    /// Business rules 1 and 2: a paste is converted once, and the original is kept for one undo.
    #[test]
    fn converts_a_chords_over_lyrics_file_and_keeps_the_original() {
        let text = "Verse 1\nG           C\nAmazing grace how sweet";
        let song = imported("Amazing Grace.txt", text);

        assert_eq!(song.source_notation, Notation::OverLyrics);
        assert_eq!(song.source_text.as_deref(), Some(text));
        assert!(song.body.starts_with("{verse: 1}"));
        assert_eq!(song.title, "Amazing Grace");
    }

    /// The first chord is the best guess a chart with no `{key}` can offer.
    #[test]
    fn guesses_the_key_from_the_first_chord() {
        assert_eq!(
            imported("x.cho", "{title: X}\n[Bb]Something [F]else")
                .original_key
                .as_deref(),
            Some("Bb")
        );
        assert_eq!(
            imported("x.cho", "{title: X}\n[Am7]Something")
                .original_key
                .as_deref(),
            Some("Am")
        );
        // `maj7` is not minor, however much it starts with an m-word.
        assert_eq!(
            imported("x.cho", "{title: X}\n[Cmaj7]Something")
                .original_key
                .as_deref(),
            Some("C")
        );
        assert_eq!(
            imported("x.cho", "{title: X}\nNo chords here").original_key,
            None
        );
    }

    #[test]
    fn takes_a_title_from_the_filename_when_the_file_has_none() {
        assert_eq!(
            title_from_filename("01 - Amazing Grace.chopro"),
            "Amazing Grace"
        );
        assert_eq!(
            title_from_filename("02_Be_Thou_My_Vision.txt"),
            "Be Thou My Vision"
        );
        assert_eq!(title_from_filename("Amazing Grace"), "Amazing Grace");
        // A number that is the whole name is the name, not a prefix to strip.
        assert_eq!(title_from_filename("40.pro"), "40");
        assert_eq!(
            imported("01 - Amazing Grace.chopro", "[G]Amazing").title,
            "Amazing Grace"
        );
    }

    /// Business rule 6: a file that cannot be read says why, and the rest of the import goes on.
    #[test]
    fn says_what_is_wrong_rather_than_throwing_the_file_away() {
        assert_eq!(
            import_file("empty.txt", "   \n\n").error.as_deref(),
            Some("The file is empty.")
        );
        assert_eq!(
            import_file("score.pdf", "%PDF-1.7\n\u{0}\u{1}\u{2}binary")
                .error
                .as_deref(),
            Some("This is not a text file.")
        );
        assert_eq!(
            import_file("broken.cho", "{title: X}\n[G]Fine\n[C Broken")
                .error
                .as_deref(),
            Some("Line 3: Unclosed [ in chord position.")
        );
        assert_eq!(
            import_file(".cho", "[G]Words").error.as_deref(),
            Some("No title in the file, and none could be taken from its name.")
        );
    }

    #[test]
    fn reads_a_tempo_a_human_typed() {
        let tempo =
            |value: &str| imported("x.cho", &format!("{{title: X}}\n{{tempo: {value}}}")).tempo;

        assert_eq!(tempo("72"), Some(72));
        assert_eq!(tempo("72 bpm"), Some(72));
        assert_eq!(tempo("fast"), None);
    }
}
