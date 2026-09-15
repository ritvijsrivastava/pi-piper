//! Shared application state handed to every axum request/WebSocket handler.

use std::sync::Arc;

use crate::registry::SessionRegistry;

/// State shared across all HTTP and WebSocket connections.
///
/// Cheap to clone: it only clones `Arc`s and reference-counted strings,
/// never the registry contents or the tokens.
#[derive(Clone)]
pub struct AppState {
    /// Every currently-registered session (headless-spawned or
    /// `piper-agent`-connected).
    pub registry: Arc<SessionRegistry>,
    /// Shared secret `piper-agent` extensions present as `?token=` to
    /// `/agent`. Never sent to the phone. The
    /// phone-facing routes (`/ws`, `/ws/control`, `/api/sessions`) have
    /// no analogous secret: they authorize any request carrying a
    /// `Tailscale-User-Login` identity header, see
    /// `server::auth::is_authorized_tailscale`.
    pub agent_token: Arc<str>,
}

impl AppState {
    pub fn new(registry: Arc<SessionRegistry>, agent_token: String) -> Self {
        Self {
            registry,
            agent_token: Arc::from(agent_token),
        }
    }
}
