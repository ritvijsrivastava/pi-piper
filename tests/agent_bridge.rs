//! End-to-end test of the `/agent` <-> `/ws` <-> `/ws/control` <->
//! `/api/sessions` machinery introduced for `piper-agent` (`SPEC.md`
//! §6). Does not spawn a real `pi` process or use a real `piper-agent`
//! extension: a plain WebSocket client stands in for the extension,
//! since the wire protocol on `/agent` is deliberately small and
//! transport-only (see `SPEC.md` §6.1).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use piper::registry::SessionRegistry;
use piper::server;
use piper::state::AppState;

const PHONE_TOKEN: &str = "phone-token";
const AGENT_TOKEN: &str = "agent-token";

async fn spawn_hub() -> SocketAddr {
    let registry = Arc::new(SessionRegistry::new());
    let state = AppState::new(registry, PHONE_TOKEN.to_string(), AGENT_TOKEN.to_string());

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = server::router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server error");
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    addr
}

async fn recv_json(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let message = socket
                .next()
                .await
                .expect("stream ended")
                .expect("ws error");
            if let Message::Text(text) = message {
                return serde_json::from_str(&text).expect("valid json");
            }
        }
    })
    .await
    .expect("timed out waiting for a message")
}

#[tokio::test]
async fn agent_endpoint_rejects_wrong_token() {
    let addr = spawn_hub().await;
    let result = connect_async(format!("ws://{addr}/agent?token=wrong")).await;
    assert!(result.is_err(), "must reject a bad agent token");
}

#[tokio::test]
async fn register_appears_in_sessions_api_and_control_snapshot() {
    let addr = spawn_hub().await;

    let (mut agent, _) = connect_async(format!("ws://{addr}/agent?token={AGENT_TOKEN}"))
        .await
        .expect("agent connects");

    agent
        .send(Message::Text(
            json!({
                "type": "register",
                "sessionId": "sess-1",
                "cwd": "/home/user/Code/piper",
                "sessionName": "Refactor auth module",
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let ack = recv_json(&mut agent).await;
    assert_eq!(ack["type"], "registered");
    assert_eq!(ack["sessionId"], "sess-1");

    // HTTP snapshot sees it.
    let client = fetch_session(&addr, "sess-1").await;
    assert_eq!(client["cwd"], "/home/user/Code/piper");
    assert_eq!(client["sessionName"], "Refactor auth module");
    assert_eq!(client["connected"], true);

    // Control channel's initial snapshot sees it too.
    let (mut control, _) = connect_async(format!("ws://{addr}/ws/control?token={PHONE_TOKEN}"))
        .await
        .expect("control connects");
    let snapshot = recv_json(&mut control).await;
    assert_eq!(snapshot["type"], "sessions_snapshot");
    let sessions = snapshot["sessions"].as_array().unwrap();
    assert!(sessions.iter().any(|s| s["sessionId"] == "sess-1"));
}

#[tokio::test]
async fn command_from_phone_is_relayed_to_agent_and_response_back_to_phone() {
    let addr = spawn_hub().await;

    let (mut agent, _) = connect_async(format!("ws://{addr}/agent?token={AGENT_TOKEN}"))
        .await
        .expect("agent connects");
    agent
        .send(Message::Text(
            json!({"type": "register", "sessionId": "sess-2", "cwd": "/tmp"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let _ack = recv_json(&mut agent).await;

    let (mut phone, _) =
        connect_async(format!("ws://{addr}/ws?token={PHONE_TOKEN}&session=sess-2"))
            .await
            .expect("phone connects");

    phone
        .send(Message::Text(
            json!({"type": "get_state"}).to_string().into(),
        ))
        .await
        .unwrap();

    // The Hub must wrap it in a command envelope for the agent side.
    let envelope = recv_json(&mut agent).await;
    assert_eq!(envelope["type"], "command");
    assert_eq!(envelope["command"]["type"], "get_state");
    let envelope_id = envelope["id"].as_str().unwrap().to_string();

    // The "extension" replies; the Hub must unwrap and relay it back to
    // the phone verbatim, matching pi's own RPC response shape.
    agent
        .send(Message::Text(
            json!({
                "type": "response",
                "id": envelope_id,
                "response": {"type": "response", "command": "get_state", "success": true, "data": {"messageCount": 0}},
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let response = recv_json(&mut phone).await;
    assert_eq!(response["type"], "response");
    assert_eq!(response["command"], "get_state");
    assert_eq!(response["success"], true);
}

#[tokio::test]
async fn disconnecting_agent_removes_session_and_notifies_control_channel() {
    let addr = spawn_hub().await;

    let (mut agent, _) = connect_async(format!("ws://{addr}/agent?token={AGENT_TOKEN}"))
        .await
        .expect("agent connects");
    agent
        .send(Message::Text(
            json!({"type": "register", "sessionId": "sess-3", "cwd": "/tmp"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let _ack = recv_json(&mut agent).await;

    let (mut control, _) = connect_async(format!("ws://{addr}/ws/control?token={PHONE_TOKEN}"))
        .await
        .expect("control connects");
    let _snapshot = recv_json(&mut control).await;

    agent.close(None).await.unwrap();
    drop(agent);

    let update = recv_json(&mut control).await;
    assert_eq!(update["type"], "session_disconnected");
    assert_eq!(update["sessionId"], "sess-3");
}

/// Minimal HTTP GET without pulling in `reqwest` as a dev-dependency:
/// finds `sess-1`'s entry from `/api/sessions`.
async fn fetch_session(addr: &SocketAddr, session_id: &str) -> Value {
    let sessions: Value = reqwest::get(format!("http://{addr}/api/sessions?token={PHONE_TOKEN}"))
        .await
        .expect("request /api/sessions")
        .json()
        .await
        .expect("valid json body");
    sessions
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["sessionId"] == session_id)
        .cloned()
        .expect("session present in /api/sessions")
}
