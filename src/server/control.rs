//! `/ws/control`: push channel feeding the phone's session list screen.
//! See `SPEC.md` §6.3. Purely Hub-authored; the only thing the phone
//! ever sends here is an optional ping/close.
//!
//! Per-viewer scoped: the snapshot and every streamed registry event
//! are filtered against the caller's `Tailscale-User-Login`, so a user
//! is never told about sessions they could not attach to (see
//! `SessionMeta::owner`). The single broadcast channel still carries
//! all events to all viewers — the filtering happens here, per client.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::broadcast::error::RecvError;

use super::auth;
use crate::state::AppState;

pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let Some(login) = auth::tailscale_login(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            "not authorized: connect over Tailscale",
        )
            .into_response();
    };
    ws.on_upgrade(move |socket| handle(socket, state, login))
}

async fn handle(socket: WebSocket, state: AppState, login: String) {
    let (mut sink, mut stream) = socket.split();
    let mut control_events = state.registry.control_subscribe();

    let snapshot = json!({
        "type": "sessions_snapshot",
        "sessions": state.registry.visible_to(&login),
    });
    if sink
        .send(Message::Text(snapshot.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    // The phone doesn't send meaningful commands on this channel; just
    // drain (and discard) whatever it sends so the socket stays alive
    // and we notice a close promptly, while the real work happens in
    // the loop below.
    let discard_inbound = tokio::spawn(async move { while stream.next().await.is_some() {} });

    loop {
        match control_events.recv().await {
            Ok(event) => {
                if !is_visible_event(&event, &login) {
                    continue;
                }
                if sink
                    .send(Message::Text(event.to_string().into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    skipped,
                    "control channel client fell behind, dropped events"
                );
            }
            Err(RecvError::Closed) => break,
        }
    }

    discard_inbound.abort();
}

/// True if `event` concerns a session `login` is allowed to see. Owned
/// sessions reach everyone on the broadcast channel, so each control
/// client filters before forwarding: `session_connected`,
/// `session_meta` and `session_update` carry the session's summary
/// (`session.owner`), while `session_disconnected` carries the owner
/// top-level (`owner` — the session is already unregistered by then).
/// Unowned sessions are visible to everyone (see `SessionMeta::owner`).
fn is_visible_event(event: &Value, login: &str) -> bool {
    let owner = event
        .get("session")
        .and_then(|s| s.get("owner"))
        .or_else(|| event.get("owner"));
    match owner {
        None | Some(Value::Null) => true,
        Some(Value::String(owner)) => owner == login,
        Some(_) => true,
    }
}
