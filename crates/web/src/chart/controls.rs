//! The header of a chart: which arrangement, which key, which capo, how it is laid out.
//!
//! The key control says where the key came from, because there are four places it can come from
//! (business rule 7) and "why is this in F?" is otherwise unanswerable from the screen.

use aurum_core::chart::effective_key::KeySource;
use aurum_core::chart::notes::Key;
use aurum_core::chart::render::shape_key_of;
use leptos::prelude::*;

use crate::prefs::display::{Display, Layout, Mode};

fn source_label(source: KeySource) -> &'static str {
    match source {
        KeySource::Set => "from this set",
        KeySource::Preference => "your preferred key",
        KeySource::Arrangement => "arrangement default",
        KeySource::Song => "original key",
        KeySource::None => "no key set",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrangementChoice {
    pub id: String,
    pub name: String,
}

/// One button of a segmented control. Segments rather than a `<select>`: a player mid-set should
/// not have to open a menu, and on a stand the choice has to be readable without being tapped.
fn segment(chosen: bool) -> &'static str {
    if chosen {
        "h-full border-line-strong px-3 text-sm font-semibold bg-raised text-ink not-first:border-l"
    } else {
        "h-full border-line-strong px-3 text-sm text-ink-3 hover:text-ink not-first:border-l"
    }
}

#[component]
#[allow(clippy::too_many_arguments)]
pub fn ChartControls(
    arrangements: Signal<Vec<ArrangementChoice>>,
    arrangement_id: Signal<Option<String>>,
    on_arrangement: Callback<String>,
    /// The key being read in, and where it came from.
    target: Signal<Option<Key>>,
    source: Signal<KeySource>,
    /// The key the chart is written in, which the twelve choices are measured from.
    original: Signal<Option<Key>>,
    on_key: Callback<Option<Key>>,
    capo: Signal<i64>,
    on_capo: Callback<i64>,
    display: Signal<Display>,
    on_display: Callback<Display>,
    /// Business rule 14: the target key would have needed a double accidental.
    respelled: Signal<bool>,
    can_edit: bool,
    editing: Signal<bool>,
    on_toggle_edit: Callback<()>,
) -> impl IntoView {
    let open = RwSignal::new(false);

    let choices = Signal::derive(move || match original.get() {
        None => Vec::new(),
        Some(original) => (0..12)
            .map(|semitones| original.transposed(semitones))
            .collect(),
    });

    view! {
        <div class="flex flex-wrap items-center gap-2 border-b border-line pb-3">
            <Show when=move || { arrangements.get().len() > 1 }>
                <select
                    class="rounded-md border border-line-strong bg-transparent px-2 py-1 text-sm"
                    data-testid="arrangement-picker"
                    prop:value=move || arrangement_id.get().unwrap_or_default()
                    on:change=move |event| on_arrangement.run(event_target_value(&event))
                >
                    <For each=move || arrangements.get() key=|choice| choice.id.clone() let:choice>
                        <option value=choice.id.clone()>{choice.name.clone()}</option>
                    </For>
                </select>
            </Show>

            <div class="relative">
                <button
                    class=move || format!(
                        "flex h-9 items-center gap-3 rounded-lg border px-3 text-left {}",
                        if target.get().is_some() {
                            "border-accent/40 bg-accent/10"
                        } else {
                            "border-line-strong"
                        },
                    )
                    data-testid="key-button"
                    on:click=move |_| open.update(|value| *value = !*value)
                >
                    <span class="flex items-baseline gap-1.5">
                        <span class="text-xs text-ink-3">"Key of"</span>
                        <span class="font-mono text-base font-bold text-accent">
                            {move || match target.get() {
                                None => "—".to_owned(),
                                Some(key) => key.to_string(),
                            }}
                        </span>
                    </span>

                    // Key, capo and the shapes they produce are one decision, so they are drawn
                    // as one object rather than a setting and a footnote.
                    <Show when=move || { capo.get() > 0 && target.get().is_some() }>
                        <span class="border-l border-accent/30 pl-3 leading-tight">
                            <span class="block text-xs font-semibold text-accent">
                                {move || format!("Capo {}", capo.get())}
                            </span>
                            <span class="block text-xs text-ink-3">
                                {move || {
                                    let key = target.get().expect("a key");

                                    format!("shapes in {}", shape_key_of(&key, capo.get() as i16))
                                }}
                            </span>
                        </span>
                    </Show>
                </button>

                <Show when=move || open.get()>
                    <div class="absolute z-10 mt-1 w-64 rounded-md border border-line-strong bg-surface p-3 text-sm shadow">
                        <p class="mb-2 text-xs text-ink-3" data-testid="key-source">
                            {move || source_label(source.get())}
                        </p>

                        <div class="mb-2 grid grid-cols-4 gap-1">
                            <For
                                each=move || choices.get()
                                key=|key| key.to_string()
                                let:choice
                            >
                                <button
                                    class="rounded-md border border-line px-1 py-1 text-xs"
                                    on:click=move |_| {
                                        on_key.run(Some(choice));
                                        open.set(false);
                                    }
                                >
                                    {choice.to_string()}
                                </button>
                            </For>
                        </div>

                        <button
                            class="mb-2 text-xs text-ink-3 underline-offset-2 hover:underline"
                            on:click=move |_| {
                                on_key.run(None);
                                open.set(false);
                            }
                        >
                            "Back to the written key"
                        </button>

                        <label class="mt-2 block text-xs text-ink-3">
                            "Capo"
                            <input
                                class="ml-2 w-16 rounded-md border border-line-strong px-1"
                                data-testid="capo"
                                type="number"
                                min="0"
                                max="11"
                                prop:value=move || capo.get()
                                on:change=move |event| {
                                    on_capo.run(
                                        event_target_value(&event).parse().unwrap_or(0).clamp(0, 11),
                                    );
                                }
                            />
                        </label>
                    </div>
                </Show>
            </div>

            <Show when=move || respelled.get()>
                <span
                    class="rounded-md bg-warn/15 px-2 py-1 text-xs text-warn"
                    title="This key would need a double accidental, so a simpler spelling is shown."
                    data-testid="respelled"
                >
                    "respelled"
                </span>
            </Show>

            <div
                class="flex h-9 overflow-hidden rounded-lg border border-line-strong"
                data-testid="layout"
                role="group"
                aria-label="Chart layout"
            >
                {[
                    (Layout::Over, "Chords over lyrics"),
                    (Layout::Inline, "Inline"),
                    (Layout::Nashville, "Nashville"),
                ]
                    .into_iter()
                    .map(|(choice, label)| view! {
                        <button
                            class=move || segment(display.get().layout == choice)
                            aria-pressed=move || (display.get().layout == choice).to_string()
                            on:click=move |_| {
                                on_display.run(Display { layout: choice, ..display.get() });
                            }
                        >
                            {label}
                        </button>
                    })
                    .collect_view()}
            </div>

            <div
                class="flex h-9 overflow-hidden rounded-lg border border-line-strong"
                data-testid="mode"
                role="group"
                aria-label="What to show"
            >
                {[
                    (Mode::Both, "Chords and lyrics"),
                    (Mode::Chords, "Chords only"),
                    (Mode::Lyrics, "Lyrics only"),
                ]
                    .into_iter()
                    .map(|(choice, label)| view! {
                        <button
                            class=move || segment(display.get().mode == choice)
                            aria-pressed=move || (display.get().mode == choice).to_string()
                            on:click=move |_| {
                                on_display.run(Display { mode: choice, ..display.get() });
                            }
                        >
                            {label}
                        </button>
                    })
                    .collect_view()}
            </div>

            <div class="flex h-9 items-center overflow-hidden rounded-lg border border-line-strong text-sm">
                <button
                    class="h-full px-3 text-ink-2 hover:bg-raised"
                    title="Smaller"
                    on:click=move |_| {
                        on_display.run(Display {
                            font_size: (display.get().font_size - 2).max(10),
                            ..display.get()
                        });
                    }
                >
                    "A-"
                </button>
                <span class="min-w-9 border-x border-line-strong px-1 text-center font-mono text-xs text-ink-3">
                    {move || display.get().font_size.to_string()}
                </span>
                <button
                    class="h-full px-3 text-ink-2 hover:bg-raised"
                    title="Larger"
                    on:click=move |_| {
                        on_display.run(Display {
                            font_size: (display.get().font_size + 2).min(48),
                            ..display.get()
                        });
                    }
                >
                    "A+"
                </button>
            </div>

            <Show when=move || can_edit>
                <button
                    class="ml-auto rounded-md bg-accent px-3 py-1 text-sm text-on-accent"
                    data-testid="edit-toggle"
                    on:click=move |_| on_toggle_edit.run(())
                >
                    {move || if editing.get() { "Done" } else { "Edit" }}
                </button>
            </Show>
        </div>
    }
}
