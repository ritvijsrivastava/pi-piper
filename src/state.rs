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
    /// `piper-agent`-connected). See `SPEC.md` §5.1.
    pub registry: Arc<SessionRegistry>,
    /// Shared secret phone clients present as `?token=` to `/ws`,
    /// `/ws/control`, and `/api/sessions`.
    pub phone_token: Arc<str>,
    /// Shared secret `piper-agent` extensions present as `?token=` to
    /// `/agent`. Never sent to the phone. See `SPEC.md` §7.
    pub agent_token: Arc<str>,
    /// Optional allowlist of Tailscale logins allowed to authenticate to
    /// the phone-facing routes via the `Tailscale-User-Login` identity
    /// header instead of `phone_token`. Empty disables this path. See
    /// `SPEC.md` §7 and `server::auth::is_authorized_tailscale_identity`.
    pub allowed_tailscale_logins: Arc<[String]>,
}

impl AppState {
    pub fn new(
        registry: Arc<SessionRegistry>,
        phone_token: String,
        agent_token: String,
        allowed_tailscale_logins: Vec<String>,
    ) -> Self {
        Self {
            registry,
            phone_token: Arc::from(phone_token),
            agent_token: Arc::from(agent_token),
            allowed_tailscale_logins: Arc::from(allowed_tailscale_logins),
        }
    }
}
