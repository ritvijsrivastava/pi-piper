//! HTTP + WebSocket server: serves the mobile PWA and bridges browser
//! WebSocket connections to the `pi` RPC process.

mod auth;
mod static_files;
mod ws;

use axum::routing::get;
use axum::Router;

use crate::state::AppState;

/// Builds the full application router: the WebSocket bridge at `/ws`, and
/// the embedded PWA for everything else.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/ws", get(ws::handler))
        .fallback(static_files::handler)
        .with_state(state)
}
