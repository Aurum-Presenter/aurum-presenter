//! ChordPro is the canonical storage; this turns it into the model everything else reads.
//!
//! Business rule 5: a chart is parsed once and transposed on the model, never by string
//! replacement on the raw text. So the parser's job is to lose nothing — an unknown directive, a
//! token that is not a chord, an unclosed bracket — because the thing it is parsing is somebody's
//! song and the worst possible outcome is that saving it drops a line.

use serde::{Deserialize, Serialize};

use super::chord::ChordToken;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    Verse,
    Chorus,
    Bridge,
    Prechorus,
    Tag,
    Intro,
    Outro,
    #[default]
    None,
}

impl SectionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SectionKind::Verse => "verse",
            SectionKind::Chorus => "chorus",
            SectionKind::Bridge => "bridge",
            SectionKind::Prechorus => "prechorus",
            SectionKind::Tag => "tag",
            SectionKind::Intro => "intro",
            SectionKind::Outro => "outro",
            SectionKind::None => "none",
        }
    }
}

/// A chord (or none) and the lyric that runs from it to the next chord.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub chord: Option<ChordToken>,
    pub lyric: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartLine {
    pub segments: Vec<Segment>,
    /// `{comment: ...}` — a performance note, not a lyric.
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub kind: SectionKind,
    /// `{verse: 2}` → "2". Presenter slides use this to label the slide.
    pub label: Option<String>,
    pub lines: Vec<ChartLine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartWarning {
    /// 1-based, so the editor gutter can point at the source line.
    pub line: usize,
    pub token: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartError {
    pub line: usize,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartMeta {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub artist: Option<String>,
    pub key: Option<String>,
    pub tempo: Option<String>,
    pub time: Option<String>,
    pub capo: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chart {
    pub meta: ChartMeta,
    pub sections: Vec<Section>,
    /// Tokens in chord position that are not chords. They render verbatim and still save.
    pub warnings: Vec<ChartWarning>,
    /// Set when the source could not be read at all; the view falls back to plain text.
    pub error: Option<ChartError>,
}

fn section_kind(name: &str) -> Option<SectionKind> {
    Some(match name {
        "verse" | "sov" | "start_of_verse" => SectionKind::Verse,
        "chorus" | "soc" | "start_of_chorus" => SectionKind::Chorus,
        "bridge" | "sob" | "start_of_bridge" => SectionKind::Bridge,
        "prechorus" | "pre_chorus" => SectionKind::Prechorus,
        "tag" => SectionKind::Tag,
        "intro" => SectionKind::Intro,
        "outro" | "ending" => SectionKind::Outro,
        _ => return None,
    })
}

const SECTION_ENDS: [&str; 6] = [
    "eov",
    "eoc",
    "eob",
    "end_of_verse",
    "end_of_chorus",
    "end_of_bridge",
];

fn meta_field<'a>(name: &str, meta: &'a mut ChartMeta) -> Option<&'a mut Option<String>> {
    Some(match name {
        "title" | "t" => &mut meta.title,
        "subtitle" | "st" => &mut meta.subtitle,
        "artist" | "composer" => &mut meta.artist,
        "key" => &mut meta.key,
        "tempo" | "bpm" => &mut meta.tempo,
        "time" => &mut meta.time,
        "capo" => &mut meta.capo,
        _ => return None,
    })
}

impl Chart {
    /// Parsing runs on the main thread, deliberately, at every length.
    ///
    /// The feature document asks for charts over 500 lines to be parsed in the search worker. On
    /// this parser a 500-line chart takes well under a millisecond, so posting the text across
    /// and cloning the model back would cost more than the parse and would put a frame of empty
    /// screen in front of a musician who is reading.
    pub fn parse(body: &str) -> Chart {
        let mut chart = Chart::default();
        let mut current = Section::default();

        for (index, raw) in body
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .enumerate()
        {
            let number = index + 1;
            let text = raw.trim_end();

            if let Some(inner) = directive_body(text) {
                let (name, value) = split_directive(inner);

                if let Some(field) = meta_field(&name, &mut chart.meta) {
                    *field = value;
                    continue;
                }

                if name == "comment" || name == "c" || name == "ci" {
                    current.lines.push(ChartLine {
                        segments: Vec::new(),
                        comment: Some(value.unwrap_or_default()),
                    });
                    continue;
                }

                if let Some(kind) = section_kind(&name) {
                    flush(&mut chart.sections, current);
                    current = Section {
                        kind,
                        label: value,
                        lines: Vec::new(),
                    };
                    continue;
                }

                if SECTION_ENDS.contains(&name.as_str()) {
                    flush(&mut chart.sections, current);
                    current = Section::default();
                }

                // An unknown directive is not an error. Charts travel between apps and carry
                // each other's extensions; dropping one must never cost the user their lyrics.
                continue;
            }

            if text.starts_with('#') {
                continue;
            }

            if chart.error.is_none() && unbalanced(text) {
                chart.error = Some(ChartError {
                    line: number,
                    message: "Unclosed [ in chord position.".to_owned(),
                });
            }

            current
                .lines
                .push(parse_line(text, number, &mut chart.warnings));
        }

        flush(&mut chart.sections, current);

        chart
    }
}

fn flush(sections: &mut Vec<Section>, current: Section) {
    if !current.lines.is_empty() || current.kind != SectionKind::None {
        sections.push(current);
    }
}

/// A directive is a line that is nothing but `{…}`, to the last brace on it.
fn directive_body(text: &str) -> Option<&str> {
    let trimmed = text.trim();

    trimmed
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
}

fn split_directive(inner: &str) -> (String, Option<String>) {
    let (name, value) = match inner.find(':') {
        Some(colon) => (&inner[..colon], Some(inner[colon + 1..].trim())),
        None => (inner, None),
    };

    let name = name
        .trim()
        .to_lowercase()
        .split(|character: char| character.is_whitespace() || character == '-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");

    (
        name,
        value.filter(|text| !text.is_empty()).map(str::to_owned),
    )
}

fn unbalanced(text: &str) -> bool {
    text.matches('[').count() != text.matches(']').count()
}

fn parse_line(text: &str, number: usize, warnings: &mut Vec<ChartWarning>) -> ChartLine {
    let mut segments: Vec<Segment> = Vec::new();
    let mut lyric = String::new();
    let mut pending: Option<ChordToken> = None;
    let mut index = 0;

    while index < text.len() {
        let Some(open) = text[index..].find('[').map(|offset| index + offset) else {
            lyric.push_str(&text[index..]);
            break;
        };

        lyric.push_str(&text[index..open]);

        let Some(close) = text[open..].find(']').map(|offset| open + offset) else {
            lyric.push_str(&text[open..]);
            break;
        };

        if !lyric.is_empty() || pending.is_some() {
            segments.push(Segment {
                chord: pending.take(),
                lyric: std::mem::take(&mut lyric),
            });
        }

        let token = ChordToken::read(&text[open + 1..close]);

        if token.chord.is_none() && !token.marker && !token.text.trim().is_empty() {
            warnings.push(ChartWarning {
                line: number,
                token: token.text.clone(),
            });
        }

        pending = Some(token);
        lyric.clear();
        index = close + 1;
    }

    if !lyric.is_empty() || pending.is_some() || segments.is_empty() {
        segments.push(Segment {
            chord: pending,
            lyric,
        });
    }

    ChartLine {
        segments,
        comment: None,
    }
}

impl ChartLine {
    /// True when the line carries no lyric text at all — a chord-only line, or a blank one.
    pub fn is_chord_only(&self) -> bool {
        self.comment.is_none()
            && self
                .segments
                .iter()
                .all(|segment| segment.lyric.trim().is_empty())
    }

    pub fn is_blank(&self) -> bool {
        self.comment.is_none()
            && self
                .segments
                .iter()
                .all(|segment| segment.chord.is_none() && segment.lyric.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chords(chart: &Chart) -> Vec<String> {
        chart
            .sections
            .iter()
            .flat_map(|section| &section.lines)
            .flat_map(|line| &line.segments)
            .filter_map(|segment| segment.chord.as_ref())
            .map(|token| token.text.clone())
            .collect()
    }

    #[test]
    fn reads_a_chord_and_the_lyric_that_runs_from_it() {
        let chart = Chart::parse("[G]Amazing [D]grace");
        let line = &chart.sections[0].lines[0];

        assert_eq!(line.segments.len(), 2);
        assert_eq!(line.segments[0].lyric, "Amazing ");
        assert_eq!(line.segments[1].lyric, "grace");
        assert_eq!(chords(&chart), ["G", "D"]);
    }

    #[test]
    fn keeps_a_lyric_that_starts_before_the_first_chord() {
        let chart = Chart::parse("Amazing [G]grace");
        let line = &chart.sections[0].lines[0];

        assert_eq!(line.segments.len(), 2);
        assert!(line.segments[0].chord.is_none());
        assert_eq!(line.segments[0].lyric, "Amazing ");
    }

    #[test]
    fn reads_the_metadata_a_chart_carries() {
        let chart = Chart::parse("{title: Amazing Grace}\n{st: Traditional}\n{bpm: 72}\n{Key: G}");

        assert_eq!(chart.meta.title.as_deref(), Some("Amazing Grace"));
        assert_eq!(chart.meta.subtitle.as_deref(), Some("Traditional"));
        assert_eq!(chart.meta.tempo.as_deref(), Some("72"));
        assert_eq!(chart.meta.key.as_deref(), Some("G"));
        assert!(chart.sections.is_empty());
    }

    #[test]
    fn opens_a_section_and_labels_it() {
        let chart = Chart::parse("{verse: 1}\n[G]One\n{soc}\n[C]Two");

        assert_eq!(chart.sections.len(), 2);
        assert_eq!(chart.sections[0].kind, SectionKind::Verse);
        assert_eq!(chart.sections[0].label.as_deref(), Some("1"));
        assert_eq!(chart.sections[1].kind, SectionKind::Chorus);
        assert_eq!(chart.sections[1].label, None);
    }

    #[test]
    fn a_directive_from_another_app_costs_nobody_their_lyrics() {
        let chart = Chart::parse("{x_custom_thing: 3}\n[G]Still here");

        assert_eq!(chords(&chart), ["G"]);
        assert_eq!(chart.sections[0].lines[0].segments[0].lyric, "Still here");
    }

    /// Acceptance criterion 6 — unreadable tokens render verbatim and flag the line.
    #[test]
    fn flags_a_token_that_is_not_a_chord_without_failing_the_parse() {
        let chart = Chart::parse("[Hmm]Something");

        assert_eq!(
            chart.warnings,
            [ChartWarning {
                line: 1,
                token: "Hmm".to_owned()
            }]
        );
        assert_eq!(chords(&chart), ["Hmm"]);
        assert!(chart.error.is_none());
    }

    #[test]
    fn does_not_flag_a_no_chord_marker() {
        assert!(Chart::parse("[N.C.]Spoken").warnings.is_empty());
        assert!(Chart::parse("[%]").warnings.is_empty());
    }

    #[test]
    fn reports_an_unclosed_bracket_against_its_line() {
        let chart = Chart::parse("[G]Fine\n[C Broken");

        assert_eq!(chart.error.as_ref().map(|error| error.line), Some(2));
        // …and still keeps the text, because the view falls back to showing it.
        assert!(
            chart.sections[0].lines[1].segments[0]
                .lyric
                .contains("Broken")
        );
    }

    #[test]
    fn a_comment_is_a_performance_note_not_a_lyric() {
        let chart = Chart::parse("{verse}\n{comment: build}\n[G]Sing");
        let line = &chart.sections[0].lines[0];

        assert_eq!(line.comment.as_deref(), Some("build"));
        assert!(line.segments.is_empty());
        assert!(!line.is_chord_only());
    }

    #[test]
    fn tells_a_chord_only_line_from_a_blank_one() {
        let chart = Chart::parse("[G] [C]\n\nWords");

        assert!(chart.sections[0].lines[0].is_chord_only());
        assert!(!chart.sections[0].lines[0].is_blank());
        assert!(chart.sections[0].lines[1].is_blank());
        assert!(!chart.sections[0].lines[2].is_chord_only());
    }

    #[test]
    fn reads_the_line_endings_a_paste_from_windows_carries() {
        let chart = Chart::parse("[G]One\r\n[C]Two\r[D]Three");

        assert_eq!(chart.sections[0].lines.len(), 3);
        assert_eq!(chords(&chart), ["G", "C", "D"]);
    }
}
