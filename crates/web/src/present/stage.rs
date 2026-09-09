//! The stage view.
//!
//! It is a subscriber to the same state as the audience, rendered by different rules: chords
//! stay, the next slide is visible, and blanking the audience never blanks the stage — the band
//! still needs the words while the congregation is looking at a logo.
//!
//! When the connection drops it keeps the last slide on screen with a stale badge. A stage view
//! that goes blank in front of a congregation is worse than one that is a few seconds behind.

use aurum_core::chart::notes::Key;
use aurum_core::present::session::{OutputKind, SessionState};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;
use serde::{Deserialize, Serialize};

use super::slide::StageSlide;
use super::transport::OutputTransport;
use crate::app::storage;

const PREFS_KEY: &str = "aurum.stage";
const KEY_PREF: &str = "aurum.stage.key";

/// How this reader wants their own screen. Per device on purpose: a tablet on a mic stand and a
/// laptop at the desk want different answers, and syncing would make each fight the other.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct StagePrefs {
    pub font_vh: f64,
    pub chords: bool,
    pub preview: bool,
    pub clock: bool,
    pub brightness: f64,
}

impl Default for StagePrefs {
    fn default() -> StagePrefs {
        StagePrefs {
            font_vh: 4.0,
            chords: true,
            preview: true,
            clock: true,
            brightness: 1.0,
        }
    }
}

fn elapsed(started_at: &str, now_ms: i64) -> String {
    let since = aurum_core::time::parse(started_at)
        .map(|at| ((now_ms - at) / 1000).max(0))
        .unwrap_or(0);

    format!("{}:{:02}", since / 60, since % 60)
}

/// A window on the control device: subscribes to the same channel as every other output.
#[component]
pub fn StagePage() -> impl IntoView {
    let params = use_query_map();
    let session_id = params.read_untracked().get("session").unwrap_or_default();
    let state = RwSignal::new(None::<SessionState>);

    let transport = StoredValue::new_local(OutputTransport::open(
        &session_id,
        OutputKind::Stage,
        "Stage window",
        Callback::new(move |next: SessionState| state.set(Some(next))),
    ));

    on_cleanup(move || {
        transport.with_value(|held| {
            if let Some(transport) = held {
                transport.close();
            }
        });
    });

    // A window the control surface opened closes itself when the session ends.
    Effect::new(move |_| {
        if state.get().is_some_and(|state| state.ended)
            && let Some(window) = web_sys::window()
        {
            let _ = window.close();
        }
    });

    view! {
        <StageScreen
            state=state.into()
            stale=Signal::derive(|| false)
            on_advance=Callback::new(move |delta: i64| {
                transport.with_value(|held| {
                    if let Some(transport) = held {
                        transport.request_advance(delta);
                    }
                });
            })
        />
    }
}

/// The screen itself, wherever its state came from — a channel on this device or a data channel
/// from the control device across the room.
#[component]
pub fn StageScreen(
    state: Signal<Option<SessionState>>,
    /// The connection has dropped and this is the last slide that arrived.
    stale: Signal<bool>,
    on_advance: Callback<i64>,
) -> impl IntoView {
    let prefs = RwSignal::new(
        storage::read(PREFS_KEY)
            .and_then(|held| serde_json::from_str(&held).ok())
            .unwrap_or_default(),
    );
    let settings = RwSignal::new(false);
    let now = RwSignal::new(crate::now_ms());

    // A tablet on a music stand must stay lit through a long song.
    crate::pwa::wake_lock::hold_while_open();

    // The clock is the operator's, not the browser's: it counts from when the session started.
    spawn_local(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(1000).await;

            if now.try_set(crate::now_ms()).is_none() {
                return;
            }
        }
    });

    let update = move |next: StagePrefs| {
        prefs.set(next);

        if let Ok(held) = serde_json::to_string(&next) {
            storage::write(PREFS_KEY, &held);
        }
    };

    // The reader's own key, which the set may override per slide (stage-view business rule 7).
    let key = storage::read(KEY_PREF).as_deref().and_then(Key::parse);

    let ended = Signal::derive(move || state.get().is_some_and(|state| state.ended));
    let current =
        Signal::derive(move || state.get().and_then(|state| state.stage_slide().cloned()));
    let next = Signal::derive(move || state.get().and_then(|state| state.next_slide().cloned()));

    view! {
        <Show
            when=move || !ended.get()
            fallback=|| view! {
                <div class="flex h-dvh w-dvw items-center justify-center bg-black text-slate-400">
                    <p>"The session has ended."</p>
                </div>
            }
        >
            <div
                class=move || if stale.get() {
                    "flex h-dvh w-dvw flex-col bg-black text-slate-100 ring-4 ring-amber-500"
                } else {
                    "flex h-dvh w-dvw flex-col bg-black text-slate-100"
                }
                data-testid="stage"
                style=move || format!("filter: brightness({})", prefs.get().brightness)
            >
                <header class="flex items-center gap-3 border-b border-slate-800 px-3 py-2 text-sm">
                    <span class="font-medium" data-testid="stage-title">
                        {move || current
                            .get()
                            .map(|slide| slide.song_title)
                            .or_else(|| {
                                state.get().map(|state| state.set_snapshot.set_name)
                            })
                            .unwrap_or_else(|| "Stage".to_owned())}
                    </span>

                    {move || current.get().and_then(|slide| slide.label).map(|label| view! {
                        <span class="opacity-70">{label}</span>
                    })}

                    // The last slide stays on screen while it reconnects: a stage view that goes
                    // blank in front of a congregation is worse than one a few seconds behind.
                    <Show when=move || stale.get()>
                        <span
                            class="rounded-full bg-amber-500 px-2 text-xs text-black"
                            data-testid="stage-stale"
                        >
                            "reconnecting"
                        </span>
                    </Show>

                    <span class="ml-auto flex items-center gap-3 opacity-70">
                        {move || state.get().map(|state| view! {
                            <span data-testid="stage-position">
                                {format!("{} / {}", state.index + 1, state.slides.len())}
                            </span>
                        })}

                        <Show when=move || prefs.get().clock>
                            <span>
                                {move || state
                                    .get()
                                    .map(|state| elapsed(&state.started_at, now.get()))
                                    .unwrap_or_default()}
                            </span>
                        </Show>

                        <button
                            class="underline"
                            on:click=move |_| settings.update(|open| *open = !*open)
                        >
                            "layout"
                        </button>
                    </span>
                </header>

                {move || state.get().and_then(|state| state.stage_message).map(|message| view! {
                    <p
                        class="bg-amber-500 px-3 py-2 text-lg font-medium text-black"
                        data-testid="stage-message"
                    >
                        {message}
                    </p>
                })}

                <main class="grid flex-1 gap-4 overflow-hidden p-4 md:grid-cols-[3fr_2fr]">
                    <section class="overflow-auto">
                        <StageSlide
                            slide=current
                            target_key=Signal::derive(move || key)
                            show_chords=Signal::derive(move || prefs.get().chords)
                            scale=Signal::derive(move || prefs.get().font_vh)
                        />
                    </section>

                    <Show when=move || prefs.get().preview>
                        <section class="overflow-auto border-l border-slate-800 pl-4 opacity-60">
                            <p class="mb-2 text-xs uppercase tracking-widest">"Next"</p>
                            <StageSlide
                                slide=next
                                target_key=Signal::derive(move || key)
                                show_chords=Signal::derive(move || prefs.get().chords)
                                scale=Signal::derive(move || prefs.get().font_vh * 0.7)
                                empty="End of the set."
                            />
                        </section>
                    </Show>
                </main>

                <footer class="flex items-center gap-3 border-t border-slate-800 px-3 py-2 text-sm opacity-70">
                    <span>
                        {move || state
                            .get()
                            .map(|state| state.set_snapshot.set_name)
                            .unwrap_or_default()}
                    </span>

                    <span class="ml-auto">
                        {move || match (current.get(), next.get()) {
                            (Some(now), Some(after)) if now.song_title != after.song_title => {
                                format!("Next: {}", after.song_title)
                            }
                            _ => String::new(),
                        }}
                    </span>

                    // Only ever honoured when the control surface has granted this device the
                    // advance (stage-view business rule 8); it is always safe to ask.
                    <span class="flex gap-2">
                        <button
                            class="rounded border border-slate-700 px-2"
                            data-testid="stage-back"
                            on:click=move |_| on_advance.run(-1)
                        >
                            "←"
                        </button>
                        <button
                            class="rounded border border-slate-700 px-2"
                            data-testid="stage-forward"
                            on:click=move |_| on_advance.run(1)
                        >
                            "→"
                        </button>
                    </span>
                </footer>

                <Show when=move || settings.get()>
                    <div class="absolute inset-x-0 bottom-0 border-t border-slate-700 bg-slate-900 p-4 text-sm">
                        <div class="flex flex-wrap items-center gap-4">
                            <label class="flex items-center gap-2">
                                "Size"
                                <input
                                    type="range"
                                    min="2"
                                    max="10"
                                    step="0.5"
                                    prop:value=move || prefs.get().font_vh
                                    on:input=move |event| {
                                        update(StagePrefs {
                                            font_vh: event_target_value(&event)
                                                .parse()
                                                .unwrap_or(4.0),
                                            ..prefs.get_untracked()
                                        });
                                    }
                                />
                            </label>

                            <label class="flex items-center gap-2">
                                "Brightness"
                                <input
                                    type="range"
                                    min="0.3"
                                    max="1"
                                    step="0.05"
                                    prop:value=move || prefs.get().brightness
                                    on:input=move |event| {
                                        update(StagePrefs {
                                            brightness: event_target_value(&event)
                                                .parse()
                                                .unwrap_or(1.0),
                                            ..prefs.get_untracked()
                                        });
                                    }
                                />
                            </label>

                            <label class="flex items-center gap-2">
                                <input
                                    type="checkbox"
                                    prop:checked=move || prefs.get().chords
                                    on:change=move |event| {
                                        update(StagePrefs {
                                            chords: event_target_checked(&event),
                                            ..prefs.get_untracked()
                                        });
                                    }
                                />
                                "Chords"
                            </label>

                            <label class="flex items-center gap-2">
                                <input
                                    type="checkbox"
                                    prop:checked=move || prefs.get().preview
                                    on:change=move |event| {
                                        update(StagePrefs {
                                            preview: event_target_checked(&event),
                                            ..prefs.get_untracked()
                                        });
                                    }
                                />
                                "Next preview"
                            </label>

                            <label class="flex items-center gap-2">
                                <input
                                    type="checkbox"
                                    prop:checked=move || prefs.get().clock
                                    on:change=move |event| {
                                        update(StagePrefs {
                                            clock: event_target_checked(&event),
                                            ..prefs.get_untracked()
                                        });
                                    }
                                />
                                "Clock"
                            </label>

                            <button
                                class="ml-auto underline"
                                on:click=move |_| settings.set(false)
                            >
                                "done"
                            </button>
                        </div>
                    </div>
                </Show>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_counts_from_the_start_of_the_session() {
        let started = "2026-09-09T10:00:00.000Z";
        let at = aurum_core::time::parse(started).expect("a timestamp");

        assert_eq!(elapsed(started, at), "0:00");
        assert_eq!(elapsed(started, at + 61_000), "1:01");
        assert_eq!(elapsed(started, at + 3_723_000), "62:03");
    }

    /// A clock that runs backwards is worse than one that has not started.
    #[test]
    fn a_clock_never_goes_negative() {
        let started = "2026-09-09T10:00:00.000Z";
        let at = aurum_core::time::parse(started).expect("a timestamp");

        assert_eq!(elapsed(started, at - 5_000), "0:00");
    }
}
