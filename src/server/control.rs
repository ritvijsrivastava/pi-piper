//! `/ws/control`: push channel feeding the phone's session list screen.
//! See `SPEC.md` §6.3. Purely Hub-authored; the only thing the phone
//! ever sends here is an optional ping/close.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use super::auth;
use crate::state::AppState;

pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if !auth::is_authorized_tailscale(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            "not authorized: connect over Tailscale",
        )
            .into_response();
    }
    ws.on_upgrade(move |socket| handle(socket, state))
}

async fn handle(socket: WebSocket, state: AppState) {
    let (mut sink, mut stream) = socket.split();
    let mut control_events = state.registry.control_subscribe();

    let snapshot = json!({
        "type": "sessions_snapshot",
        "sessions": state.registry.list(),
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
