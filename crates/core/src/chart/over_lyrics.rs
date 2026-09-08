//! Converting chords-over-lyrics text to ChordPro on entry (business rules 1 and 2).
//!
//! Everything a band already owns is in this format — SongbookPro, OnSong, Ultimate Guitar, a
//! printout typed into a text file. Storing it as-is would mean two chart formats forever, so it
//! is converted once, on paste, and the original is kept in `source_text` for one undo.

use serde::{Deserialize, Serialize};

use super::chord::Chord;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Notation {
    ChordPro,
    OverLyrics,
    Ambiguous,
}

const HEADINGS: [&str; 11] = [
    "interlude",
    "instrumental",
    "prechorus",
    "pre-chorus",
    "chorus",
    "bridge",
    "ending",
    "intro",
    "outro",
    "verse",
    "tag",
];

/// A line is a chord line when it has at least one token and every token is a chord.
pub fn is_chord_line(line: &str) -> bool {
    let mut tokens = line.split_whitespace().peekable();

    tokens.peek().is_some() && tokens.all(|token| Chord::parse(token).is_some())
}

/// `Verse 2`, `[Chorus]`, `Pre-Chorus:` — the heading and its label, if the line is one.
fn section_heading(line: &str) -> Option<(String, Option<String>)> {
    let mut rest = line.trim();
    let bracketed = rest.starts_with('[');

    if bracketed {
        rest = rest.strip_prefix('[')?.trim_start();
    }

    rest = rest.strip_suffix(':').unwrap_or(rest).trim_end();

    if bracketed {
        rest = rest.strip_suffix(']')?.trim_end();
        rest = rest.strip_suffix(':').unwrap_or(rest).trim_end();
    }

    let lowered = rest.to_lowercase();
    let name = HEADINGS
        .into_iter()
        .find(|heading| lowered.starts_with(heading))?;
    let label = lowered[name.len()..].trim();

    // A label is one or two digits, or a single letter — `Verse 12`, `Chorus b`. Anything else
    // means the line is a lyric that merely opens with the word "bridge".
    let numbered = (1..=2).contains(&label.len()) && label.chars().all(|c| c.is_ascii_digit());
    let lettered = label.len() == 1 && label.chars().all(|c| c.is_ascii_alphabetic());

    if !label.is_empty() && !numbered && !lettered {
        return None;
    }

    Some((
        name.replace('-', ""),
        (!label.is_empty()).then(|| label.to_owned()),
    ))
}

pub fn detect_notation(text: &str) -> Notation {
    if has_directive(text) || has_bracketed_chord(text) {
        return Notation::ChordPro;
    }

    if lines_of(text).iter().any(|line| is_chord_line(line)) {
        Notation::OverLyrics
    } else {
        Notation::Ambiguous
    }
}

/// `{title:` or `{soc}` — a brace, a directive name, then a colon or the closing brace.
fn has_directive(text: &str) -> bool {
    text.match_indices('{').any(|(start, _)| {
        let rest = &text[start + 1..];
        let name = rest
            .chars()
            .take_while(|c| c.is_ascii_alphabetic() || *c == '_')
            .count();

        name > 0
            && rest[name..]
                .trim_start_matches([' ', '\t'])
                .starts_with([':', '}'])
    })
}

/// `[G]`, `[Bbmaj7]` — a bracket opening on a note letter and closing before the line ends.
fn has_bracketed_chord(text: &str) -> bool {
    text.match_indices('[').any(|(start, _)| {
        let rest = &text[start + 1..];

        rest.starts_with(|c: char| c.is_ascii_alphabetic() && c.to_ascii_uppercase() <= 'G')
            && rest
                .chars()
                .skip(1)
                .take(12)
                .take_while(|c| *c != '\n')
                .any(|c| c == ']')
    })
}

fn lines_of(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

/// Converts chords-over-lyrics to ChordPro, preserving each chord's character column.
///
/// The column is the whole contract: acceptance criterion 1 says a converted chart re-rendered
/// over its lyrics must put every chord back in the column it was pasted at. So a chord binds to
/// the character offset it sits above in the next non-blank line, and a chord past the end of
/// that line keeps its column as trailing spaces.
pub fn to_chord_pro(text: &str) -> String {
    let lines = lines_of(text);
    let mut output: Vec<String> = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        index += 1;

        if let Some((name, label)) = section_heading(line) {
            output.push(match label {
                Some(label) => format!("{{{name}: {label}}}"),
                None => format!("{{{name}}}"),
            });
            continue;
        }

        if !is_chord_line(line) {
            output.push(line.trim_end().to_owned());
            continue;
        }

        let chords = positions_of(line);
        let next = lines.get(index);

        match next {
            Some(next)
                if !next.trim().is_empty()
                    && !is_chord_line(next)
                    && section_heading(next).is_none() =>
            {
                output.push(merge(next.trim_end(), &chords));
                index += 1;
            }
            _ => output.push(merge("", &chords)),
        }
    }

    format!("{}\n", collapse_blank_runs(&output.join("\n")).trim())
}

#[derive(Clone, Debug)]
struct Placement {
    /// In characters, not bytes — the column is what the musician sees.
    column: usize,
    chord: String,
}

fn positions_of(line: &str) -> Vec<Placement> {
    let mut placements = Vec::new();
    let mut current: Option<Placement> = None;

    for (column, character) in line.chars().enumerate() {
        if character.is_whitespace() {
            placements.extend(current.take());
        } else {
            current
                .get_or_insert_with(|| Placement {
                    column,
                    chord: String::new(),
                })
                .chord
                .push(character);
        }
    }

    placements.extend(current);
    placements
}

/// Inserts `[chord]` into a lyric line at each chord's column, right to left so offsets hold.
fn merge(lyric: &str, chords: &[Placement]) -> String {
    let mut result: Vec<char> = lyric.chars().collect();

    for placement in chords.iter().rev() {
        while result.len() < placement.column {
            result.push(' ');
        }

        let bracketed = format!("[{}]", placement.chord);

        result.splice(placement.column..placement.column, bracketed.chars());
    }

    result.into_iter().collect()
}

/// Three or more blank lines in a paste are formatting, not structure.
fn collapse_blank_runs(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut newlines = 0;

    for character in text.chars() {
        if character == '\n' {
            newlines += 1;

            if newlines > 2 {
                continue;
            }
        } else {
            newlines = 0;
        }

        result.push(character);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::chordpro::{Chart, SectionKind};
    use crate::chart::notes::Key;
    use crate::chart::render::{Layout, RenderOptions};

    const PASTED: &str = concat!(
        "Verse 1\n",
        "G           C\n",
        "Amazing grace how sweet\n",
        "       D        G\n",
        "the sound of it all",
    );

    #[test]
    fn detects_the_notation_it_was_given() {
        assert_eq!(detect_notation(PASTED), Notation::OverLyrics);
        assert_eq!(
            detect_notation("{title: X}\n[G]Amazing"),
            Notation::ChordPro
        );
        assert_eq!(detect_notation("{soc}\nWords"), Notation::ChordPro);
        assert_eq!(detect_notation("just some words"), Notation::Ambiguous);
        assert!(is_chord_line("G           C"));
        assert!(!is_chord_line("Amazing grace"));
        assert!(!is_chord_line("   "));
    }

    /// Acceptance criterion 1 — every chord returns to the column it was pasted at.
    #[test]
    fn round_trips_chord_columns_through_chord_pro() {
        let chart = Chart::parse(&to_chord_pro(PASTED));
        let key = Key::parse("G").unwrap();
        let rendered = chart.render(&RenderOptions {
            source: key,
            target: key,
            capo: 0,
            layout: Layout::Over,
        });

        let rows: Vec<_> = rendered
            .sections
            .iter()
            .flat_map(|section| &section.lines)
            .map(|line| line.over_lyrics_rows())
            .filter(|row| !row.chords.is_empty())
            .collect();

        assert_eq!(rows[0].chords, "G           C");
        assert_eq!(rows[0].lyrics, "Amazing grace how sweet");
        assert_eq!(rows[1].chords, "       D        G");
        assert_eq!(rows[1].lyrics, "the sound of it all");
    }

    #[test]
    fn turns_a_plain_section_heading_into_a_directive() {
        assert!(to_chord_pro(PASTED).starts_with("{verse: 1}"));
        assert_eq!(
            Chart::parse(&to_chord_pro(PASTED)).sections[0].kind,
            SectionKind::Verse
        );
    }

    #[test]
    fn reads_the_shapes_a_heading_is_written_in() {
        assert_eq!(
            section_heading("[Chorus]"),
            Some(("chorus".to_owned(), None))
        );
        assert_eq!(
            section_heading("Pre-Chorus:"),
            Some(("prechorus".to_owned(), None))
        );
        assert_eq!(
            section_heading("  Verse 12  "),
            Some(("verse".to_owned(), Some("12".to_owned())))
        );
        assert_eq!(
            section_heading("Bridge b"),
            Some(("bridge".to_owned(), Some("b".to_owned())))
        );
        // A lyric that merely opens with the word is not a heading.
        assert_eq!(section_heading("Bridge over troubled water"), None);
        assert_eq!(section_heading("Amazing grace"), None);
    }

    /// A chord line with nothing under it keeps its columns rather than binding to a heading.
    #[test]
    fn keeps_a_chord_line_that_has_no_lyric_beneath_it() {
        let converted = to_chord_pro("Intro\nG   C\n\nVerse\nG\nWords");

        assert!(converted.contains("[G]    [C]"));
        assert!(converted.contains("{intro}"));
        assert!(converted.contains("[G]Words"));
    }

    /// A chord past the end of its lyric keeps its column as trailing spaces.
    #[test]
    fn a_chord_past_the_end_of_the_lyric_keeps_its_column() {
        assert_eq!(
            to_chord_pro("G        C\nShort").trim_end(),
            "[G]Short    [C]"
        );
    }

    #[test]
    fn a_paste_full_of_blank_lines_comes_back_readable() {
        assert_eq!(to_chord_pro("G\nOne\n\n\n\n\nC\nTwo"), "[G]One\n\n[C]Two\n");
    }

    /// Business rule 2 — converting must never lose a line of somebody's song. Take the chords
    /// back out of the conversion and the lyrics have to be exactly what was pasted.
    #[test]
    fn keeps_every_lyric_line_it_was_given() {
        let mut lyrics = String::new();
        let mut depth = 0;

        for character in to_chord_pro(PASTED).chars() {
            match character {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ if depth == 0 => lyrics.push(character),
                _ => {}
            }
        }

        assert_eq!(
            lyrics.trim(),
            "{verse: 1}\nAmazing grace how sweet\nthe sound of it all"
        );
    }
}
