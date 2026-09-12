//! The parts of the app that are about being an installed application rather than a page.

pub mod install;
pub mod share;
pub mod update;
pub mod wake_lock;

/// Registers the service worker, once, at start-up.
///
/// Never with `autoUpdate`: a new worker waits until the app says the moment is safe, which it
/// will not do while a live session is running (PWA business rule 1).
pub fn register_service_worker() {
    let Some(window) = web_sys::window() else {
        return;
    };

    let container = window.navigator().service_worker();

    // `navigator.serviceWorker` is only exposed in a secure context (https, or localhost) — on
    // a plain-http LAN address web_sys still hands back a wrapper around `undefined`, and
    // calling `.register()` on it throws instead of returning an error the app can ignore.
    if wasm_bindgen::JsValue::from(container.clone()).is_undefined() {
        return;
    }

    // A failure here is a browser without service workers, or a page served from a file. The
    // app works either way; it simply will not start with the radio off.
    let _ = container.register("/sw.js");
}
