//! Piper: a multi-session remote control hub for `pi` coding-agent
//! sessions, so they can be controlled remotely (e.g. from a phone
//! browser) over a private Tailscale network.
//!
//! See `README.md` for setup and `SPEC.md` for the full architecture.

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use piper::agent_token;
use piper::config::Config;
use piper::registry::SessionRegistry;
use piper::rpc::process::PiProcess;
use piper::server;
use piper::session::{now_ms, AgentLink, SessionKind, SessionMeta};
use piper::state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::parse();

    let agent_token = match &config.agent_token {
        Some(token) => token.clone(),
        None => {
            let path = config
                .agent_token_path
                .clone()
                .unwrap_or_else(agent_token::default_path);
            agent_token::load_or_create(&path)?
        }
    };

    let registry = Arc::new(SessionRegistry::new());

    // Optional headless (v1-compatible) session: Piper spawns `pi
    // --mode rpc` itself for a project with no terminal open. See
    // `SPEC.md` §10. Kept alive here so we can shut it down cleanly on
    // Ctrl+C; its own lifecycle otherwise just unregisters itself from
    // the registry if it exits.
    let headless_pi: Option<Arc<PiProcess>> = if let Some(project_dir) = &config.project_dir {
        let pi = Arc::new(PiProcess::spawn(&config)?);
        let session_id = uuid::Uuid::new_v4().to_string();
        let meta = SessionMeta {
            session_id: session_id.clone(),
            session_file: None,
            session_name: None,
            cwd: project_dir.display().to_string(),
            connected_at_ms: now_ms(),
            kind: SessionKind::Headless,
        };
        registry.register(session_id.clone(), AgentLink::Headless(pi.clone()), meta);

        // Headless sessions have no extension to tell us when the
        // underlying `pi` exits, so watch it directly and unregister.
        let watch_registry = registry.clone();
        let watch_pi = pi.clone();
        tokio::spawn(async move {
            let status = watch_pi.wait().await;
            tracing::warn!(
                ?status,
                "headless pi process exited, removing from registry"
            );
            watch_registry.unregister(&session_id);
        });

        Some(pi)
    } else {
        None
    };

    let state = AppState::new(
        registry,
        config.token.clone(),
        agent_token,
        config.allowed_tailscale_logins.clone(),
    );

    let listener = TcpListener::bind(config.bind).await?;
    tracing::info!(addr = %config.bind, "piper listening");

    let app = server::router(state);

    tokio::select! {
        result = axum::serve(listener, app) => {
            result?;
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received ctrl-c, shutting down");
        }
    }

    if let Some(pi) = headless_pi {
        pi.shutdown().await.ok();
    }

    Ok(())
}
