//! The Aurum Presenter client.
//!
//! Client-rendered, deliberately: the app has to start from a precached shell with the radio
//! off, which rules out server-side rendering and hydration. Every rule it obeys comes from
//! `aurum-core`, which the server links too — so a chart transposes the same way on both sides
//! of the wire because it is the same code.
#![forbid(unsafe_code)]

pub mod api;
pub mod app;
pub mod auth;
pub mod db;
pub mod library;
pub mod routes;
pub mod sync;

/// The wall clock, in the one format every timestamp in this system uses.
pub fn now() -> String {
    aurum_core::time::format(now_ms())
}

pub fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

/// A client-minted id. Every record this device creates gets one here, which is what lets it be
/// created with the radio off and still reach the server without ever being remapped.
pub fn new_id() -> String {
    let mut random = [0_u8; 10];

    for byte in &mut random {
        *byte = (js_sys::Math::random() * 256.0) as u8;
    }

    aurum_core::ids::uuidv7(now_ms(), random)
}

/// Whether the device believes it has a connection. It is a hint, not a fact — a request that
/// fails says more — but it is enough to keep a sync tick from starting on a plane.
pub fn online() -> bool {
    web_sys::window()
        .map(|window| window.navigator().on_line())
        .unwrap_or(true)
}
