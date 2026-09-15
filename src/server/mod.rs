//! HTTP + WebSocket server: serves the mobile PWA and bridges browser
//! and `piper-agent` WebSocket connections to the session registry.

mod agent;
mod auth;
mod control;
mod sessions_api;
mod static_files;
mod ws;

use axum::routing::get;
use axum::Router;

use crate::state::AppState;

/// Builds the full application router:
/// - `/agent`: `piper-agent` extensions register live sessions here
///   (agent-token gated, see `SPEC.md` §6.1/§7).
/// - `/ws`: phone bridge to one session's RPC stream (Tailscale-identity
///   gated, see `SPEC.md` §7; unchanged from Piper v1 other than the
///   `?session=` param).
/// - `/ws/control`: phone push channel for the session list screen.
/// - `/api/sessions`: HTTP snapshot of the session list.
/// - everything else: the embedded PWA.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/agent", get(agent::handler))
        .route("/ws", get(ws::handler))
        .route("/ws/control", get(control::handler))
        .route("/api/sessions", get(sessions_api::handler))
        .fallback(static_files::handler)
        .with_state(state)
}
