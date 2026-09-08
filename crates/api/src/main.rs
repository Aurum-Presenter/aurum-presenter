//! The API server.
//!
//! One binary: the HTTP API, the signalling relay and the console commands. Nothing here holds
//! any rule of its own — the rules live in `aurum-core`; this crate is the shell that gives them
//! a socket, a database and an object store.
use axum::{routing::get, Json, Router};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let bind = std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let listener = tokio::net::TcpListener::bind(&bind).await?;

    tracing::info!("listening on {bind}");

    axum::serve(listener, routes()).await?;

    Ok(())
}

fn routes() -> Router {
    Router::new().route("/api/v1/health", get(health))
}

/// The same answer the PHP server gives, because the contract is what is being kept.
async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "core": aurum_core::version() }))
}
