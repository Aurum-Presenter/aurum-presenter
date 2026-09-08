//! One binary: the HTTP API, the signalling relay and the console commands.
//!
//! Nothing here holds a rule of its own — the rules live in `aurum-core`; this crate is the
//! shell that gives them a socket, a database and an object store.

use aurum_api::cli::{Cli, Command};
use aurum_api::config::Config;
use aurum_api::routes::router;
use aurum_api::state::AppState;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .init();

    let config = Config::from_env();

    match Cli::parse().command.unwrap_or(Command::Serve) {
        Command::Serve => serve(config).await,
        other => aurum_api::cli::run(other, config).await,
    }
}

async fn serve(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let (bind, signal_bind) = (config.bind.clone(), config.signal_bind.clone());
    let state = AppState::build(config).await?;

    // Applied at startup rather than lazily: a deployment finds out its database cannot be
    // opened when it starts, not on the first request that happens to need it.
    state.db.migrate_control()?;

    // The relay is a second listener in the same process, not a second binary. PHP-FPM could not
    // hold a socket open; a tokio task can, and one deployment artefact is one fewer thing to
    // start.
    {
        let state = state.clone();

        tokio::spawn(async move {
            if let Err(error) = aurum_api::signal::serve(state, &signal_bind).await {
                tracing::error!("the signalling relay stopped: {error}");
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(&bind).await?;

    tracing::info!("listening on {bind}");

    axum::serve(listener, router(state)).await?;

    Ok(())
}
