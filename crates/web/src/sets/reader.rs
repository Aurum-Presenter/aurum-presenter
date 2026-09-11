//! Reader mode: one item per screen, for a musician holding a phone on a mic stand.
//!
//! Arrow keys on a desktop, swipe on a phone, and a wake lock for as long as the set is open —
//! a screen that sleeps between verses is the single most annoying thing a stage app can do.

use aurum_core::chart::notes::Key;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};
use wasm_bindgen::prelude::*;
use web_sys::{KeyboardEvent, TouchEvent};

use super::resolved::{ResolvedItem, use_resolved_set};
use crate::chart::view::ChartView;
use crate::prefs::display::{self, Display};

/// How far a thumb has to travel before it counts as a page turn rather than a scroll.
const SWIPE_PX: f64 = 60.0;

#[component]
pub fn ReaderPage() -> impl IntoView {
    let params = use_params_map();
    let navigate = StoredValue::new(use_navigate());
    let set_id = Signal::derive(move || params.read().get("set_id").unwrap_or_default());
    let resolved = use_resolved_set(set_id);
    let display = RwSignal::new(display::load());
    let jumping = RwSignal::new(false);

    let items = Signal::derive(move || resolved.get().items);
    let count = Signal::derive(move || items.get().len());

    // The position is clamped rather than trusted: a set that shrank while somebody was reading
    // it must not leave them on a page that no longer exists.
    let position = Signal::derive(move || {
        let asked: usize = params
            .read()
            .get("index")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);

        asked.min(count.get().saturating_sub(1))
    });

    let go_to = move |index: usize| {
        navigate.get_value()(
            &format!("/sets/{}/read/{index}", set_id.get_untracked()),
            Default::default(),
        );
    };

    let step = move |delta: i64| {
        let next = position.get_untracked() as i64 + delta;

        if next >= 0 && (next as usize) < count.get_untracked() {
            go_to(next as usize);
        }
    };

    // Keys are read on the window, not on a focused element: the reader has both hands on an
    // instrument and is hitting a pedal or a spacebar, not tabbing to a button first.
    {
        let Some(window) = web_sys::window() else {
            return ().into_any();
        };

        let listener =
            Closure::<dyn Fn(KeyboardEvent)>::new(move |event: KeyboardEvent| {
                match event.key().as_str() {
                    "ArrowRight" | "PageDown" | " " => {
                        event.prevent_default();
                        step(1);
                    }
                    "ArrowLeft" | "PageUp" => {
                        event.prevent_default();
                        step(-1);
                    }
                    "Escape" => {
                        navigate.get_value()(
                            &format!("/sets/{}", set_id.get_untracked()),
                            Default::default(),
                        );
                    }
                    _ => {}
                }
            });

        let _ =
            window.add_event_listener_with_callback("keydown", listener.as_ref().unchecked_ref());

        let held = StoredValue::new_local(Some(listener));

        // Removed on the way out, or every set opened in this tab would still be listening.
        on_cleanup(move || {
            let Some(window) = web_sys::window() else {
                return;
            };

            held.with_value(|listener| {
                if let Some(listener) = listener {
                    let _ = window.remove_event_listener_with_callback(
                        "keydown",
                        listener.as_ref().unchecked_ref(),
                    );
                }
            });
        });
    }

    crate::pwa::wake_lock::hold_while_open();

    let touch_start = StoredValue::new(None::<f64>);

    let current = Signal::derive(move || items.get().get(position.get()).cloned());
    let name = Signal::derive(move || resolved.get().set.map(|set| set.name).unwrap_or_default());

    view! {
        <Show
            when=move || current.get().is_some()
            fallback=|| view! { <p class="p-6 text-sm text-ink-3">"Loading…"</p> }
        >
            <div
                class="mx-auto max-w-4xl p-4"
                on:touchstart=move |event: TouchEvent| {
                    touch_start.set_value(
                        event.touches().get(0).map(|touch| touch.client_x() as f64),
                    );
                }
                on:touchend=move |event: TouchEvent| {
                    let (Some(from), Some(touch)) =
                        (touch_start.get_value(), event.changed_touches().get(0))
                    else {
                        return;
                    };

                    let travelled = touch.client_x() as f64 - from;

                    touch_start.set_value(None);

                    if travelled.abs() > SWIPE_PX {
                        step(if travelled < 0.0 { 1 } else { -1 });
                    }
                }
            >
                <header class="mb-3 flex items-center gap-3 border-b border-line pb-2">
                    <A
                        href=move || format!("/sets/{}", set_id.get())
                        attr:class="text-sm text-ink-3 hover:text-ink underline-offset-2 hover:underline"
                    >
                        {move || format!("← {}", name.get())}
                    </A>

                    <button
                        class="text-sm text-ink-3 hover:text-ink underline-offset-2 hover:underline"
                        data-testid="reader-position"
                        on:click=move |_| jumping.set(true)
                    >
                        {move || format!("{} / {}", position.get() + 1, count.get())}
                    </button>

                    <span class="ml-auto flex gap-2 text-sm">
                        <button
                            class="rounded-md border border-line-strong px-3"
                            data-testid="reader-back"
                            prop:disabled=move || position.get() == 0
                            on:click=move |_| step(-1)
                        >
                            "←"
                        </button>
                        <button
                            class="rounded-md border border-line-strong px-3"
                            data-testid="reader-forward"
                            prop:disabled=move || position.get() + 1 >= count.get()
                            on:click=move |_| step(1)
                        >
                            "→"
                        </button>
                    </span>
                </header>

                {move || current.get().map(|resolved| view! {
                    <ReaderItem resolved display=display.into() />
                })}

                <Show when=move || jumping.get()>
                    <div
                        class="fixed inset-0 z-20 flex items-end bg-black/60"
                        on:click=move |_| jumping.set(false)
                    >
                        <ul
                            class="max-h-[70vh] w-full overflow-auto rounded-t bg-surface p-2"
                            on:click=|event| event.stop_propagation()
                        >
                            {move || items
                                .get()
                                .into_iter()
                                .enumerate()
                                .map(|(index, item)| view! {
                                    <li>
                                        <button
                                            class=move || if index == position.get() {
                                                "flex w-full gap-2 px-2 py-2 text-left font-semibold"
                                            } else {
                                                "flex w-full gap-2 px-2 py-2 text-left"
                                            }
                                            on:click=move |_| {
                                                go_to(index);
                                                jumping.set(false);
                                            }
                                        >
                                            <span class="w-6 text-ink-4">{index + 1}</span>
                                            <span>{item.title.clone()}</span>
                                            {item.key.map(|key| view! {
                                                <span class="ml-auto text-ink-3">
                                                    {key.to_string()}
                                                </span>
                                            })}
                                        </button>
                                    </li>
                                })
                                .collect_view()}
                        </ul>
                    </div>
                </Show>
            </div>
        </Show>
    }
    .into_any()
}

/// One item, as a musician reads it. Shared with the print pack, which is the same thing on
/// paper.
#[component]
pub fn ReaderItem(resolved: ResolvedItem, display: Signal<Display>) -> impl IntoView {
    let fallback = Key::parse("C").expect("C is a key");
    let written = resolved.written.unwrap_or(fallback);
    let target = resolved.key.unwrap_or(written);
    let capo = resolved.capo;
    let body = resolved.arrangement.as_ref().map(|row| row.body.clone());
    let has_chart = body.as_deref().is_some_and(|body| !body.trim().is_empty());

    view! {
        <article>
            <h2 class="text-xl font-semibold">
                {resolved.title.clone()}
                {resolved.key.map(|key| view! {
                    <span class="ml-3 text-base font-normal text-ink-3">{key.to_string()}</span>
                })}
                {(capo > 0).then(|| view! {
                    <span class="ml-2 text-base font-normal text-ink-3">
                        {format!("capo {capo}")}
                    </span>
                })}
            </h2>

            {resolved.item.note.clone().map(|note| view! {
                <p class="mb-2 rounded-md bg-warn/10 px-2 py-1 text-sm text-warn">{note}</p>
            })}

            {resolved.missing.then(|| view! {
                <p class="mb-2 text-sm text-warn">
                    "This song has been deleted from the library. The set keeps its title so the \
                     running order still reads."
                </p>
            })}

            {resolved.item.content.clone().map(|content| view! {
                <p class="whitespace-pre-wrap py-4 text-lg">{content}</p>
            })}

            {match (resolved.arrangement.is_some(), has_chart) {
                (true, true) => {
                    let body = body.unwrap_or_default();

                    view! {
                        <ChartView
                            body=Signal::derive(move || body.clone())
                            source=Signal::derive(move || written)
                            target=Signal::derive(move || target)
                            capo=Signal::derive(move || capo)
                            display
                        />
                    }
                    .into_any()
                }

                (true, false) => view! {
                    <p class="py-4 text-sm text-ink-3">"No chart for this song yet."</p>
                }
                .into_any(),

                _ => ().into_any(),
            }}
        </article>
    }
}
