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
        <div class="flex flex-wrap items-center gap-2 border-b border-slate-200 pb-3 dark:border-slate-800">
            <Show when=move || { arrangements.get().len() > 1 }>
                <select
                    class="rounded border border-slate-300 bg-transparent px-2 py-1 text-sm dark:border-slate-700"
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
                    class="rounded border border-slate-300 px-3 py-1 text-sm dark:border-slate-700"
                    data-testid="key-button"
                    on:click=move |_| open.update(|value| *value = !*value)
                >
                    {move || match target.get() {
                        None => "Set key".to_owned(),
                        Some(key) => format!("Key of {key}"),
                    }}

                    <Show when=move || { capo.get() > 0 && target.get().is_some() }>
                        <span class="ml-2 text-slate-500">
                            {move || {
                                let key = target.get().expect("a key");

                                format!(
                                    "Capo {} — shapes in {}",
                                    capo.get(),
                                    shape_key_of(&key, capo.get() as i16),
                                )
                            }}
                        </span>
                    </Show>
                </button>

                <Show when=move || open.get()>
                    <div class="absolute z-10 mt-1 w-64 rounded border border-slate-300 bg-white p-3 text-sm shadow dark:border-slate-700 dark:bg-slate-900">
                        <p class="mb-2 text-xs text-slate-500" data-testid="key-source">
                            {move || source_label(source.get())}
                        </p>

                        <div class="mb-2 grid grid-cols-4 gap-1">
                            <For
                                each=move || choices.get()
                                key=|key| key.to_string()
                                let:choice
                            >
                                <button
                                    class="rounded border border-slate-200 px-1 py-1 text-xs dark:border-slate-700"
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
                            class="mb-2 text-xs underline text-slate-500"
                            on:click=move |_| {
                                on_key.run(None);
                                open.set(false);
                            }
                        >
                            "Back to the written key"
                        </button>

                        <label class="mt-2 block text-xs text-slate-500">
                            "Capo"
                            <input
                                class="ml-2 w-16 rounded border border-slate-300 px-1 dark:border-slate-700"
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
                    class="rounded bg-amber-100 px-2 py-1 text-xs text-amber-900"
                    title="This key would need a double accidental, so a simpler spelling is shown."
                    data-testid="respelled"
                >
                    "respelled"
                </span>
            </Show>

            <select
                class="rounded border border-slate-300 bg-transparent px-2 py-1 text-sm dark:border-slate-700"
                data-testid="layout"
                prop:value=move || display.get().layout.as_str()
                on:change=move |event| {
                    on_display.run(Display {
                        layout: Layout::parse(&event_target_value(&event)),
                        ..display.get()
                    });
                }
            >
                <option value="over">"Chords over lyrics"</option>
                <option value="inline">"Inline"</option>
                <option value="nashville">"Nashville"</option>
            </select>

            <select
                class="rounded border border-slate-300 bg-transparent px-2 py-1 text-sm dark:border-slate-700"
                data-testid="mode"
                prop:value=move || display.get().mode.as_str()
                on:change=move |event| {
                    on_display.run(Display {
                        mode: Mode::parse(&event_target_value(&event)),
                        ..display.get()
                    });
                }
            >
                <option value="both">"Chords and lyrics"</option>
                <option value="chords">"Chords only"</option>
                <option value="lyrics">"Lyrics only"</option>
            </select>

            <div class="flex items-center gap-1 text-sm">
                <button
                    class="rounded border border-slate-300 px-2 dark:border-slate-700"
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
                <button
                    class="rounded border border-slate-300 px-2 dark:border-slate-700"
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
                    class="ml-auto rounded bg-slate-900 px-3 py-1 text-sm text-white"
                    data-testid="edit-toggle"
                    on:click=move |_| on_toggle_edit.run(())
                >
                    {move || if editing.get() { "Done" } else { "Edit" }}
                </button>
            </Show>
        </div>
    }
}
