//! `/agent`: `pi-piper-agent` extensions connect here to register a live
//! interactive `pi` session for remote control. The registration
//! envelope is documented in `ARCHITECTURE.md`.
//!
//! This endpoint is agent-token gated — a shared
//! secret never sent to the phone, unlike `/ws`, `/ws/control`, and
//! `/api/sessions`, which authorize any request arriving over
//! Tailscale instead. Everything after the upgrade is the tiny registration envelope
//! defined in `ARCHITECTURE.md`; the pi RPC JSON it carries is passed
//! through unwrapped into the session's own broadcast channel via
//! `SessionRegistry::publish`.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::auth;
use crate::rpc::remote_agent::RemoteAgent;
use crate::session::{now_ms, AgentLink, SessionKind, SessionMeta};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct AgentAuthQuery {
    token: Option<String>,
}

pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<AgentAuthQuery>,
) -> Response {
    if !auth::is_authorized(&state.agent_token, query.token.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "missing or invalid agent token").into_response();
    }
    ws.on_upgrade(move |socket| handle(socket, state))
}

async fn handle(socket: WebSocket, state: AppState) {
    let (mut to_client, mut from_client) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Drains outbound envelope messages (registration ack + commands)
    // onto the real WebSocket sink.
    let writer = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if to_client.send(message).await.is_err() {
                break;
            }
        }
    });

    let mut session_id: Option<String> = None;

    while let Some(Ok(message)) = from_client.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(frame) = serde_json::from_str::<Value>(&text) else {
            tracing::warn!("ignoring non-JSON frame from agent connection");
            continue;
        };

        match frame.get("type").and_then(Value::as_str) {
            Some("register") => {
                let Some(id) = frame.get("sessionId").and_then(Value::as_str) else {
                    tracing::warn!("ignoring register frame with no sessionId");
                    continue;
                };

                let meta = SessionMeta {
                    session_id: id.to_string(),
                    session_file: text_field(&frame, "sessionFile"),
                    session_name: text_field(&frame, "sessionName"),
                    cwd: text_field(&frame, "cwd").unwrap_or_default(),
                    connected_at_ms: now_ms(),
                    kind: SessionKind::Remote,
                };

                let remote_agent = std::sync::Arc::new(RemoteAgent::new(tx.clone()));
                state
                    .registry
                    .register(id.to_string(), AgentLink::Remote(remote_agent), meta);

                session_id = Some(id.to_string());
                let _ = tx.send(Message::Text(
                    json!({"type": "registered", "sessionId": id})
                        .to_string()
                        .into(),
                ));
            }
            Some("meta_update") => {
                if let (Some(id), Some(patch)) = (&session_id, frame.get("patch")) {
                    state.registry.update_meta(id, patch);
                }
            }
            Some("event") => {
                if let (Some(id), Some(event)) = (&session_id, frame.get("event")) {
                    state.registry.publish(id, event.clone());
                }
            }
            Some("response") => {
                if let (Some(id), Some(response)) = (&session_id, frame.get("response")) {
                    state.registry.publish(id, response.clone());
                }
            }
            other => tracing::warn!(?other, "unknown /agent frame type"),
        }
    }

    if let Some(id) = &session_id {
        state.registry.unregister(id);
    }
    writer.abort();
}

fn text_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}
