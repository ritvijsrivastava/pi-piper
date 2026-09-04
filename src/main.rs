//! Piper: a small Rust bridge that exposes a `pi` coding-agent RPC session
//! over a WebSocket, so it can be controlled remotely (e.g. from a phone
//! browser) over a private Tailscale network.
//!
//! See `README.md` for the architecture overview and setup instructions.

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use piper::config::Config;
use piper::rpc::process::PiProcess;
use piper::server;
use piper::state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::parse();
    let pi = Arc::new(PiProcess::spawn(&config)?);
    let state = AppState::new(pi.clone(), config.token.clone());

    let listener = TcpListener::bind(config.bind).await?;
    tracing::info!(addr = %config.bind, "piper listening");

    let app = server::router(state);

    // Run the server until it errors, the operator hits Ctrl+C, or the
    // underlying `pi` process exits unexpectedly (see rpc::process for why
    // a crash is treated as fatal here rather than restarted in-process).
    tokio::select! {
        result = axum::serve(listener, app) => {
            result?;
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received ctrl-c, shutting down");
        }
        status = pi.wait() => {
            tracing::error!(?status, "pi process exited unexpectedly, shutting down");
        }
    }

    pi.shutdown().await?;
    Ok(())
}
