//! One module per group of endpoints. Nothing here decides a rule — handlers read a request,
//! call `aurum-core` or a repository, and shape the answer.

pub mod account;
pub mod auth;
pub mod files;
pub mod sync;
pub mod workspaces;
