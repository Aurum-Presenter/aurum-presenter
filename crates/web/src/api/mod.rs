//! Talking to the server, and the one credential the app holds in memory.

pub mod client;

pub use client::{Api, ApiError, RestoreResult};
