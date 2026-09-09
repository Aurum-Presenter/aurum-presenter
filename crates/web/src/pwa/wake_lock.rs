//! Keeps the screen awake, and lets it sleep again on the way out.
//!
//! Every screen this is used on is one somebody is looking at without touching: a chart on a
//! music stand, a set in reader mode, a projector showing a slide through a long prayer. A
//! dimmed phone mid-song is the same failure as a lost slide (PWA business rule 4).
//!
//! The Wake Lock API is not in stable `web-sys`, so it is reached through `Reflect` — the same
//! way the app reaches Web Locks. The whole surface is two calls, and a browser that has
//! neither still works; the screen just dims as it always did.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// The sentinel a granted lock hands back. Releasing it is what lets the screen sleep again.
fn release(sentinel: &JsValue) {
    let Ok(release) = js_sys::Reflect::get(sentinel, &JsValue::from_str("release")) else {
        return;
    };

    if let Ok(release) = release.dyn_into::<js_sys::Function>() {
        let _ = release.call0(sentinel);
    }
}

async fn request() -> Option<JsValue> {
    let navigator = web_sys::window()?.navigator();
    let api = js_sys::Reflect::get(&navigator, &JsValue::from_str("wakeLock")).ok()?;

    if api.is_undefined() || api.is_null() {
        return None;
    }

    let requesting = js_sys::Reflect::get(&api, &JsValue::from_str("request"))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;

    let promise = requesting
        .call1(&api, &JsValue::from_str("screen"))
        .ok()?
        .dyn_into::<js_sys::Promise>()
        .ok()?;

    // Denied, unsupported, or the tab was not visible. Everything still works.
    JsFuture::from(promise).await.ok()
}

/// Holds a wake lock for as long as the calling component is mounted.
///
/// The browser drops the lock whenever the tab is hidden, so it is asked for again on the way
/// back — otherwise a glance at a text message would leave the stage screen dimming for the
/// rest of the set.
pub fn hold_while_open() {
    let held = StoredValue::new_local(None::<JsValue>);
    let gone = StoredValue::new(false);

    let take = move || {
        spawn_local(async move {
            let already = held.try_with_value(|held| held.is_some()) == Some(true);

            if gone.try_get_value() != Some(false) || already {
                return;
            }

            let sentinel = request().await;

            // The screen may have been left while the browser was deciding.
            match gone.try_get_value() {
                Some(false) => held.set_value(sentinel),
                _ => {
                    if let Some(sentinel) = sentinel {
                        release(&sentinel);
                    }
                }
            }
        });
    };

    take();

    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };

    let on_visible = Closure::<dyn Fn()>::new(move || {
        if web_sys::window()
            .and_then(|window| window.document())
            .is_some_and(|document| {
                document.visibility_state() == web_sys::VisibilityState::Visible
            })
        {
            // A hidden tab loses the lock, so this is a fresh request rather than a re-check.
            held.set_value(None);
            take();
        }
    });

    let _ = document
        .add_event_listener_with_callback("visibilitychange", on_visible.as_ref().unchecked_ref());

    let listener = StoredValue::new_local(Some(on_visible));

    on_cleanup(move || {
        gone.set_value(true);

        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            listener.with_value(|on_visible| {
                if let Some(on_visible) = on_visible {
                    let _ = document.remove_event_listener_with_callback(
                        "visibilitychange",
                        on_visible.as_ref().unchecked_ref(),
                    );
                }
            });
        }

        held.with_value(|sentinel| {
            if let Some(sentinel) = sentinel {
                release(sentinel);
            }
        });
    });
}
