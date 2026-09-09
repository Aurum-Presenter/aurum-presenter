//! The audience screen.
//!
//! It renders the session state and nothing else: no database writes, no sync worker, no logic
//! of its own. That is what lets it be reloaded, moved between screens, or cast, and come back
//! on the right slide without being told anything.
//!
//! It never shows a spinner or a flash of white — before the first message arrives it paints the
//! theme background, because a white rectangle in front of a congregation is worse than a blank
//! one.

use aurum_core::present::session::{BackgroundKind, SessionState, Theme};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;
use wasm_bindgen::prelude::*;
use web_sys::KeyboardEvent;

use super::displays::go_fullscreen;
use super::slide::AudienceSlide;
use super::transport::OutputTransport;
use crate::api::Api;
use crate::db::Database;
use crate::db::records::LiveSession;

/// The colour behind everything, whatever the theme's background kind is.
fn colour_of(theme: &Theme) -> String {
    match theme.background_kind {
        BackgroundKind::Color | BackgroundKind::Gradient => theme.background_value.clone(),
        BackgroundKind::Image => "#000000".to_owned(),
    }
}

fn screen_label() -> String {
    let size = web_sys::window()
        .map(|window| window.screen())
        .and_then(|screen| screen.ok())
        .and_then(|screen| Some((screen.width().ok()?, screen.height().ok()?)));

    match size {
        Some((width, height)) => format!("Audience · {width}×{height}"),
        None => "Audience".to_owned(),
    }
}

#[component]
pub fn AudiencePage() -> impl IntoView {
    let params = use_query_map();
    let session_id = params.read_untracked().get("session").unwrap_or_default();
    let workspace_id = params.read_untracked().get("workspace").unwrap_or_default();

    let state = RwSignal::new(None::<SessionState>);
    let image = RwSignal::new(None::<String>);

    // A projector screen that sleeps mid-service is the same failure as a lost slide.
    crate::pwa::wake_lock::hold_while_open();

    let transport = StoredValue::new_local(OutputTransport::open(
        &session_id,
        aurum_core::present::session::OutputKind::Audience,
        &screen_label(),
        Callback::new(move |next: SessionState| state.set(Some(next))),
    ));

    // The first paint comes from the local mirror, so a window reopened mid-service shows the
    // current slide before the control surface has said anything.
    {
        let (session_id, workspace_id) = (session_id.clone(), workspace_id.clone());

        spawn_local(async move {
            if workspace_id.is_empty() {
                return;
            }

            let Ok(db) = Database::open(&workspace_id).await else {
                return;
            };

            let held: Option<LiveSession> =
                db.get("live_sessions", &session_id).await.ok().flatten();

            if let Some(mirrored) = held.and_then(|held| serde_json::from_value(held.state).ok())
                && state.get_untracked().is_none()
            {
                state.set(Some(mirrored));
            }
        });
    }

    // Full screen from inside the output window, where the gesture requirement is satisfiable.
    if let Some(window) = web_sys::window() {
        let listener = Closure::<dyn Fn(KeyboardEvent)>::new(move |event: KeyboardEvent| {
            if event.key().eq_ignore_ascii_case("f") {
                go_fullscreen();
            }
        });

        let _ =
            window.add_event_listener_with_callback("keydown", listener.as_ref().unchecked_ref());
        listener.forget();
    }

    // The control surface opened this window, so it may close it. If the browser refuses — a tab
    // somebody opened by hand — the screen goes to the theme's background rather than a stale
    // slide in front of a room.
    Effect::new(move |_| {
        if state.get().is_some_and(|state| state.ended)
            && let Some(window) = web_sys::window()
        {
            let _ = window.close();
        }
    });

    let theme = Signal::derive(move || state.get().map(|state| state.theme).unwrap_or_default());

    // The background image is read from this device's copy and only fetched if it is missing.
    // It is resolved here rather than passed in the state so every output resolves it for
    // itself — a paired tablet and a projector do not share a file store.
    Effect::new(move |_| {
        let Some(held) = state.get() else {
            return;
        };

        if held.theme.background_kind != BackgroundKind::Image {
            image.set(None);
            return;
        }

        spawn_local(async move {
            let Ok(db) = Database::open(&held.workspace_id).await else {
                return;
            };

            image.set(
                super::background::url_of(
                    &db,
                    &Api::default(),
                    &held.workspace_id,
                    &held.theme.background_value,
                )
                .await,
            );
        });
    });

    let running = Signal::derive(move || state.get().filter(|state| !state.ended));

    // Closed on the way out, so the control surface stops counting a window nobody has.
    on_cleanup(move || {
        transport.with_value(|held| {
            if let Some(transport) = held {
                transport.close();
            }
        });
    });

    view! {
        <div
            class="h-dvh w-dvw overflow-hidden"
            data-testid="audience"
            style=move || {
                let theme = theme.get();
                let colour = colour_of(&theme);

                match image.get() {
                    Some(url) => format!(
                        "background: {colour} center / cover no-repeat url({url}); color: {}",
                        theme.text_color,
                    ),
                    None => format!("background: {colour}; color: {}", theme.text_color),
                }
            }
        >
            <Show when=move || running.get().is_some()>
                <AudienceSlide
                    slide=Signal::derive(move || {
                        running.get().and_then(|state| state.audience_slide().cloned())
                    })
                    theme
                    workspace_id=Signal::derive(move || {
                        running.get().map(|state| state.workspace_id).unwrap_or_default()
                    })
                />
            </Show>

            {move || running.get().and_then(|state| state.message).map(|message| view! {
                <div
                    class="absolute inset-x-0 bottom-0 bg-black/70 p-6 text-center"
                    style="font-size: 5vh"
                    data-testid="audience-message"
                >
                    {message}
                </div>
            })}
        </div>
    }
}
