//! `/ws?session=<id>`: bridges a phone WebSocket connection to one
//! registered session's RPC stream.
//!
//! Messages are relayed as opaque JSON in both directions: the browser
//! speaks pi's own RPC protocol directly (see pi's `docs/rpc.md`), so
//! Piper does not define or maintain a second protocol. Piper only
//! validates that a message is well-formed JSON before forwarding it.
//! Unchanged from Piper v1 other than session selection (`SPEC.md` §6.4).

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;

use super::auth;
use crate::session::SessionHandle;
use crate::state::AppState;

/// Query parameters accepted on the WebSocket upgrade request.
#[derive(Deserialize)]
pub struct WsQuery {
    /// Which registered session to bridge to. May be omitted only when
    /// exactly one session is registered (see `SPEC.md` §6.4).
    session: Option<String>,
}

/// Checks the caller arrived over Tailscale, resolves the target
/// session *scoped to the caller's login* (owned sessions are visible
/// only to their owner, see `SessionMeta::owner`), then upgrades the
/// connection and starts bridging it.
pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(login) = auth::tailscale_login(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            "not authorized: connect over Tailscale",
        )
            .into_response();
    };

    let handle = match &query.session {
        Some(id) => state.registry.get_authorized(id, &login),
        None => state.registry.get_default_for(&login),
    };

    let Some(handle) = handle else {
        // A session that exists but belongs to someone else is reported
        // as 403, not folded into 404: the caller is on a trusted
        // tailnet, and "you don't own that" is far more debuggable than
        // a misleading "no such session".
        let (status, hint) = match &query.session {
            Some(id) => {
                if state.registry.get(id).is_some() {
                    (
                        StatusCode::FORBIDDEN,
                        "session belongs to another user".to_string(),
                    )
                } else {
                    (
                        StatusCode::BAD_REQUEST,
                        "no session registered with that id".to_string(),
                    )
                }
            }
            None => (
                StatusCode::BAD_REQUEST,
                "no session id given and it is not the only one visible to you; GET /api/sessions and pass ?session=<id>".to_string(),
            ),
        };
        return (status, hint).into_response();
    };

    ws.on_upgrade(move |socket| bridge(socket, handle))
}

/// Relays session events to the socket and socket messages to the
/// session, concurrently, until either side disconnects or the session
/// itself disconnects (e.g. the terminal running `piper-agent` closed).
async fn bridge(socket: WebSocket, handle: Arc<SessionHandle>) {
    let (mut to_client, mut from_client) = socket.split();
    let mut events = handle.link.subscribe();
    let mut connected = handle.connected_rx();

    let forward_events = tokio::spawn(async move {
        loop {
            tokio::select! {
                changed = connected.changed() => {
                    if changed.is_err() || !*connected.borrow() {
                        break;
                    }
                }
                event = events.recv() => {
                    match event {
                        Ok(event) => {
                            let text = event.to_string();
                            if to_client.send(Message::Text(text.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "websocket client fell behind, dropped events");
                        }
                        Err(RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    while let Some(Ok(message)) = from_client.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        match serde_json::from_str::<Value>(&text) {
            Ok(command) => {
                if let Err(err) = handle.link.send(&command).await {
                    tracing::warn!(%err, "failed to forward command to session");
                }
            }
            Err(err) => tracing::warn!(%err, "ignoring non-JSON message from client"),
        }
    }

    forward_events.abort();
}
