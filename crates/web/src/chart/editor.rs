//! The editing surface: ChordPro text on the left, the live preview on the right, and a gutter
//! naming tokens that could not be read as chords.
//!
//! The gutter never blocks a save. A musician typing a chart ten minutes before a service is not
//! going to be stopped by a validator that thinks `[Hmm]` is a mistake.

use aurum_core::chart::chordpro::Chart;
use aurum_core::chart::over_lyrics::{Notation, detect_notation, to_chord_pro};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use web_sys::HtmlTextAreaElement;

/// A chart is saved eight tenths of a second after the typing stops: long enough not to write on
/// every keystroke, short enough that closing the tab does not lose the verse just typed.
const AUTOSAVE_MS: u32 = 800;

/// Shown in an empty editor: the three things a chart needs, in the order they are written.
const PLACEHOLDER: &str = "{title: Song}\n{verse: 1}\n[G]Type or paste a chart…";

/// What a save carries. `source_text` is the pre-conversion paste, kept for exactly one undo
/// (business rule 1) and cleared the moment that undo is taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveInput {
    pub body: String,
    pub source_notation: Notation,
    pub source_text: Option<String>,
}

/// Where a paste is going, so the converted text lands where the caret was rather than at the end.
fn split_at_caret(area: &HtmlTextAreaElement) -> (String, String) {
    let value = area.value();
    let units: Vec<u16> = value.encode_utf16().collect();
    let start = (area.selection_start().ok().flatten().unwrap_or(0) as usize).min(units.len());
    let end = (area.selection_end().ok().flatten().unwrap_or(0) as usize).min(units.len());
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };

    (
        String::from_utf16_lossy(&units[..start]),
        String::from_utf16_lossy(&units[end..]),
    )
}

#[component]
pub fn ChartEditor(
    body: Signal<String>,
    on_save: Callback<SaveInput>,
    /// The preview pane, passed in so the editor does not have to know how a chart is rendered
    /// or in which key this reader wants it.
    preview: Callback<String, AnyView>,
) -> impl IntoView {
    let draft = RwSignal::new(body.get_untracked());
    let saved = RwSignal::new(true);

    // Undo holds the whole pre-conversion text, not a diff: one step back to exactly what was
    // pasted is the entire promise, and anything cleverer would have to survive further typing.
    let undo_to = RwSignal::new(None::<String>);
    let asking = RwSignal::new(None::<String>);

    let notation = StoredValue::new(Notation::ChordPro);
    let source_text = StoredValue::new(None::<String>);
    // Each keystroke cancels the pending write by raising the generation the timer checks.
    let generation = StoredValue::new(0_u64);

    let warnings = Signal::derive(move || Chart::parse(&draft.get()).warnings);
    // Eight is as many as a gutter can show without becoming the page. Hoisted out of the view
    // because a turbofish inside an attribute reads as an opening tag to the macro.
    let shown = Signal::derive(move || warnings.get().into_iter().take(8).collect::<Vec<_>>());

    // Every read here is a `try_`: an autosave can land after the screen it belongs to is gone,
    // and losing the last two words is better than a panic that takes the whole app down.
    let commit = move || {
        let (Some(body), Some(source_notation), Some(source_text)) = (
            draft.try_get_untracked(),
            notation.try_get_value(),
            source_text.try_get_value(),
        ) else {
            return;
        };

        on_save.run(SaveInput {
            body,
            source_notation,
            source_text,
        });
        let _ = saved.try_set(true);
    };

    // Navigating away in the eight tenths of a second after a keystroke must not lose it.
    on_cleanup(move || {
        if saved.try_get_untracked() == Some(false) {
            commit();
        }
    });

    let touched = move || {
        let _ = saved.try_set(false);
        generation.update_value(|value| *value += 1);
        let mine = generation.get_value();

        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(AUTOSAVE_MS).await;

            // Typing since this timer started means a later one owns the save.
            if generation.try_get_value() == Some(mine) {
                commit();
            }
        });
    };

    let accept = move |text: String, before: String, after: String| {
        notation.set_value(Notation::OverLyrics);
        source_text.set_value(Some(text.clone()));
        undo_to.set(Some(format!("{before}{text}{after}")));
        draft.set(format!("{before}{}{after}", to_chord_pro(&text)));
        touched();
    };

    let on_paste = move |event: web_sys::ClipboardEvent| {
        let Some(text) = event
            .clipboard_data()
            .and_then(|data| data.get_data("text/plain").ok())
        else {
            return;
        };

        if text.trim().is_empty() {
            return;
        }

        match detect_notation(&text) {
            // Already ChordPro: the browser's own paste does the right thing.
            Notation::ChordPro => {}

            // Business rule 2 can only decide when there is a chord line to see. With none, the
            // user knows what they pasted and we do not.
            Notation::Ambiguous => {
                event.prevent_default();
                asking.set(Some(text));
            }

            Notation::OverLyrics => {
                event.prevent_default();

                let Some(area) = event
                    .target()
                    .and_then(|target| target.dyn_into::<HtmlTextAreaElement>().ok())
                else {
                    return;
                };

                let (before, after) = split_at_caret(&area);
                accept(text, before, after);
            }
        }
    };

    view! {
        <div class="grid gap-4 lg:grid-cols-2">
            <div>
                <Show when=move || undo_to.get().is_some()>
                    <div
                        class="mb-2 flex items-center gap-3 rounded-md border border-accent/60 bg-accent/10 px-3 py-2 text-sm text-accent"
                        data-testid="converted"
                    >
                        <span>"Converted from chords over lyrics."</span>
                        <button
                            class="text-ink-3 hover:text-ink underline-offset-2 hover:underline"
                            data-testid="undo-conversion"
                            on:click=move |_| {
                                if let Some(original) = undo_to.get_untracked() {
                                    notation.set_value(Notation::ChordPro);
                                    source_text.set_value(None);
                                    undo_to.set(None);
                                    draft.set(original);
                                    touched();
                                }
                            }
                        >
                            "Undo"
                        </button>
                    </div>
                </Show>

                <textarea
                    class="h-[60vh] w-full rounded-md border border-line-strong bg-surface p-3 font-mono text-sm leading-relaxed"
                    data-testid="chart-editor"
                    spellcheck="false"
                    placeholder=PLACEHOLDER
                    prop:value=move || draft.get()
                    on:input=move |event| {
                        draft.set(event_target_value(&event));
                        touched();
                    }
                    on:paste=on_paste
                    on:blur=move |_| {
                        if !saved.get_untracked() {
                            generation.update_value(|value| *value += 1);
                            commit();
                        }
                    }
                />

                <div class="mt-2 flex items-start gap-4 text-xs text-ink-3">
                    <span data-testid="save-state">
                        {move || if saved.get() { "Saved" } else { "Saving…" }}
                    </span>

                    <ul class="space-y-0.5" data-testid="chart-warnings">
                        <For each=move || shown.get() key=|warning| (warning.line, warning.token.clone()) let:warning>
                            <li class="text-warn">
                                {format!(
                                    "Line {}: “{}” is not a chord — it will be shown as written.",
                                    warning.line,
                                    warning.token,
                                )}
                            </li>
                        </For>
                    </ul>
                </div>
            </div>

            <div class="rounded-md border border-line p-3">
                {move || preview.run(draft.get())}
            </div>

            <Show when=move || asking.get().is_some()>
                <NotationPrompt
                    on_over_lyrics=Callback::new(move |text: String| {
                        let current = draft.get_untracked();

                        accept(text, current, String::new());
                        asking.set(None);
                    })
                    on_chord_pro=Callback::new(move |text: String| {
                        draft.update(|value| value.push_str(&text));
                        touched();
                        asking.set(None);
                    })
                    on_cancel=Callback::new(move |()| asking.set(None))
                    text=Signal::derive(move || asking.get().unwrap_or_default())
                />
            </Show>
        </div>
    }
}

/// The one question the converter cannot answer for itself.
#[component]
fn NotationPrompt(
    text: Signal<String>,
    on_over_lyrics: Callback<String>,
    on_chord_pro: Callback<String>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    view! {
        <div
            class="fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-6"
            data-testid="notation-prompt"
        >
            <div class="w-96 rounded-md bg-surface p-4 shadow-lg">
                <h2 class="mb-2 font-semibold">"Which notation is this?"</h2>
                <p class="mb-4 text-sm text-ink-3">
                    "No chord line was recognised, so the format cannot be told from the text alone."
                </p>
                <div class="flex gap-2">
                    <button
                        class="rounded-md bg-accent px-3 py-2 text-sm text-on-accent"
                        data-testid="paste-over-lyrics"
                        on:click=move |_| on_over_lyrics.run(text.get_untracked())
                    >
                        "Chords over lyrics"
                    </button>
                    <button
                        class="rounded-md border border-line-strong px-3 py-2 text-sm"
                        data-testid="paste-chordpro"
                        on:click=move |_| on_chord_pro.run(text.get_untracked())
                    >
                        "ChordPro"
                    </button>
                    <button
                        class="ml-auto text-sm text-ink-3 hover:text-ink underline-offset-2 hover:underline"
                        on:click=move |_| on_cancel.run(())
                    >
                        "Cancel"
                    </button>
                </div>
            </div>
        </div>
    }
}
