//! The Aurum Presenter API.
//!
//! A shell around `aurum-core`: this crate owns sockets, SQLite files, an object store and an
//! SMTP connection, and holds no domain rule of its own. Where a rule appears to live here, it
//! is a rule about HTTP — a status code, a cookie's lifetime — and not about music.
#![forbid(unsafe_code)]

pub mod auth;
pub mod cli;
pub mod config;
pub mod db;
pub mod error;
pub mod extract;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod signal;
pub mod state;
pub mod storage;
