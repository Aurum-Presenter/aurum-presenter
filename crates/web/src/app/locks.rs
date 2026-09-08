//! Locks that hold across every window of the app on one device.
//!
//! The app is routinely open three times at once — library, control surface, stage — and each
//! runs the same thirty-second tick. Without this they would all drain the same outbox and pull
//! the same deltas: harmless, because pushes are idempotent and the watermark is monotonic, but
//! three times the requests for one device's work (offline-sync acceptance criterion 8).
//!
//! `web_sys` has no binding for the Web Locks API, so this is a hand-written one.

use std::future::Future;

use js_sys::{Function, Object, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// `navigator.locks`, or `None` where the API does not exist.
fn manager() -> Option<Object> {
    let navigator = web_sys::window()?.navigator();
    let locks = Reflect::get(&navigator, &"locks".into()).ok()?;

    (!locks.is_undefined() && !locks.is_null()).then(|| locks.unchecked_into())
}

fn request(manager: &Object, name: &str, options: Option<Object>, callback: &Function) -> Promise {
    let method: Function = Reflect::get(manager, &"request".into())
        .expect("locks.request")
        .unchecked_into();

    let result = match options {
        Some(options) => method.call3(manager, &name.into(), &options, callback),
        None => method.call2(manager, &name.into(), callback),
    };

    result
        .expect("locks.request accepts these arguments")
        .unchecked_into()
}

/// Runs the pass only if no other window is already running it, and returns `skipped` if one is.
///
/// The lock is held only while the pass runs, so a window closed mid-sync hands it straight to
/// the next one rather than leaving the device stuck.
pub async fn as_sole_worker<T: 'static>(
    name: &str,
    pass: impl Future<Output = T> + 'static,
    skipped: T,
) -> T {
    let Some(manager) = manager() else {
        // Safari before 15.4, and any context without the API. Falling back to running the pass
        // is the right way round: syncing twice is a waste, not syncing at all is data loss.
        return pass.await;
    };

    let held = std::rc::Rc::new(std::cell::RefCell::new(None));
    let outcome = held.clone();
    let mut pass = Some(pass);

    let callback = Closure::once_into_js(move |lock: JsValue| -> Promise {
        if lock.is_null() {
            return Promise::resolve(&JsValue::UNDEFINED);
        }

        let pass = pass.take().expect("the callback runs once");

        wasm_bindgen_futures::future_to_promise(async move {
            *outcome.borrow_mut() = Some(pass.await);

            Ok(JsValue::UNDEFINED)
        })
    });

    let options = Object::new();
    Reflect::set(&options, &"ifAvailable".into(), &true.into()).expect("a plain object");

    let _ = JsFuture::from(request(
        &manager,
        name,
        Some(options),
        callback.unchecked_ref(),
    ))
    .await;

    held.borrow_mut().take().unwrap_or(skipped)
}

/// Waits its turn rather than skipping: for work every caller must do, but only one at a time.
///
/// The refresh cookie is the case that matters. Three windows opening at once each ask for a new
/// access token, and rotation means a second use of the same refresh token is theft as far as
/// the server is concerned — it revokes the whole family and signs the musician out everywhere,
/// mid-service, for the crime of having the control surface and the stage view open. Taking
/// turns means each window presents the cookie the last one left behind.
pub async fn in_turn<T: 'static>(name: &str, work: impl Future<Output = T> + 'static) -> T {
    let Some(manager) = manager() else {
        return work.await;
    };

    let held = std::rc::Rc::new(std::cell::RefCell::new(None));
    let outcome = held.clone();
    let mut work = Some(work);

    let callback = Closure::once_into_js(move |_lock: JsValue| -> Promise {
        let work = work.take().expect("the callback runs once");

        wasm_bindgen_futures::future_to_promise(async move {
            *outcome.borrow_mut() = Some(work.await);

            Ok(JsValue::UNDEFINED)
        })
    });

    let _ = JsFuture::from(request(&manager, name, None, callback.unchecked_ref())).await;

    held.borrow_mut()
        .take()
        .expect("the lock callback always runs")
}
