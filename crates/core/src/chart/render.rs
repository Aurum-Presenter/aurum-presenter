//! Turning the parsed model into text on a screen.
//!
//! Transposition lives here rather than in the store because it is a display transform and
//! nothing else (business rule 6): the ChordPro body is byte-identical before and after any
//! number of key changes, which is what lets two members read the same chart in two keys.

use serde::{Deserialize, Serialize};

use super::chord::ChordToken;
use super::chordpro::{Chart, SectionKind};
use super::notes::Key;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    #[default]
    Inline,
    Over,
    Nashville,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderOptions {
    /// The key the stored body is written in.
    pub source: Key,
    /// The sounding key the reader wants.
    pub target: Key,
    /// 0–11. Shapes are drawn `capo` semitones below the sounding key (business rule 11).
    pub capo: i16,
    pub layout: Layout,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedSegment {
    pub chord: Option<String>,
    pub lyric: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedLine {
    pub segments: Vec<RenderedSegment>,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedSection {
    pub kind: SectionKind,
    pub label: Option<String>,
    pub lines: Vec<RenderedLine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedChart {
    pub sections: Vec<RenderedSection>,
    /// The key the chord shapes are in — the sounding key, unless a capo is fitted.
    pub shape_key: Key,
    /// True when a spelling needed a double accidental and a simpler enharmonic was used.
    pub respelled: bool,
}

/// The key the shapes are written in: sounding key minus the capo position.
pub fn shape_key_of(target: &Key, capo: i16) -> Key {
    if capo == 0 {
        *target
    } else {
        target.transposed(-capo)
    }
}

impl Chart {
    pub fn render(&self, options: &RenderOptions) -> RenderedChart {
        let shape_key = shape_key_of(&options.target, options.capo);
        let semitones =
            (shape_key.tonic.pitch_class() - options.source.tonic.pitch_class()).rem_euclid(12);
        let mut respelled = false;

        let mut render_token = |token: &Option<ChordToken>| -> Option<String> {
            let token = token.as_ref()?;

            let Some(chord) = &token.chord else {
                // Business rule 3: unreadable tokens stay exactly as written, in chord position.
                return Some(token.text.clone());
            };

            if options.layout == Layout::Nashville {
                // Degrees are relative to the key, so they are the same before and after
                // transposition — and a capo cannot change them either.
                return Some(chord.nashville(&options.source));
            }

            let transposed = chord.transposed(semitones, &shape_key);
            respelled = respelled || transposed.respelled;

            Some(transposed.chord.to_string())
        };

        let sections = self
            .sections
            .iter()
            .map(|section| RenderedSection {
                kind: section.kind,
                label: section.label.clone(),
                lines: section
                    .lines
                    .iter()
                    .map(|line| RenderedLine {
                        comment: line.comment.clone(),
                        segments: line
                            .segments
                            .iter()
                            .map(|segment| RenderedSegment {
                                chord: render_token(&segment.chord),
                                lyric: segment.lyric.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect();

        RenderedChart {
            sections,
            shape_key,
            respelled,
        }
    }
}

/// The two rows of a chords-over-lyrics line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OverLyricsRows {
    pub chords: String,
    pub lyrics: String,
}

impl RenderedLine {
    /// Chords padded to the column their lyric starts at.
    ///
    /// This is the inverse of the paste converter, and acceptance criterion 1 depends on the pair
    /// round-tripping — a chord pasted at column 12 is stored at offset 12 and lands back at 12.
    pub fn over_lyrics_rows(&self) -> OverLyricsRows {
        let mut chords = String::new();
        let mut lyrics = String::new();

        for segment in &self.segments {
            if let Some(chord) = &segment.chord {
                // A chord whose column is already covered by the previous chord's text is nudged
                // right, so two chords never run together into one unreadable token.
                if width(&chords) > width(&lyrics) {
                    pad_to(&mut lyrics, width(&chords) + 1);
                }

                pad_to(&mut chords, width(&lyrics));
                chords.push_str(chord);
            }

            lyrics.push_str(&segment.lyric);
        }

        OverLyricsRows {
            chords: chords.trim_end().to_owned(),
            lyrics,
        }
    }

    /// Inline layout: `[G]` brackets, as the body is stored.
    pub fn inline_text(&self) -> String {
        self.segments
            .iter()
            .map(|segment| match &segment.chord {
                Some(chord) => format!("[{chord}]{}", segment.lyric),
                None => segment.lyric.clone(),
            })
            .collect()
    }
}

/// Columns, not bytes: a lyric with an accent in it must not shift the chords above it.
fn width(text: &str) -> usize {
    text.chars().count()
}

fn pad_to(text: &mut String, columns: usize) {
    for _ in width(text)..columns {
        text.push(' ');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Key {
        Key::parse(text).expect("a key")
    }

    fn options(from: &str, to: &str, capo: i16, layout: Layout) -> RenderOptions {
        RenderOptions {
            source: key(from),
            target: key(to),
            capo,
            layout,
        }
    }

    /// Every chord of a body at a target key, in reading order.
    fn chords_of(body: &str, options: &RenderOptions) -> Vec<String> {
        Chart::parse(body)
            .render(options)
            .sections
            .iter()
            .flat_map(|section| section.lines.clone())
            .flat_map(|line| line.segments)
            .filter_map(|segment| segment.chord)
            .collect()
    }

    /// Acceptance criterion 2 — the enharmonic follows the target key signature, not a table.
    #[test]
    fn spells_the_fourth_as_gb_in_db_and_as_f_sharp_in_b() {
        let body = "[F]Amazing [G]grace";

        assert_eq!(
            chords_of(body, &options("C", "Db", 0, Layout::Inline)),
            ["Gb", "Ab"]
        );
        assert_eq!(
            chords_of(body, &options("C", "B", 0, Layout::Inline)),
            ["E", "F#"]
        );
    }

    /// Business rule 9 — a minor key spells from its relative major.
    #[test]
    fn transposes_a_minor_key_with_its_relative_majors_signature() {
        assert_eq!(
            chords_of("[Am] [F] [C] [G]", &options("Am", "Cm", 0, Layout::Inline)),
            ["Cm", "Ab", "Eb", "Bb"]
        );
    }

    /// Business rule 14 — no Fx; the chart reports that it was respelled.
    #[test]
    fn reports_a_respelling_up_to_the_chart_header() {
        let rendered = Chart::parse("[C#]").render(&options("C", "C#", 0, Layout::Inline));

        assert!(rendered.respelled);
        assert_eq!(
            chords_of("[C#]", &options("C", "C#", 0, Layout::Inline)),
            ["D"]
        );
    }

    /// Business rule 6 — the stored body is byte-identical however many keys it is read in.
    #[test]
    fn leaves_the_stored_body_untouched_whatever_key_it_is_read_in() {
        let body = "[G]Nothing [C]here [D]changes";
        let chart = Chart::parse(body);

        chart.render(&options("G", "Bb", 3, Layout::Over));
        chart.render(&options("G", "E", 0, Layout::Nashville));

        assert_eq!(
            chart.render(&options("G", "G", 0, Layout::Inline)).sections[0].lines[0].inline_text(),
            body
        );
    }

    /// Acceptance criterion 4 — capo 2 sounding in D means shapes in C.
    #[test]
    fn draws_shapes_below_the_sounding_key_without_changing_it() {
        let rendered = Chart::parse("[D] [G] [A]").render(&options("D", "D", 2, Layout::Inline));

        assert_eq!(rendered.shape_key.to_string(), "C");
        assert_eq!(
            chords_of("[D] [G] [A]", &options("D", "D", 2, Layout::Inline)),
            ["C", "F", "G"]
        );
    }

    /// Acceptance criterion 7.
    #[test]
    fn numbers_degrees_relative_to_the_key() {
        assert_eq!(
            chords_of("[G] [C] [D] [Em]", &options("G", "G", 0, Layout::Nashville)),
            ["1", "4", "5", "6m"]
        );
    }

    /// A capo cannot renumber a degree — Nashville is written against the sounding key.
    #[test]
    fn nashville_ignores_the_capo_and_the_target_key() {
        assert_eq!(
            chords_of("[G] [C]", &options("G", "Bb", 3, Layout::Nashville)),
            ["1", "4"]
        );
    }

    /// Acceptance criterion 6 — unreadable tokens render verbatim.
    #[test]
    fn renders_an_unreadable_token_exactly_as_written() {
        assert_eq!(
            chords_of("[Hmm]Something", &options("C", "D", 0, Layout::Inline)),
            ["Hmm"]
        );
    }

    /// Acceptance criterion 1 — a chord pasted at a column lands back at that column.
    #[test]
    fn puts_every_chord_back_in_the_column_it_was_stored_at() {
        let chart = Chart::parse("[G]Amazing grace how[C] sweet");
        let rows = chart.render(&options("G", "G", 0, Layout::Over)).sections[0].lines[0]
            .over_lyrics_rows();

        assert_eq!(rows.lyrics, "Amazing grace how sweet");
        assert_eq!(rows.chords, "G                C");
    }

    /// Two chords in a row must not run together into one unreadable token.
    #[test]
    fn nudges_a_chord_that_would_collide_with_the_one_before_it() {
        let chart = Chart::parse("[G][C]Together");
        let rows = chart.render(&options("G", "G", 0, Layout::Over)).sections[0].lines[0]
            .over_lyrics_rows();

        assert_eq!(rows.chords, "G C");
        assert_eq!(rows.lyrics, "  Together");
    }
}
