//! IndexedDB, as futures rather than callbacks.
//!
//! `web_sys` exposes the API faithfully, which means every operation is an event target you
//! attach `onsuccess` and `onerror` to. This module is the one place that deals with that, so
//! nothing above it has to.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::Function;
use wasm_bindgen::prelude::*;
use web_sys::{IdbOpenDbRequest, IdbRequest};

use crate::db::DbError;

/// Resolves when the request succeeds, with its result.
pub async fn request(request: IdbRequest) -> Result<JsValue, DbError> {
    let (sender, receiver) = oneshot();
    let resolve = sender.clone();
    let reject = sender;

    let on_success = Closure::once_into_js(move |event: web_sys::Event| {
        let result = event
            .target()
            .and_then(|target| target.dyn_into::<IdbRequest>().ok())
            .and_then(|request| request.result().ok())
            .unwrap_or(JsValue::UNDEFINED);

        resolve(Ok(result));
    });
    let on_error = Closure::once_into_js(move |_event: web_sys::Event| {
        reject(Err(DbError::Request("the request failed".to_owned())));
    });

    request.set_onsuccess(Some(on_success.unchecked_ref::<Function>()));
    request.set_onerror(Some(on_error.unchecked_ref::<Function>()));

    receiver.await
}

/// The same, for the open request, whose upgrade handler has to run first.
pub async fn open_request(
    request: IdbOpenDbRequest,
    upgrade: impl FnOnce(&web_sys::IdbDatabase) + 'static,
) -> Result<JsValue, DbError> {
    let mut upgrade = Some(upgrade);

    let on_upgrade = Closure::once_into_js(move |event: web_sys::IdbVersionChangeEvent| {
        let Some(database) = event
            .target()
            .and_then(|target| target.dyn_into::<IdbOpenDbRequest>().ok())
            .and_then(|request| request.result().ok())
            .and_then(|result| result.dyn_into::<web_sys::IdbDatabase>().ok())
        else {
            return;
        };

        if let Some(upgrade) = upgrade.take() {
            upgrade(&database);
        }
    });

    request.set_onupgradeneeded(Some(on_upgrade.unchecked_ref::<Function>()));

    self::request(request.unchecked_into()).await
}

/// What IndexedDB eventually hands back.
type Answer = Result<JsValue, DbError>;

/// A minimal one-shot channel: IndexedDB hands us its answer in a callback, and the caller is
/// awaiting a future.
fn oneshot() -> (
    Rc<dyn Fn(Answer)>,
    impl std::future::Future<Output = Answer>,
) {
    #[derive(Default)]
    struct Shared {
        value: Option<Result<JsValue, DbError>>,
        waker: Option<std::task::Waker>,
    }

    let shared = Rc::new(RefCell::new(Shared::default()));
    let sender = shared.clone();

    let send = move |value: Answer| {
        let mut shared = sender.borrow_mut();

        if shared.value.is_none() {
            shared.value = Some(value);
        }

        if let Some(waker) = shared.waker.take() {
            waker.wake();
        }
    };

    let receiver = std::future::poll_fn(move |context| {
        let mut held = shared.borrow_mut();

        match held.value.take() {
            Some(value) => std::task::Poll::Ready(value),
            None => {
                held.waker = Some(context.waker().clone());
                std::task::Poll::Pending
            }
        }
    });

    (Rc::new(send), receiver)
}
