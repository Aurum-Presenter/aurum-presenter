//! The Aurum Presenter client.
//!
//! Client-rendered, deliberately: the app has to start from a precached shell with the radio
//! off, which rules out server-side rendering and hydration. Every rule it obeys comes from
//! `aurum-core`, which the server links too — so a chart transposes the same way on both sides
//! of the wire because it is the same code.
#![forbid(unsafe_code)]

pub mod api;
pub mod app;
pub mod db;
