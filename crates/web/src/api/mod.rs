//! Talking to the server, and the one credential the app holds in memory.

pub mod account;
pub mod client;
pub mod models;

pub use client::{Api, ApiError, RestoreResult};
pub use models::{Account, Workspace};
