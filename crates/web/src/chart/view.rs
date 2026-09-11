//! The reading surface.
//!
//! Everything here is derived: the stored ChordPro is parsed once, and a key change re-renders
//! from the model rather than rewriting anything. That is business rule 6, and it is why two
//! members can read the same chart in two keys without either of them editing it.

use aurum_core::chart::chordpro::{Chart, SectionKind};
use aurum_core::chart::notes::Key;
use aurum_core::chart::render::{RenderOptions, RenderedLine};
use leptos::prelude::*;

use crate::prefs::display::{Display, Mode};

fn section_label(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::Verse => "Verse",
        SectionKind::Chorus => "Chorus",
        SectionKind::Bridge => "Bridge",
        SectionKind::Prechorus => "Pre-chorus",
        SectionKind::Tag => "Tag",
        SectionKind::Intro => "Intro",
        SectionKind::Outro => "Outro",
        SectionKind::None => "",
    }
}

#[component]
pub fn ChartView(
    body: Signal<String>,
    source: Signal<Key>,
    target: Signal<Key>,
    capo: Signal<i64>,
    display: Signal<Display>,
    /// Told when the target key needed a double accidental and a simpler one was used
    /// (business rule 14), so the header can say the chart was respelled.
    #[prop(optional)]
    on_respelled: Option<Callback<bool>>,
) -> impl IntoView {
    let chart = Signal::derive(move || Chart::parse(&body.get()));

    let rendered = Signal::derive(move || {
        let options = RenderOptions {
            source: source.get(),
            target: target.get(),
            capo: capo.get() as i16,
            layout: display.get().layout.to_core(),
        };

        chart.get().render(&options)
    });

    Effect::new(move |_| {
        if let Some(told) = on_respelled {
            told.run(rendered.get().respelled);
        }
    });

    view! {
        <Show
            when=move || chart.get().error.is_none()
            fallback=move || view! { <PlainFallback body chart /> }
        >
            <div
                class=move || {
                    if display.get().columns == 2 { "gap-8 md:columns-2" } else { "" }
                }
                style=move || format!("font-size: {}px", display.get().font_size)
                data-testid="chart"
            >
                // Rebuilt whole on every change rather than kept in a keyed list. A rendered
                // chart has no ids — the only key available is the position, and position is
                // exactly what does not change when a chart is transposed. Keying by it would
                // leave the old chords on screen in the new key. A chart is tens of lines; this
                // is not the place to be clever.
                {move || rendered
                    .get()
                    .sections
                    .into_iter()
                    .map(|section| view! { <SectionView section display /> })
                    .collect_view()}
            </div>
        </Show>
    }
}

#[component]
fn SectionView(
    section: aurum_core::chart::render::RenderedSection,
    display: Signal<Display>,
) -> impl IntoView {
    let kind = section.kind;
    let heading = match &section.label {
        Some(label) => format!("{} {label}", section_label(kind)),
        None => section_label(kind).to_owned(),
    };
    let lines = section.lines;

    view! {
        <section class="mb-5 break-inside-avoid">
            <Show when=move || display.get().show_sections && kind != SectionKind::None>
                <h3 class="mb-1 text-xs font-semibold uppercase tracking-wide text-ink-3">
                    {heading.clone()}
                </h3>
            </Show>

            {lines
                .into_iter()
                .map(|line| view! { <Line line display /> })
                .collect_view()}
        </section>
    }
}

#[component]
fn Line(line: RenderedLine, display: Signal<Display>) -> impl IntoView {
    if let Some(comment) = &line.comment {
        return view! { <p class="my-1 italic text-ink-3">{comment.clone()}</p> }.into_any();
    }

    let rows = line.over_lyrics_rows();
    let lyrics_only: String = line
        .segments
        .iter()
        .map(|segment| segment.lyric.as_str())
        .collect();
    let chords_only: Vec<String> = line
        .segments
        .iter()
        .filter_map(|segment| segment.chord.clone())
        .collect();
    let inline = line.clone();

    view! {
        {move || match display.get().mode {
            Mode::Lyrics => {
                let text = lyrics_only.clone();

                // A blank line is a blank line, not a collapsed one: the spacing is the phrasing.
                view! { <p class="whitespace-pre-wrap">{if text.is_empty() { " ".to_owned() } else { text }}</p> }
                    .into_any()
            }

            Mode::Chords if chords_only.is_empty() => ().into_any(),

            Mode::Chords => view! {
                <p class="font-mono font-semibold text-accent">
                    {chords_only.join("  ")}
                </p>
            }
            .into_any(),

            Mode::Both if display.get().layout == crate::prefs::display::Layout::Inline => {
                let segments = inline.segments.clone();

                view! {
                    <p class="whitespace-pre-wrap leading-8">
                        {segments
                            .into_iter()
                            .map(|segment| view! {
                                <span>
                                    {segment.chord.map(|chord| view! {
                                        <span class="font-mono font-semibold text-accent">
                                            {format!("[{chord}]")}
                                        </span>
                                    })}
                                    {segment.lyric}
                                </span>
                            })
                            .collect_view()}
                    </p>
                }
                .into_any()
            }

            // Chords over lyrics: two monospace rows whose columns line up by construction,
            // which is the same alignment the paste converter preserved on the way in.
            Mode::Both => {
                let (chords, lyrics) = (rows.chords.clone(), rows.lyrics.clone());

                view! {
                    <div class="font-mono leading-tight">
                        <div class="whitespace-pre font-semibold text-accent">
                            {if chords.is_empty() { " ".to_owned() } else { chords }}
                        </div>
                        <div class="whitespace-pre">
                            {if lyrics.is_empty() { " ".to_owned() } else { lyrics }}
                        </div>
                    </div>
                }
                .into_any()
            }
        }}
    }
    .into_any()
}

/// A chart that cannot be read is still a chart somebody has to play from tonight, so it falls
/// back to its own source text with the offending line named.
#[component]
fn PlainFallback(body: Signal<String>, chart: Signal<Chart>) -> impl IntoView {
    view! {
        <div>
            <p class="mb-3 rounded-md border border-warn/50 bg-warn/10 p-3 text-sm text-warn">
                {move || {
                    let chart = chart.get();
                    let error = chart.error.as_ref();

                    format!(
                        "This chart could not be read (line {}: {}) and is shown exactly as it \
                         was written.",
                        error.map(|error| error.line).unwrap_or_default(),
                        error.map(|error| error.message.as_str()).unwrap_or_default(),
                    )
                }}
            </p>
            <pre class="whitespace-pre-wrap font-mono text-sm">{move || body.get()}</pre>
        </div>
    }
}
