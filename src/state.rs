//! Shared application state handed to every axum request/WebSocket handler.

use std::sync::Arc;

use crate::rpc::process::PiProcess;

/// State shared across all HTTP and WebSocket connections.
///
/// Cheap to clone: it only clones an `Arc` and a reference-counted string,
/// never the underlying process or token.
#[derive(Clone)]
pub struct AppState {
    /// The single `pi` RPC process this Piper instance controls.
    pub pi: Arc<PiProcess>,
    /// Shared secret required to open a WebSocket connection.
    pub auth_token: Arc<str>,
}

impl AppState {
    pub fn new(pi: Arc<PiProcess>, auth_token: String) -> Self {
        Self {
            pi,
            auth_token: Arc::from(auth_token),
        }
    }
}
