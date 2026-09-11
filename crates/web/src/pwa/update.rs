//! Applying a new version.
//!
//! A downloaded update waits. It waits longer if a live session is running, because a reload
//! mid-song is the worst thing this app could do — and the flag that says so is set by the
//! control surface and cleared when the session ends (PWA business rule 3).
//!
//! The flag lives in `localStorage` rather than in a signal because the control surface and the
//! window showing the toast are not always the same window.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::app::storage;

const SESSION_FLAG: &str = "aurum.session.active";

pub fn hold(active: bool) {
    // Without storage the hold cannot be recorded; the toast is still never automatic.
    if active {
        storage::write(SESSION_FLAG, "1");
    } else {
        storage::remove(SESSION_FLAG);
    }
}

pub fn held() -> bool {
    storage::read(SESSION_FLAG).as_deref() == Some("1")
}

/// Holds updates for as long as the calling component is mounted, and releases them on the way
/// out — including when the control window is closed rather than the session properly ended.
pub fn hold_while_open() {
    hold(true);
    on_cleanup(|| hold(false));
}

/// The toast that offers a downloaded version.
///
/// It is never automatic. A reload is a decision, and it is the operator's — the app has no way
/// to know whether the person in front of it is mid-verse.
#[component]
pub fn UpdateToast() -> impl IntoView {
    use leptos::task::spawn_local;
    use wasm_bindgen::prelude::*;
    let ready = RwSignal::new(false);
    let later = RwSignal::new(false);

    // Business rule 2: check on foreground and every half hour while there is a connection.
    spawn_local(async move {
        loop {
            if crate::online() {
                check().await;
            }

            gloo_timers::future::TimeoutFuture::new(30 * 60 * 1000).await;

            if ready.try_get().is_none() {
                return;
            }
        }
    });

    if let Some(container) = registration() {
        let waiting = Closure::<dyn Fn()>::new(move || ready.set(waiting_worker().is_some()));

        let _ = container
            .add_event_listener_with_callback("updatefound", waiting.as_ref().unchecked_ref());
        waiting.forget();

        // A worker that finished downloading before this window opened is still waiting.
        ready.set(waiting_worker().is_some());
    }

    let reload = move |_| {
        let Some(worker) = waiting_worker() else {
            return;
        };

        // `skipWaiting`, then the browser reloads once the new worker takes control.
        let _ = js_sys::Reflect::get(&worker, &JsValue::from_str("postMessage"))
            .ok()
            .and_then(|held| held.dyn_into::<js_sys::Function>().ok())
            .map(|post| post.call1(&worker, &JsValue::from_str("skip-waiting")));

        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(300).await;

            if let Some(window) = web_sys::window() {
                let _ = window.location().reload();
            }
        });
    };

    view! {
        <Show when=move || { ready.get() && !later.get() && !held() }>
            <div
                class="fixed bottom-4 left-1/2 z-30 flex -translate-x-1/2 items-center gap-3 rounded-full bg-accent px-4 py-2 text-sm text-on-accent shadow-lg"
                data-testid="update-toast"
            >
                <span>"A new version is ready."</span>
                <button class="text-ink-3 hover:text-ink underline-offset-2 hover:underline" on:click=reload>"Reload"</button>
                <button class="opacity-70 text-ink-3 hover:text-ink underline-offset-2 hover:underline" on:click=move |_| later.set(true)>
                    "Later"
                </button>
            </div>
        </Show>
    }
}

/// The service worker container, where there is one.
fn registration() -> Option<web_sys::ServiceWorkerContainer> {
    Some(web_sys::window()?.navigator().service_worker())
}

/// A worker that has downloaded and is waiting for permission to take over.
fn waiting_worker() -> Option<wasm_bindgen::JsValue> {
    use wasm_bindgen::JsValue;

    let container = registration()?;
    let ready = js_sys::Reflect::get(container.as_ref(), &JsValue::from_str("controller")).ok()?;

    // `controller` says a worker is running; the waiting one is found through the registration,
    // which is only reachable asynchronously — so this is refreshed by the `updatefound` event
    // above rather than polled.
    let _ = ready;

    WAITING.with(|held| held.borrow().clone())
}

thread_local! {
    static WAITING: std::cell::RefCell<Option<wasm_bindgen::JsValue>> =
        const { std::cell::RefCell::new(None) };
}

/// Asks the browser to look for a new worker, and remembers one that is waiting.
async fn check() {
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    let Some(container) = registration() else {
        return;
    };

    let Ok(registration) = JsFuture::from(
        container
            .ready()
            .unwrap_or_else(|_| js_sys::Promise::resolve(&JsValue::UNDEFINED)),
    )
    .await
    else {
        return;
    };

    if let Ok(update) = js_sys::Reflect::get(&registration, &JsValue::from_str("update"))
        && let Ok(update) = update.dyn_into::<js_sys::Function>()
        && let Ok(promise) = update.call0(&registration)
        && let Ok(promise) = promise.dyn_into::<js_sys::Promise>()
    {
        let _ = JsFuture::from(promise).await;
    }

    let waiting = js_sys::Reflect::get(&registration, &JsValue::from_str("waiting")).ok();

    WAITING.with(|held| {
        *held.borrow_mut() = waiting.filter(|value| !value.is_null() && !value.is_undefined());
    });
}
