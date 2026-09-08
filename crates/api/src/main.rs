//! One binary: the HTTP API, the signalling relay and the console commands.
//!
//! Nothing here holds a rule of its own — the rules live in `aurum-core`; this crate is the
//! shell that gives them a socket, a database and an object store.

use aurum_api::config::Config;
use aurum_api::routes::router;
use aurum_api::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .init();

    let config = Config::from_env();
    let bind = config.bind.clone();
    let state = AppState::build(config).await?;

    // Applied at startup rather than lazily: a deployment finds out its database cannot be
    // opened when it starts, not on the first request that happens to need it.
    state.db.migrate_control()?;

    let listener = tokio::net::TcpListener::bind(&bind).await?;

    tracing::info!("listening on {bind}");

    axum::serve(listener, router(state)).await?;

    Ok(())
}
