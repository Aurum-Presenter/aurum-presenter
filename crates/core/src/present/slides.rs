//! Turning a set into slides.
//!
//! The rules are deterministic and run once, at session start (business rule 6): a slide list
//! that could change while the operator is on slide 14 is a slide list that will change while
//! the operator is on slide 14.
//!
//! Slides keep the *parsed* lines rather than rendered text, because the audience and the stage
//! want different things from them — the audience gets lyrics with the chords stripped, and the
//! stage gets chords at its own reader's key.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::chart::chordpro::{Chart, ChartLine, Section, SectionKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlideKind {
    #[default]
    Lyrics,
    Text,
    Blank,
    Sheet,
    Title,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slide {
    pub id: String,
    /// The set item this came from, so the control surface can group slides by song.
    pub item_id: String,
    pub kind: SlideKind,
    pub song_title: String,
    /// "Chorus", "Verse 2" — shown on stage always, on the audience only if the theme says so.
    pub label: Option<String>,
    /// Parsed lines: chords and lyrics, untransposed. Empty for blank and sheet slides.
    pub lines: Vec<ChartLine>,
    /// Plain text for a non-song item.
    pub text: Option<String>,
    pub sheet_id: Option<String>,
    pub page: Option<u32>,
    /// The key the lines are written in, so a stage view can transpose them to its own.
    pub written_key: Option<String>,
    /// The key the band agreed for this set, if any; it wins over a reader's preference.
    pub set_key: Option<String>,
    pub capo: i16,
}

/// One set item, frozen at session start. Nothing here is read from the database again.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotItem {
    pub item_id: String,
    pub song_id: Option<String>,
    pub title: String,
    /// ChordPro, exactly as it was when the session started.
    pub body: Option<String>,
    pub written_key: Option<String>,
    pub set_key: Option<String>,
    pub capo: i16,
    pub item_type: Option<String>,
    pub content: Option<String>,
    pub note: Option<String>,
    pub sheet_id: Option<String>,
    pub sheet_pages: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub set_id: Option<String>,
    pub set_name: String,
    pub items: Vec<SnapshotItem>,
    pub taken_at: String,
}

fn label_name(kind: SectionKind) -> Option<&'static str> {
    Some(match kind {
        SectionKind::Verse => "Verse",
        SectionKind::Chorus => "Chorus",
        SectionKind::Bridge => "Bridge",
        SectionKind::Prechorus => "Pre-chorus",
        SectionKind::Tag => "Tag",
        SectionKind::Intro => "Intro",
        SectionKind::Outro => "Outro",
        SectionKind::None => return None,
    })
}

/// How many lines fit on a slide at this theme's size.
///
/// Derived rather than measured so that two devices building the same session get the same
/// slides — an audience window and a stage device that disagree about slide count cannot follow
/// the same index.
pub fn lines_per_slide(font_size_vh: f64, safe_area_pct: f64) -> usize {
    let usable = 100.0 - safe_area_pct * 2.0;

    ((usable / (font_size_vh * 1.35)).floor() as i64).max(2) as usize
}

/// The theme's defaults, matching `Theme`'s own — a slide list built without a theme is the one
/// the default theme would have produced.
pub const DEFAULT_FONT_SIZE_VH: f64 = 8.0;
pub const DEFAULT_SAFE_AREA_PCT: f64 = 5.0;

impl Snapshot {
    pub fn slides(&self, font_size_vh: f64, safe_area_pct: f64) -> Vec<Slide> {
        let per_slide = lines_per_slide(font_size_vh, safe_area_pct);
        let mut slides = Vec::new();

        for (item_index, item) in self.items.iter().enumerate() {
            let base = Slide {
                item_id: item.item_id.clone(),
                song_title: item.title.clone(),
                written_key: item.written_key.clone(),
                set_key: item.set_key.clone(),
                capo: item.capo,
                ..Slide::default()
            };

            let body = item.body.as_deref().unwrap_or("").trim();

            // Business rule 4: a non-song item is its own slide, and a blank item is a black one.
            if body.is_empty() {
                if item.item_type.as_deref() == Some("blank") {
                    slides.push(Slide {
                        id: format!("{item_index}-blank"),
                        kind: SlideKind::Blank,
                        ..base
                    });
                    continue;
                }

                match item
                    .content
                    .as_deref()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                {
                    Some(content) => {
                        for (part, text) in split_text(content, per_slide).into_iter().enumerate() {
                            slides.push(Slide {
                                id: format!("{item_index}-text-{part}"),
                                kind: SlideKind::Text,
                                text: Some(text),
                                ..base.clone()
                            });
                        }
                    }
                    None => slides.push(Slide {
                        id: format!("{item_index}-title"),
                        kind: SlideKind::Title,
                        ..base
                    }),
                }

                continue;
            }

            // Business rule 5: a sheet item becomes one slide per page.
            if let Some(sheet_id) = &item.sheet_id {
                for page in 1..=item.sheet_pages.unwrap_or(1) {
                    slides.push(Slide {
                        id: format!("{item_index}-sheet-{page}"),
                        kind: SlideKind::Sheet,
                        sheet_id: Some(sheet_id.clone()),
                        page: Some(page),
                        ..base.clone()
                    });
                }

                continue;
            }

            let chart = Chart::parse(item.body.as_deref().unwrap_or(""));

            for (section_index, section) in expand_repeats(&chart.sections).iter().enumerate() {
                let lines: Vec<ChartLine> = section
                    .lines
                    .iter()
                    .filter(|line| !line.is_blank())
                    .cloned()
                    .collect();

                if lines.is_empty() {
                    continue;
                }

                for (part, group) in split_lines(&lines, per_slide).into_iter().enumerate() {
                    slides.push(Slide {
                        id: format!("{item_index}-{section_index}-{part}"),
                        kind: SlideKind::Lyrics,
                        label: label_of(section),
                        lines: group,
                        ..base.clone()
                    });
                }
            }
        }

        slides
    }
}

fn label_of(section: &Section) -> Option<String> {
    let name = label_name(section.kind)?;

    Some(match &section.label {
        Some(label) => format!("{name} {label}"),
        None => name.to_owned(),
    })
}

/// Business rule 3: a bare `{chorus}` after the chorus has been written once is a repeat, and it
/// gets its own slides rather than being silently dropped.
fn expand_repeats(sections: &[Section]) -> Vec<Section> {
    let mut seen: HashMap<String, Vec<ChartLine>> = HashMap::new();

    sections
        .iter()
        .map(|section| {
            let key = format!(
                "{}:{}",
                section.kind.as_str(),
                section.label.as_deref().unwrap_or("")
            );

            if section.lines.iter().any(|line| !line.is_blank()) {
                seen.insert(key, section.lines.clone());
                return section.clone();
            }

            let earlier = seen
                .get(&key)
                .or_else(|| seen.get(&format!("{}:", section.kind.as_str())));

            match earlier {
                Some(lines) => Section {
                    lines: lines.clone(),
                    ..section.clone()
                },
                None => section.clone(),
            }
        })
        .collect()
}

/// Business rule 2: split at a sentence end, then at a line end. Never mid-line — a lyric cut in
/// half mid-phrase is worse than a smaller font.
pub fn split_lines(lines: &[ChartLine], per_slide: usize) -> Vec<Vec<ChartLine>> {
    if lines.len() <= per_slide {
        return vec![lines.to_vec()];
    }

    let mut groups = Vec::new();
    let mut rest = lines;

    while rest.len() > per_slide {
        let window = &rest[..per_slide];
        let mut cut = window.len();

        for index in (per_slide.div_ceil(2)..window.len()).rev() {
            if ends_sentence(&window[index]) {
                cut = index + 1;
                break;
            }
        }

        groups.push(rest[..cut].to_vec());
        rest = &rest[cut..];
    }

    if !rest.is_empty() {
        groups.push(rest.to_vec());
    }

    groups
}

fn ends_sentence(line: &ChartLine) -> bool {
    let text = text_of(line);
    let trimmed = text.trim_end();
    let trimmed = trimmed.strip_suffix('"').unwrap_or(trimmed);

    trimmed.ends_with(['.', '!', '?'])
}

pub fn text_of(line: &ChartLine) -> String {
    match &line.comment {
        Some(comment) => comment.clone(),
        None => line
            .segments
            .iter()
            .map(|segment| segment.lyric.as_str())
            .collect(),
    }
}

/// The same splitting, for the plain text of an announcement or a reading.
pub fn split_text(content: &str, per_slide: usize) -> Vec<String> {
    let mut chunks = Vec::new();

    for paragraph in content
        .split('\n')
        .collect::<Vec<_>>()
        .split(|line| line.trim().is_empty())
    {
        for group in paragraph.chunks(per_slide) {
            let text = group.join("\n");

            if !text.trim().is_empty() {
                chunks.push(text);
            }
        }
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(body: Option<&str>) -> SnapshotItem {
        SnapshotItem {
            item_id: "item-1".to_owned(),
            song_id: Some("song-1".to_owned()),
            title: "Amazing Grace".to_owned(),
            body: body.map(str::to_owned),
            written_key: Some("G".to_owned()),
            ..SnapshotItem::default()
        }
    }

    fn snapshot(items: Vec<SnapshotItem>) -> Snapshot {
        Snapshot {
            set_id: Some("set-1".to_owned()),
            set_name: "Sunday".to_owned(),
            items,
            taken_at: "2026-09-06T09:00:00Z".to_owned(),
        }
    }

    fn built(body: &str) -> Vec<Slide> {
        snapshot(vec![item(Some(body))]).slides(DEFAULT_FONT_SIZE_VH, DEFAULT_SAFE_AREA_PCT)
    }

    fn texts(slide: &Slide) -> Vec<String> {
        slide.lines.iter().map(text_of).collect()
    }

    #[test]
    fn makes_one_slide_per_section_labelled() {
        let slides = built(
            "{verse: 1}\n[G]Amazing grace\nhow sweet the sound\n\n{chorus}\n[C]My chains are gone",
        );

        assert_eq!(
            slides
                .iter()
                .map(|slide| slide.label.clone())
                .collect::<Vec<_>>(),
            [Some("Verse 1".to_owned()), Some("Chorus".to_owned())]
        );
        assert_eq!(texts(&slides[0]), ["Amazing grace", "how sweet the sound"]);
    }

    #[test]
    fn keeps_chords_on_the_slide_so_the_stage_can_transpose_them_itself() {
        let slides = built("{verse}\n[G]Amazing [D]grace");

        assert_eq!(
            slides[0].lines[0].segments[0]
                .chord
                .as_ref()
                .map(|token| token.text.as_str()),
            Some("G")
        );
        assert_eq!(slides[0].written_key.as_deref(), Some("G"));
    }

    /// Business rule 3.
    #[test]
    fn expands_a_repeated_section_into_its_own_slides() {
        let slides =
            built("{chorus}\nMy chains are gone\n\n{verse: 2}\nThe Lord has promised\n\n{chorus}");

        assert_eq!(
            slides
                .iter()
                .map(|slide| slide.label.clone())
                .collect::<Vec<_>>(),
            [
                Some("Chorus".to_owned()),
                Some("Verse 2".to_owned()),
                Some("Chorus".to_owned())
            ]
        );
        assert_eq!(texts(&slides[2]), ["My chains are gone"]);
    }

    /// Business rule 2: split at a sentence end rather than mid-thought, and never mid-line.
    #[test]
    fn splits_a_long_section_at_a_sentence_end() {
        let body = concat!(
            "{verse}\n",
            "Line one goes here.\n",
            "Line two continues\n",
            "and line three ends it.\n",
            "Line four starts again\n",
            "line five\n",
            "line six"
        );
        let slides = snapshot(vec![item(Some(body))]).slides(20.0, 5.0);

        assert!(slides.len() > 1);
        assert_eq!(
            text_of(slides[0].lines.last().unwrap()),
            "and line three ends it."
        );
        assert_eq!(
            slides.iter().flat_map(texts).count(),
            6,
            "every line has to land on exactly one slide"
        );
    }

    #[test]
    fn never_splits_a_line_in_half() {
        let lines: Vec<String> = (0..20)
            .map(|index| format!("line number {index}"))
            .collect();
        let slides = snapshot(vec![item(Some(&format!(
            "{{verse}}\n{}",
            lines.join("\n")
        )))])
        .slides(12.0, 5.0);
        let seen: Vec<String> = slides.iter().flat_map(texts).collect();

        assert_eq!(seen, lines);
    }

    /// Business rule 4.
    #[test]
    fn turns_a_non_song_item_into_text_slides_and_a_blank_item_into_a_black_one() {
        let slides = snapshot(vec![
            SnapshotItem {
                item_id: "a".to_owned(),
                title: "Notices".to_owned(),
                item_type: Some("announcement".to_owned()),
                content: Some("Coffee is in the hall".to_owned()),
                ..SnapshotItem::default()
            },
            SnapshotItem {
                item_id: "b".to_owned(),
                title: "Blank".to_owned(),
                item_type: Some("blank".to_owned()),
                ..SnapshotItem::default()
            },
        ])
        .slides(DEFAULT_FONT_SIZE_VH, DEFAULT_SAFE_AREA_PCT);

        assert_eq!(slides[0].kind, SlideKind::Text);
        assert_eq!(slides[0].text.as_deref(), Some("Coffee is in the hall"));
        assert_eq!(slides[1].kind, SlideKind::Blank);
    }

    /// Business rule 5.
    #[test]
    fn makes_one_slide_per_page_of_a_sheet_item() {
        let slides = snapshot(vec![SnapshotItem {
            sheet_id: Some("sheet-1".to_owned()),
            sheet_pages: Some(3),
            ..item(Some("{verse}\nSomething"))
        }])
        .slides(DEFAULT_FONT_SIZE_VH, DEFAULT_SAFE_AREA_PCT);

        assert_eq!(slides.len(), 3);
        assert_eq!(
            slides.iter().map(|slide| slide.page).collect::<Vec<_>>(),
            [Some(1), Some(2), Some(3)]
        );
        assert_eq!(slides[0].kind, SlideKind::Sheet);
    }

    #[test]
    fn carries_the_set_key_so_every_output_agrees_on_it() {
        let slides = snapshot(vec![SnapshotItem {
            set_key: Some("A".to_owned()),
            capo: 2,
            ..item(Some("{verse}\nA line"))
        }])
        .slides(DEFAULT_FONT_SIZE_VH, DEFAULT_SAFE_AREA_PCT);

        assert_eq!(slides[0].set_key.as_deref(), Some("A"));
        assert_eq!(slides[0].capo, 2);
    }

    /// Business rule 6 — two devices given the same snapshot build the same list.
    #[test]
    fn is_deterministic() {
        let input = snapshot(vec![item(Some(
            "{verse}\nOne\nTwo\nThree\n\n{chorus}\nFour",
        ))]);

        assert_eq!(input.slides(8.0, 5.0), input.slides(8.0, 5.0));
    }

    #[test]
    fn fits_fewer_lines_as_the_font_grows() {
        assert!(lines_per_slide(4.0, 5.0) > lines_per_slide(10.0, 5.0));
        assert!(lines_per_slide(40.0, 5.0) >= 2);
    }

    #[test]
    fn splits_plain_text_on_its_paragraphs() {
        assert_eq!(split_text("One\n\nTwo\n\n", 4), ["One", "Two"]);
        assert_eq!(split_text("a\nb\nc\nd", 2), ["a\nb", "c\nd"]);
    }
}
