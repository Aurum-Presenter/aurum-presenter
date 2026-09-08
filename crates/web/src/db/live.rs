//! A write to a store, turned into a signal.
//!
//! This is the one genuinely new piece of infrastructure in the client rewrite. Dexie's
//! `liveQuery` gave the React app a subscription per query: read the songs of a folder, and the
//! list re-renders when anything writes to `songs`. Almost every screen depends on it, so it has
//! to exist before anything that reads data.
//!
//! The implementation is deliberately coarse. A version counter per store, bumped on every
//! write, and a query that reads the counter before it reads the data — so any write to a store
//! re-runs every query over it. Dexie is finer-grained than that, but the finer granularity
//! costs a dependency graph nobody would be able to debug at two in the morning, and the app's
//! largest store is a few thousand songs.
//!
//! It also crosses windows. The app is routinely open three times at once — library, control
//! surface, stage — and a song edited in one has to appear in the others, which is what the
//! broadcast channel is for.

use std::cell::RefCell;
use std::collections::HashMap;

use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use web_sys::{BroadcastChannel, MessageEvent};

thread_local! {
    static VERSIONS: RefCell<HashMap<String, RwSignal<u64>>> = RefCell::new(HashMap::new());
    static CHANNEL: RefCell<Option<BroadcastChannel>> = const { RefCell::new(None) };
}

fn channel_name(workspace_id: &str) -> String {
    format!("aurum-db-{workspace_id}")
}

fn version_of(store: &str) -> RwSignal<u64> {
    VERSIONS.with(|versions| {
        *versions
            .borrow_mut()
            .entry(store.to_owned())
            .or_insert_with(|| RwSignal::new(0))
    })
}

/// Starts listening for writes made in the app's other windows.
///
/// Called once, when a workspace is opened. Without it the control surface would keep showing
/// the set as it was when the operator opened it.
pub fn listen(workspace_id: &str) {
    let Ok(channel) = BroadcastChannel::new(&channel_name(workspace_id)) else {
        return;
    };

    let on_message = Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
        if let Some(store) = event.data().as_string() {
            bump(&store);
        }
    });

    channel.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();

    CHANNEL.with(|held| *held.borrow_mut() = Some(channel));
}

/// Announces that a store was written: here, and in every other window of this app.
pub fn changed(workspace_id: &str, store: &str) {
    bump(store);

    CHANNEL.with(|held| {
        if let Some(channel) = held.borrow().as_ref() {
            let _ = channel.post_message(&JsValue::from_str(store));
        } else if let Ok(channel) = BroadcastChannel::new(&channel_name(workspace_id)) {
            // A window that writes before it has started listening still has to tell the others.
            let _ = channel.post_message(&JsValue::from_str(store));
        }
    });
}

fn bump(store: &str) {
    let version = version_of(store);

    version.update(|count| *count = count.wrapping_add(1));
}

/// Reads the version counters of the stores a query depends on, so that reading them inside a
/// reactive scope subscribes the query to their writes.
///
/// Public because a query is written by hand — Leptos has no query builder to hide this in —
/// and calling it is the whole subscription.
pub fn watching(stores: &[&str]) {
    for store in stores {
        version_of(store).track();
    }
}

/// Re-runs an asynchronous read whenever any of the named stores is written.
///
/// The Leptos equivalent of `liveQuery`: the closure is an ordinary async read, and the list of
/// stores is what makes it live.
pub fn live_query<T, F, Fut>(stores: &'static [&'static str], read: F) -> LocalResource<T>
where
    T: 'static,
    F: Fn() -> Fut + 'static,
    Fut: std::future::Future<Output = T> + 'static,
{
    LocalResource::new(move || {
        watching(stores);

        read()
    })
}
