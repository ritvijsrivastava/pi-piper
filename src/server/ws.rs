//! WebSocket endpoint that bridges a browser connection to the `pi` RPC
//! process's stdin/stdout.
//!
//! Messages are relayed as opaque JSON in both directions: the browser
//! speaks pi's own RPC protocol directly (see pi's `docs/rpc.md`), so
//! Piper does not define or maintain a second protocol. Piper only
//! validates that a message is well-formed JSON before forwarding it.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::broadcast::error::RecvError;

use super::auth;
use crate::state::AppState;

/// Query parameters accepted on the WebSocket upgrade request.
#[derive(Deserialize)]
pub struct AuthQuery {
    token: Option<String>,
}

/// Validates the shared-secret token, then upgrades the connection and
/// starts bridging it to the `pi` process.
pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<AuthQuery>,
) -> Response {
    if !auth::is_authorized(&state.auth_token, query.token.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "missing or invalid token").into_response();
    }
    ws.on_upgrade(move |socket| bridge(socket, state))
}

/// Relays `pi` events to the socket and socket messages to `pi`,
/// concurrently, until either side disconnects.
async fn bridge(socket: WebSocket, state: AppState) {
    let (mut to_client, mut from_client) = socket.split();
    let mut events = state.pi.subscribe();

    // pi -> browser
    let forward_events = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    let text = event.to_string();
                    if to_client.send(Message::Text(text.into())).await.is_err() {
                        // Browser disconnected; stop forwarding.
                        break;
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "websocket client fell behind, dropped events");
                }
                Err(RecvError::Closed) => break,
            }
        }
    });

    // browser -> pi
    while let Some(Ok(message)) = from_client.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        match serde_json::from_str::<Value>(&text) {
            Ok(command) => {
                if let Err(err) = state.pi.send(&command).await {
                    tracing::warn!(%err, "failed to forward command to pi");
                }
            }
            Err(err) => tracing::warn!(%err, "ignoring non-JSON message from client"),
        }
    }

    forward_events.abort();
}
