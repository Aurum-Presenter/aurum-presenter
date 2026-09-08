//! The sync engine: every write lands locally first, and the screen never waits for a network.

pub mod engine;

pub use engine::SyncEngine;
