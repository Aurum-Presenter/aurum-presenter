//! Drawing a slide.
//!
//! The audience and the stage render the same slide with different rules: the audience gets
//! lyrics, large, on the theme's background; the stage keeps the chords and puts them in the
//! reader's own key. Neither holds any logic about what happens next — they are functions of
//! the state they were handed.

use aurum_core::chart::chordpro::{Chart, Section, SectionKind};
use aurum_core::chart::notes::Key;
use aurum_core::chart::render::{Layout, RenderOptions};
use aurum_core::present::session::{Align, Theme};
use aurum_core::present::slides::{Slide, SlideKind, text_of};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::blobs::BlobStore;
use crate::db::Database;
use crate::sheets::renderer::{PdfiumRenderer, SheetRenderer, bytes_of};

/// The width a sheet is rasterised at for a projector. Beyond this the engine is doing work no
/// screen can show.
const SHEET_WIDTH: u32 = 1920;

/// The smallest a slide will ever shrink to. Below this, splitting has failed and shrinking
/// further only makes it unreadable from the back of a room.
const MINIMUM_VH: f64 = 2.5;

/// Fitting text rather than letting it overflow: a long verse comes down in size until it fits,
/// which is what the slide-splitting rules leave for the renderer to finish.
fn fitted(maximum: f64, lines: &[String]) -> f64 {
    let longest = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    let by_count = if lines.is_empty() {
        maximum
    } else {
        maximum.min(78.0 / lines.len() as f64)
    };

    let by_width = if longest == 0 {
        maximum
    } else {
        maximum.min(170.0 / longest as f64)
    };

    by_count.min(by_width).max(MINIMUM_VH)
}

fn lines_of(slide: &Slide) -> Vec<String> {
    match slide.kind {
        SlideKind::Text => slide
            .text
            .as_deref()
            .unwrap_or("")
            .lines()
            .map(str::to_owned)
            .collect(),
        _ => slide.lines.iter().map(text_of).collect(),
    }
}

/// Sizes are in container units, not viewport units, so the same component fills a projector and
/// fits the little preview on the control surface — at the same proportions, which is the whole
/// point of a preview.
#[component]
pub fn AudienceSlide(
    slide: Signal<Option<Slide>>,
    theme: Signal<Theme>,
    workspace_id: Signal<String>,
) -> impl IntoView {
    view! {
        {move || {
            let held = slide.get();
            let theme = theme.get();

            if let Some(sheet) = held.as_ref().filter(|slide| slide.kind == SlideKind::Sheet) {
                return view! { <SheetSlide slide=sheet.clone() workspace_id /> }.into_any();
            }

            let lines = held.as_ref().map(lines_of).unwrap_or_default();
            let size = fitted(theme.font_size_vh, &lines);
            let label = held.as_ref().and_then(|slide| slide.label.clone());

            view! {
                <div
                    class="flex h-full w-full flex-col items-center justify-center"
                    style=format!(
                        "container-type: size; padding: {}cqh {}cqw",
                        theme.safe_area_pct,
                        theme.safe_area_pct,
                    )
                >
                    {(theme.show_section_labels).then_some(label).flatten().map(|label| view! {
                        <p
                            class="mb-4 uppercase tracking-widest opacity-60"
                            style=format!("font-size: {}cqh", size / 3.0)
                        >
                            {label}
                        </p>
                    })}

                    <div
                        class=if theme.align == Align::Center {
                            "w-full text-center"
                        } else {
                            "w-full text-left"
                        }
                        style=format!(
                            "font-size: {size}cqh; line-height: 1.25; font-family: {}",
                            theme.font_family,
                        )
                    >
                        {lines
                            .into_iter()
                            .map(|line| view! {
                                <p class="whitespace-pre-wrap">
                                    {if line.is_empty() { " ".to_owned() } else { line }}
                                </p>
                            })
                            .collect_view()}
                    </div>
                </div>
            }
            .into_any()
        }}
    }
}

/// Stage-view business rule 7: chords in the reader's own key, unless the set fixed one for
/// everybody.
#[component]
pub fn StageSlide(
    slide: Signal<Option<Slide>>,
    target_key: Signal<Option<Key>>,
    show_chords: Signal<bool>,
    /// Height units, so the same component is a stage screen and a preview.
    scale: Signal<f64>,
    #[prop(default = "Waiting for the first slide…")] empty: &'static str,
) -> impl IntoView {
    view! {
        {move || {
            let Some(slide) = slide.get() else {
                return view! { <p class="opacity-60">{empty}</p> }.into_any();
            };

            let scale = scale.get();

            match slide.kind {
                SlideKind::Text => {
                    return view! {
                        <p class="whitespace-pre-wrap" style=format!("font-size: {scale}vh")>
                            {slide.text.clone().unwrap_or_default()}
                        </p>
                    }
                    .into_any();
                }

                // Nothing to read: the stage says which song it is and stays out of the way.
                SlideKind::Blank | SlideKind::Sheet | SlideKind::Title => {
                    return view! {
                        <p class="opacity-60" style=format!("font-size: {}vh", scale / 2.0)>
                            {slide.song_title.clone()}
                        </p>
                    }
                    .into_any();
                }

                SlideKind::Lyrics => {}
            }

            let fallback = Key::parse("C").expect("C is a key");
            let written = slide
                .written_key
                .as_deref()
                .and_then(Key::parse)
                .unwrap_or(fallback);
            let target = slide
                .set_key
                .as_deref()
                .and_then(Key::parse)
                .or_else(|| target_key.get())
                .unwrap_or(written);

            let chart = Chart {
                sections: vec![Section {
                    kind: SectionKind::None,
                    label: None,
                    lines: slide.lines.clone(),
                }],
                ..Chart::default()
            };

            let rendered = chart.render(&RenderOptions {
                source: written,
                target,
                capo: slide.capo,
                layout: Layout::Over,
            });

            let chords = show_chords.get();

            view! {
                <div style=format!("font-size: {scale}vh")>
                    {rendered
                        .sections
                        .first()
                        .map(|section| section.lines.clone())
                        .unwrap_or_default()
                        .into_iter()
                        .map(|line| {
                            let rows = line.over_lyrics_rows();

                            view! {
                                <div class="font-mono leading-tight">
                                    {chords.then(|| view! {
                                        <div class="whitespace-pre font-semibold text-sky-300">
                                            {if rows.chords.is_empty() {
                                                " ".to_owned()
                                            } else {
                                                rows.chords.clone()
                                            }}
                                        </div>
                                    })}
                                    <div class="whitespace-pre">
                                        {if rows.lyrics.is_empty() {
                                            " ".to_owned()
                                        } else {
                                            rows.lyrics
                                        }}
                                    </div>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            }
            .into_any()
        }}
    }
}

/// A sheet page on the audience screen, rendered from the file this device already holds.
///
/// The output window is the same origin as the control surface, so it opens the same local
/// database and reads the same cached file. Nothing is fetched: if the file is not on the device
/// the slide says which page it would have been, rather than showing a broken image to a room.
#[component]
fn SheetSlide(slide: Slide, workspace_id: Signal<String>) -> impl IntoView {
    let image = RwSignal::new(None::<String>);
    let title = slide.song_title.clone();
    let page = slide.page.unwrap_or(1);

    if let Some(sheet_id) = slide.sheet_id.clone() {
        let workspace_id = workspace_id.get_untracked();

        spawn_local(async move {
            let Ok(db) = Database::open(&workspace_id).await else {
                return;
            };

            let store = BlobStore::new(db, &workspace_id);

            let (Some(blob), Ok(renderer)) =
                (store.get(&sheet_id).await, PdfiumRenderer::load().await)
            else {
                return;
            };

            let Some(bytes) = bytes_of(&blob).await else {
                return;
            };

            if let Ok(rendered) = renderer.render(&bytes, (page as i32) - 1, SHEET_WIDTH) {
                image.set(rendered.to_data_url());
            }
        });
    }

    view! {
        <div class="flex h-full w-full items-center justify-center">
            {move || match image.get() {
                Some(source) => view! {
                    <img src=source alt="" class="max-h-full max-w-full" />
                }
                .into_any(),

                None => view! {
                    <p class="opacity-60">{format!("{title} — page {page}")}</p>
                }
                .into_any(),
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_slide_uses_the_themes_size() {
        assert_eq!(fitted(8.0, &["Amazing grace".to_owned()]), 8.0);
    }

    #[test]
    fn a_verse_with_many_lines_comes_down_to_fit() {
        let many: Vec<String> = (0..20).map(|_| "a line".to_owned()).collect();

        assert!(fitted(8.0, &many) < 8.0);
    }

    #[test]
    fn one_very_long_line_comes_down_too() {
        let wide = vec!["x".repeat(60)];

        assert!(fitted(8.0, &wide) < 8.0);
    }

    #[test]
    fn it_never_shrinks_past_readable() {
        let awful: Vec<String> = (0..200).map(|_| "x".repeat(300)).collect();

        assert_eq!(fitted(8.0, &awful), MINIMUM_VH);
    }
}
