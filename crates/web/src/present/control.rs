//! The control surface: one writer, many screens.
//!
//! Every change bumps the revision and goes out to every output at once. Outputs that stop
//! acking are marked and left alone — one dead tablet must not stall the projector.

use std::rc::Rc;

use aurum_core::present::session::{
    BlankMode, CODE_TTL_MS, OutputKind, OutputStatus, SessionMessage, SessionState, code_life,
    pairing_code,
};
use aurum_core::present::slides::Slide;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};
use wasm_bindgen::prelude::*;
use web_sys::KeyboardEvent;

use super::displays::{Opened, open_audience};
use super::pairing::{PairingHost, Status};
use super::slide::{AudienceSlide, StageSlide};
use super::store::Sessions;
use super::theme::ThemeDrawer;
use super::transport::ControlTransport;
use crate::app::{storage, use_workspace};

/// Per device, not per workspace: it is about this laptop's screens (presenter-output prefs).
const HINT_DISMISSED: &str = "aurum.presenter.hint-dismissed";

/// The three blanking modes, in the order an operator reaches for them.
const BLANK_MODES: [(BlankMode, &str); 3] = [
    (BlankMode::Black, "black"),
    (BlankMode::Logo, "logo"),
    (BlankMode::Freeze, "freeze"),
];

/// The running order, grouped by the item each slide belongs to.
#[derive(Clone, Debug, PartialEq)]
struct Group {
    item_id: String,
    title: String,
    slides: Vec<(usize, Slide)>,
}

fn grouped(slides: &[Slide]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();

    for (index, slide) in slides.iter().enumerate() {
        match groups.last_mut() {
            Some(last) if last.item_id == slide.item_id => {
                last.slides.push((index, slide.clone()));
            }
            _ => groups.push(Group {
                item_id: slide.item_id.clone(),
                title: slide.song_title.clone(),
                slides: vec![(index, slide.clone())],
            }),
        }
    }

    groups
}

#[component]
pub fn ControlPage() -> impl IntoView {
    let context = use_workspace();
    let params = use_params_map();
    let navigate = StoredValue::new(use_navigate());
    let session_id = params
        .read_untracked()
        .get("session_id")
        .unwrap_or_default();
    let workspace_id = StoredValue::new(context.workspace.get_untracked().id);

    let state = RwSignal::new(None::<SessionState>);
    let outputs = RwSignal::new(Vec::<OutputStatus>::new());
    let hint = RwSignal::new(None::<String>);
    let code = RwSignal::new(None::<(String, i64)>);
    let pairing = RwSignal::new(None::<Status>);
    let code_expired = RwSignal::new(false);
    let now = RwSignal::new(crate::now_ms());
    let message = RwSignal::new(String::new());
    let stage_message = RwSignal::new(String::new());
    let theme_open = RwSignal::new(false);
    let ending = RwSignal::new(false);
    let hint_dismissed = RwSignal::new(storage::read(HINT_DISMISSED).as_deref() == Some("1"));

    // The operator's laptop must not dim between songs any more than a musician's phone does.
    crate::pwa::wake_lock::hold_while_open();

    // A reload mid-song is the worst thing the app could do, so a downloaded update waits for
    // the session to end (PWA business rule 3).
    crate::pwa::update::hold_while_open();

    let sessions = StoredValue::new_local(context.db.get_untracked().map(Sessions::new));
    let audience_window = StoredValue::new_local(None::<Opened>);
    let host = StoredValue::new_local(None::<Rc<PairingHost>>);
    let transport = StoredValue::new_local(None::<ControlTransport>);

    // One place where a new state is stored, mirrored and broadcast, so the three cannot drift.
    let apply = move |next: SessionState| {
        state.set(Some(next.clone()));

        transport.with_value(|held| {
            if let Some(transport) = held {
                transport.broadcast(&next);
            }
        });

        if let Some(sessions) = sessions.get_value() {
            spawn_local(async move {
                let _ = sessions.save(&next).await;
            });
        }
    };

    let move_by = move |delta: i64| {
        let Some(current) = state.get_untracked() else {
            return;
        };

        let next = current.advanced(delta);

        apply(next.clone());

        if let Some(sessions) = sessions.get_value() {
            spawn_local(async move {
                let _ = sessions.log_advance(&next).await;
            });
        }
    };

    {
        let session_id = session_id.clone();

        let control = ControlTransport::open(
            &session_id,
            Callback::new(move |found: Vec<OutputStatus>| outputs.set(found)),
            // Honoured only for an output the operator has granted the advance to; the
            // transport has already checked that before this runs.
            Callback::new(move |(delta, _): (i64, String)| move_by(delta)),
        );

        transport.set_value(control);
    }

    // The mirror is the truth on a reload: the control window may have been closed and reopened.
    {
        let session_id = session_id.clone();

        spawn_local(async move {
            let Some(sessions) = sessions.get_value() else {
                return;
            };

            if let Some(loaded) = sessions.load(&session_id).await {
                state.set(Some(loaded.clone()));

                transport.with_value(|held| {
                    if let Some(transport) = held {
                        transport.broadcast(&loaded);
                    }
                });
            }
        });
    }

    // The operator's hands are on the keyboard, not the mouse: space and the arrows move, B
    // blanks. Not while they are typing a message, which is the one time those keys mean text.
    if let Some(window) = web_sys::window() {
        let listener = Closure::<dyn Fn(KeyboardEvent)>::new(move |event: KeyboardEvent| {
            let typing = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.active_element())
                .is_some_and(|element| element.tag_name() == "INPUT");

            if typing {
                return;
            }

            match event.key().as_str() {
                "ArrowRight" | "PageDown" | " " => {
                    event.prevent_default();
                    move_by(1);
                }
                "ArrowLeft" | "PageUp" => {
                    event.prevent_default();
                    move_by(-1);
                }
                "b" | "B" => {
                    if let Some(current) = state.get_untracked() {
                        apply(current.blanked(BlankMode::Black));
                    }
                }
                _ => {}
            }
        });

        let _ =
            window.add_event_listener_with_callback("keydown", listener.as_ref().unchecked_ref());
        listener.forget();
    }

    // A code that has run out stops working (stage-view acceptance criterion 8). The control
    // surface stops listening rather than merely saying the code is old — a code written on a
    // whiteboard an hour ago must not still be a way into a session.
    spawn_local(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(5000).await;

            if now.try_set(crate::now_ms()).is_none() {
                return;
            }

            let (Some((_, expires)), pairing_now) = (code.get_untracked(), pairing.get_untracked())
            else {
                continue;
            };

            if pairing_now == Some(Status::Connected) {
                continue;
            }

            if code_life(expires, crate::now_ms()).expired {
                host.with_value(|held| {
                    if let Some(host) = held {
                        host.close();
                    }
                });

                host.set_value(None);
                pairing.set(None);
                code.set(None);
                code_expired.set(true);
            }
        }
    });

    let open_output = move |kind: &'static str| {
        let Some(window) = web_sys::window() else {
            return;
        };

        let origin = window.location().origin().unwrap_or_default();
        let url = format!(
            "{origin}/output/{kind}?session={}&workspace={}",
            params
                .read_untracked()
                .get("session_id")
                .unwrap_or_default(),
            workspace_id.get_value(),
        );

        if kind == "stage" {
            let _ = window.open_with_url_and_target_and_features(
                &url,
                "aurum-stage",
                "width=1000,height=700",
            );

            return;
        }

        spawn_local(async move {
            let opened = open_audience(&url).await;

            hint.set(opened.hint.clone());
            audience_window.set_value(Some(opened));
        });
    };

    let start_pairing = move |_| {
        let mut random = [0_u8; 6];

        for byte in &mut random {
            *byte = (js_sys::Math::random() * 256.0) as u8;
        }

        let value = pairing_code(random);

        code.set(Some((value.clone(), crate::now_ms() + CODE_TTL_MS)));
        pairing.set(Some(Status::Waiting));
        code_expired.set(false);

        host.with_value(|held| {
            if let Some(host) = held {
                host.close();
            }
        });

        let workspace_id = context.workspace.get_untracked().id;

        let started = PairingHost::listen(
            &value,
            &workspace_id,
            Callback::new(move |peer: Rc<super::pairing::Peer>| {
                transport.with_value(|held| {
                    let Some(transport) = held else {
                        return;
                    };

                    transport.attach_peer(&peer.output_id, peer.send.clone());
                    transport.receive(
                        SessionMessage::Hello {
                            output_id: peer.output_id.clone(),
                            kind: OutputKind::PairedStage,
                            label: "Paired device".to_owned(),
                        },
                        Callback::new(move |found: Vec<OutputStatus>| outputs.set(found)),
                        Callback::new(move |(delta, _): (i64, String)| move_by(delta)),
                    );
                });
            }),
            Callback::new(move |(output_id, incoming): (String, SessionMessage)| {
                // A state message can only travel outwards; anything else is stamped with the
                // id this control gave the peer, so a device cannot answer as another output.
                let stamped = match incoming {
                    SessionMessage::State { state } => SessionMessage::State { state },
                    SessionMessage::Ack {
                        kind,
                        label,
                        revision,
                        ..
                    } => SessionMessage::Ack {
                        output_id,
                        kind,
                        label,
                        revision,
                    },
                    SessionMessage::Hello { kind, label, .. } => SessionMessage::Hello {
                        output_id,
                        kind,
                        label,
                    },
                    SessionMessage::RequestState { .. } => {
                        SessionMessage::RequestState { output_id }
                    }
                    SessionMessage::Advance { delta, .. } => {
                        SessionMessage::Advance { output_id, delta }
                    }
                    SessionMessage::Bye { .. } => SessionMessage::Bye { output_id },
                };

                transport.with_value(|held| {
                    if let Some(transport) = held {
                        transport.receive(
                            stamped.clone(),
                            Callback::new(move |found: Vec<OutputStatus>| outputs.set(found)),
                            Callback::new(move |(delta, _): (i64, String)| move_by(delta)),
                        );
                    }
                });
            }),
            Callback::new(move |status: Status| pairing.set(Some(status))),
        );

        host.set_value(Some(started));
    };

    let finish = move |_| {
        let Some(current) = state.get_untracked() else {
            return;
        };

        if let Some(sessions) = sessions.get_value() {
            let current = current.clone();

            spawn_local(async move {
                let _ = sessions.end(&current).await;
            });
        }

        transport.with_value(|held| {
            if let Some(transport) = held {
                transport.broadcast(&SessionState {
                    ended: true,
                    revision: current.revision + 1,
                    ..current.clone()
                });
            }
        });

        audience_window.with_value(|held| {
            if let Some(opened) = held {
                if let Some(window) = &opened.window {
                    let _ = window.close();
                }

                if let Some(connection) = &opened.connection {
                    let _ = js_sys::Reflect::get(connection, &JsValue::from_str("terminate"))
                        .ok()
                        .and_then(|held| held.dyn_into::<js_sys::Function>().ok())
                        .map(|terminate| terminate.call0(connection));
                }
            }
        });

        navigate.get_value()(
            &match current.set_snapshot.set_id.as_deref() {
                Some(id) => format!("/sets/{id}"),
                None => "/sets".to_owned(),
            },
            Default::default(),
        );
    };

    on_cleanup(move || {
        transport.with_value(|held| {
            if let Some(transport) = held {
                transport.close();
            }
        });

        host.with_value(|held| {
            if let Some(host) = held {
                host.close();
            }
        });
    });

    let slides = Signal::derive(move || state.get().map(|state| state.slides).unwrap_or_default());
    let groups = Memo::new(move |_| grouped(&slides.get()));

    view! {
        <Show
            when=move || state.get().is_some()
            fallback=|| view! {
                <div class="p-6 text-sm">
                    <p class="text-ink-3">"That session is not running on this device."</p>
                    <A href="/sets" attr:class="text-ink-3 hover:text-ink underline-offset-2 hover:underline">"Back to sets"</A>
                </div>
            }
        >
            <div
                class="grid h-[calc(100dvh-3.5rem)] grid-cols-1 gap-4 p-4 lg:grid-cols-[2fr_1fr]"
                data-testid="control"
            >
                <div class="flex min-h-0 flex-col gap-3">
                    <div class="flex flex-wrap items-center gap-2 text-sm">
                        // Red is live all the way through this screen — the badge, the preview
                        // border, the chip in the running order — so the operator never reads a
                        // label to know what the room is seeing.
                        <span class="flex h-6 items-center gap-2 rounded-sm bg-live px-2.5 text-[11px] font-bold tracking-[0.1em] text-white">
                            <span class="size-1.5 rounded-full bg-white"></span>
                            "LIVE"
                        </span>
                        <span class="font-medium">
                            {move || state
                                .get()
                                .map(|state| state.set_snapshot.set_name)
                                .unwrap_or_default()}
                        </span>
                        <span class="font-mono text-ink-3" data-testid="control-position">
                            {move || state
                                .get()
                                .map(|state| format!("{} / {}", state.index + 1, state.slides.len()))
                                .unwrap_or_default()}
                        </span>

                        <span class="ml-auto flex flex-wrap gap-2">
                            <button
                                class="rounded-md border border-line-strong px-3 py-1"
                                data-testid="previous-slide"
                                on:click=move |_| move_by(-1)
                            >
                                "← Previous"
                            </button>
                            <button
                                class="rounded-md bg-accent px-3 py-1 text-on-accent"
                                data-testid="next-slide"
                                on:click=move |_| move_by(1)
                            >
                                "Next →"
                            </button>

                            {BLANK_MODES
                                .into_iter()
                                .map(|(mode, label)| view! {
                                    <button
                                        class=move || if state
                                            .get()
                                            .is_some_and(|state| state.blank_mode == mode)
                                        {
                                            "rounded-md border px-3 py-1 border-accent bg-accent/10"
                                        } else {
                                            "rounded-md border px-3 py-1 border-line-strong"
                                        }
                                        data-testid=format!("blank-{label}")
                                        on:click=move |_| {
                                            if let Some(current) = state.get_untracked() {
                                                apply(current.blanked(mode));
                                            }
                                        }
                                    >
                                        {label}
                                    </button>
                                })
                                .collect_view()}

                            <button
                                class="rounded-md border border-line-strong px-3 py-1"
                                on:click=move |_| theme_open.set(true)
                            >
                                "Theme"
                            </button>
                            <button
                                class="rounded-md border border-live/50 px-3 py-1 text-live-ink"
                                data-testid="end-session"
                                on:click=move |_| ending.set(true)
                            >
                                "End"
                            </button>
                        </span>
                    </div>

                    <div class="grid min-h-0 flex-1 grid-cols-2 gap-3">
                        <figure class="flex min-h-0 flex-col">
                            <figcaption class="mb-1 flex items-center gap-2 text-xs uppercase tracking-widest text-live-ink">
                                <span class="size-1.5 rounded-full bg-live"></span>
                                "On the audience screen"
                            </figcaption>
                            <div
                                class="aspect-video overflow-hidden rounded-md border-2 border-live"
                                data-testid="audience-preview"
                                style=move || {
                                    let theme = state
                                        .get()
                                        .map(|state| state.theme)
                                        .unwrap_or_default();

                                    format!(
                                        "background: {}; color: {}",
                                        theme.background_value,
                                        theme.text_color,
                                    )
                                }
                            >
                                <AudienceSlide
                                    slide=Signal::derive(move || {
                                        state.get().and_then(|state| state.audience_slide().cloned())
                                    })
                                    theme=Signal::derive(move || {
                                        state.get().map(|state| state.theme).unwrap_or_default()
                                    })
                                    workspace_id=Signal::derive(move || {
                                        state
                                            .get()
                                            .map(|state| state.workspace_id)
                                            .unwrap_or_default()
                                    })
                                />
                            </div>
                        </figure>

                        <figure class="flex min-h-0 flex-col">
                            <figcaption class="mb-1 flex items-center gap-2 text-xs uppercase tracking-widest text-accent">
                                <span class="size-1.5 rounded-full bg-accent"></span>
                                "Next"
                            </figcaption>
                            <div class="aspect-video overflow-hidden rounded-md border border-accent/40 bg-black p-3 text-white">
                                <StageSlide
                                    slide=Signal::derive(move || {
                                        state.get().and_then(|state| state.next_slide().cloned())
                                    })
                                    target_key=Signal::derive(|| None)
                                    show_chords=Signal::derive(|| true)
                                    scale=Signal::derive(|| 2.0)
                                    empty="End of the set."
                                />
                            </div>
                        </figure>
                    </div>

                    <Announcement
                        placeholder="Message on the audience screen…"
                        testid="audience-message-input"
                        value=message
                        show="Show"
                        on_show=Callback::new(move |text: Option<String>| {
                            if let Some(current) = state.get_untracked() {
                                apply(current.with_message(text.as_deref()));
                            }
                        })
                    />

                    <Announcement
                        placeholder="Message to the stage only — two more times…"
                        testid="stage-message-input"
                        value=stage_message
                        show="Send"
                        on_show=Callback::new(move |text: Option<String>| {
                            if let Some(current) = state.get_untracked() {
                                apply(current.with_stage_message(text.as_deref()));
                            }
                        })
                    />
                </div>

                <aside class="flex min-h-0 flex-col gap-4 overflow-auto text-sm">
                    <section>
                        <h3 class="mb-1 font-semibold">"Screens"</h3>

                        <div class="mb-2 flex flex-wrap gap-2">
                            <button
                                class="rounded-md border border-line-strong px-3 py-1"
                                data-testid="open-audience"
                                on:click=move |_| open_output("audience")
                            >
                                "Audience window"
                            </button>
                            <button
                                class="rounded-md border border-line-strong px-3 py-1"
                                data-testid="open-stage"
                                on:click=move |_| open_output("stage")
                            >
                                "Stage window"
                            </button>
                            <button
                                class="rounded-md border border-line-strong px-3 py-1"
                                data-testid="pair-a-device"
                                on:click=start_pairing
                            >
                                "Pair a device"
                            </button>
                        </div>

                        <Show when=move || { hint.get().is_some() && !hint_dismissed.get() }>
                            <p class="mb-2 text-xs text-ink-3">
                                {move || hint.get()}
                                " "
                                <button
                                    class="text-ink-3 hover:text-ink underline-offset-2 hover:underline"
                                    on:click=move |_| {
                                        hint_dismissed.set(true);
                                        // Without storage it comes back next time, which is the
                                        // safe direction.
                                        storage::write(HINT_DISMISSED, "1");
                                    }
                                >
                                    "Got it"
                                </button>
                            </p>
                        </Show>

                        <Show when=move || code_expired.get()>
                            <p class="mb-2 text-xs text-warn">
                                "That code has expired. Issue a new one to pair a device."
                            </p>
                        </Show>

                        {move || code.get().map(|(value, expires)| view! {
                            <div
                                class="mb-2 rounded-md border border-line p-2"
                                data-testid="pairing-code"
                            >
                                <p class="text-xs text-ink-3">
                                    "On the other device: open Aurum, choose Join session, and enter"
                                </p>
                                <p class="my-1 font-mono text-2xl tracking-widest">{value}</p>
                                <p class="text-xs text-ink-3">
                                    {move || match pairing.get() {
                                        Some(Status::Failed) => "The signalling relay could not \
                                             be reached; a device on this network can still not \
                                             be paired."
                                            .to_owned(),
                                        Some(Status::Connected) => "A device is connected.".to_owned(),
                                        _ => format!(
                                            "Waiting for a device. The code works for {} more \
                                             minutes.",
                                            code_life(expires, now.get()).minutes_left,
                                        ),
                                    }}
                                </p>
                            </div>
                        })}

                        <ul class="space-y-1" data-testid="outputs">
                            <Show when=move || outputs.get().is_empty()>
                                <li class="text-ink-3">"No screens attached yet."</li>
                            </Show>

                            {move || outputs
                                .get()
                                .into_iter()
                                .map(|output| {
                                    let id = output.output_id.clone();
                                    let paired = output.kind == OutputKind::PairedStage;

                                    view! {
                                        <li class="flex items-center gap-2">
                                            <span class=if output.responding {
                                                "text-ok"
                                            } else {
                                                "text-warn"
                                            }>
                                                "●"
                                            </span>
                                            <span>{output.label.clone()}</span>
                                            <span class="text-xs text-ink-3">
                                                {if output.responding {
                                                    format!("revision {}", output.last_ack_revision)
                                                } else {
                                                    "not responding".to_owned()
                                                }}
                                            </span>

                                            {paired.then(|| view! {
                                                <label class="ml-auto flex items-center gap-1 text-xs">
                                                    <input
                                                        type="checkbox"
                                                        prop:checked=output.can_advance
                                                        on:change=move |event| {
                                                            transport.with_value(|held| {
                                                                if let Some(transport) = held {
                                                                    outputs.set(transport.grant_advance(
                                                                        &id,
                                                                        event_target_checked(&event),
                                                                    ));
                                                                }
                                                            });
                                                        }
                                                    />
                                                    "may advance"
                                                </label>
                                            })}
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                    </section>

                    <section class="min-h-0 flex-1">
                        <h3 class="mb-1 font-semibold">"Running order"</h3>
                        <ol class="space-y-1" data-testid="slide-list">
                            {move || groups
                                .get()
                                .into_iter()
                                .map(|group| view! {
                                    <li>
                                        <p class="mt-2 text-xs uppercase tracking-widest text-ink-3">
                                            {group.title.clone()}
                                        </p>
                                        <div class="flex flex-wrap gap-1">
                                            {group
                                                .slides
                                                .into_iter()
                                                .map(|(index, slide)| view! {
                                                    <button
                                                        class=move || {
                                                            // The same two signals as the previews:
                                                            // red is on the screen now, gold is
                                                            // what one press away looks like.
                                                            let here = state
                                                                .get()
                                                                .map(|state| state.index);

                                                            match here {
                                                                Some(at) if at == index => {
                                                                    "rounded-md border px-2 py-1 text-xs font-semibold border-live bg-live/10 text-live-ink"
                                                                }
                                                                Some(at) if at + 1 == index => {
                                                                    "rounded-md border px-2 py-1 text-xs border-accent/60 text-accent"
                                                                }
                                                                _ => "rounded-md border px-2 py-1 text-xs border-line text-ink-3",
                                                            }
                                                        }
                                                        on:click=move |_| {
                                                            if let Some(current) = state.get_untracked() {
                                                                apply(current.jumped(index as i64));
                                                            }
                                                        }
                                                    >
                                                        {slide
                                                            .label
                                                            .clone()
                                                            .unwrap_or_else(|| {
                                                                slide.kind.as_str().to_owned()
                                                            })}
                                                    </button>
                                                })
                                                .collect_view()}
                                        </div>
                                    </li>
                                })
                                .collect_view()}
                        </ol>
                    </section>
                </aside>

                <Show when=move || theme_open.get()>
                    <ThemeDrawer
                        theme=Signal::derive(move || {
                            state.get().map(|state| state.theme).unwrap_or_default()
                        })
                        on_close=Callback::new(move |()| theme_open.set(false))
                        on_change=Callback::new(move |theme| {
                            if let Some(current) = state.get_untracked() {
                                apply(SessionState {
                                    theme,
                                    revision: current.revision + 1,
                                    ..current
                                });
                            }
                        })
                    />
                </Show>

                <Show when=move || ending.get()>
                    <div class="fixed inset-0 z-20 flex items-center justify-center bg-black/60 p-6">
                        <div class="w-80 rounded-md bg-surface p-4 shadow-lg">
                            <h2 class="mb-2 font-semibold">"End this session?"</h2>
                            <p class="mb-4 text-sm text-ink-3">
                                "Every screen closes and the session is written to the log."
                            </p>
                            <div class="flex gap-2">
                                <button
                                    class="rounded-md bg-accent px-3 py-2 text-sm text-on-accent"
                                    data-testid="confirm-end"
                                    on:click=finish
                                >
                                    "End session"
                                </button>
                                <button
                                    class="text-sm text-ink-3 hover:text-ink underline-offset-2 hover:underline"
                                    on:click=move |_| ending.set(false)
                                >
                                    "Keep going"
                                </button>
                            </div>
                        </div>
                    </div>
                </Show>
            </div>
        </Show>
    }
}

/// One of the two message boxes. They differ only in who reads what they say.
#[component]
fn Announcement(
    placeholder: &'static str,
    testid: &'static str,
    value: RwSignal<String>,
    show: &'static str,
    on_show: Callback<Option<String>>,
) -> impl IntoView {
    view! {
        <div class="flex flex-wrap gap-2 text-sm">
            <input
                class="min-w-40 flex-1 rounded-md border border-line-strong px-3 py-2"
                placeholder=placeholder
                data-testid=testid
                prop:value=move || value.get()
                on:input=move |event| value.set(event_target_value(&event))
            />
            <button
                class="rounded-md border border-line-strong px-3"
                on:click=move |_| on_show.run(Some(value.get_untracked()))
            >
                {show}
            </button>
            <button
                class="rounded-md border border-line-strong px-3"
                on:click=move |_| {
                    value.set(String::new());
                    on_show.run(None);
                }
            >
                "Clear"
            </button>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurum_core::present::slides::SlideKind;

    fn slide(item_id: &str, title: &str) -> Slide {
        Slide {
            id: format!("{item_id}-{title}"),
            item_id: item_id.to_owned(),
            song_title: title.to_owned(),
            kind: SlideKind::Lyrics,
            ..Slide::default()
        }
    }

    #[test]
    fn slides_group_under_the_item_they_belong_to() {
        let slides = vec![
            slide("one", "Cornerstone"),
            slide("one", "Cornerstone"),
            slide("two", "Amazing Grace"),
        ];

        let groups = grouped(&slides);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].title, "Cornerstone");
        assert_eq!(groups[0].slides.len(), 2);
        assert_eq!(
            groups[1].slides[0].0, 2,
            "the index is the position in the whole set"
        );
    }

    /// A set can hold the same song twice — two verses of it either side of a reading — and the
    /// running order has to show that as two blocks, not one.
    #[test]
    fn the_same_song_twice_is_two_groups() {
        let slides = vec![
            slide("one", "Cornerstone"),
            slide("two", "Psalm 23"),
            slide("three", "Cornerstone"),
        ];

        assert_eq!(grouped(&slides).len(), 3);
    }

    #[test]
    fn nothing_groups_to_nothing() {
        assert!(grouped(&[]).is_empty());
    }
}
