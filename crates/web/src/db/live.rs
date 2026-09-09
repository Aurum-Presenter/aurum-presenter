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
    static VERSIONS: RefCell<HashMap<String, ArcRwSignal<u64>>> = RefCell::new(HashMap::new());
    static CHANNEL: RefCell<Option<BroadcastChannel>> = const { RefCell::new(None) };
}

fn channel_name(workspace_id: &str) -> String {
    format!("aurum-db-{workspace_id}")
}

/// The counter for one store.
///
/// Deliberately an `ArcRwSignal` and not an `RwSignal`. An `RwSignal` belongs to whichever
/// reactive owner happened to create it, and the first reader of a store is usually a component:
/// the library page reads `arrangements` to show each song's key. Navigate away and that owner is
/// disposed, taking the counter with it — after which every write to the store updates a signal
/// nobody can hear, and the next screen silently stops being live. An `ArcRwSignal` lives as long
/// as this map does, which is as long as the tab.
fn version_of(store: &str) -> ArcRwSignal<u64> {
    VERSIONS.with(|versions| {
        versions
            .borrow_mut()
            .entry(store.to_owned())
            .or_insert_with(|| ArcRwSignal::new(0))
            .clone()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this guards: the counter used to be an `RwSignal`, so the first component to read
    /// a store owned it. Navigating away disposed that owner, and every later write to the store
    /// went nowhere — the next screen showed data that never changed again.
    #[test]
    fn a_counter_outlives_the_screen_that_first_read_it() {
        let _root = Owner::new();
        _root.set();

        let screen = Owner::new();

        screen.with(|| watching(&["songs"]));
        screen.cleanup();
        drop(screen);

        let counter = version_of("songs");
        let before = counter.get_untracked();

        bump("songs");

        assert_eq!(
            counter.get_untracked(),
            before + 1,
            "a write after the first reader was disposed must still be heard",
        );
    }

    #[test]
    fn stores_count_separately() {
        let _root = Owner::new();
        _root.set();

        let songs = version_of("sets-test-a").get_untracked();

        bump("sets-test-b");

        assert_eq!(version_of("sets-test-a").get_untracked(), songs);
        assert_eq!(version_of("sets-test-b").get_untracked(), 1);
    }
}
