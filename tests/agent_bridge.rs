//! End-to-end test of the `/agent` <-> `/ws` <-> `/ws/control` <->
//! `/api/sessions` machinery introduced for `pi-piper-agent`. Does not
//! spawn a real `pi` process or use a real `pi-piper-agent`
//! extension: a plain WebSocket client stands in for the extension,
//! since the wire protocol on `/agent` is deliberately small and
//! transport-only (see `ARCHITECTURE.md`).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use pi_piper::registry::SessionRegistry;
use pi_piper::server;
use pi_piper::state::AppState;

const AGENT_TOKEN: &str = "agent-token";

/// Tailnet login stamped on requests that pretend to have been proxied
/// in by `tailscale serve`, which is the only
/// thing that authorizes `/ws` and `/ws/control`.
const TAILSCALE_LOGIN: &str = "test-user@github";

/// Builds a WebSocket upgrade request carrying a `Tailscale-User-Login`
/// header, standing in for what `tailscale serve` would stamp on a real
/// proxied request.
fn authorized_request(url: &str) -> tokio_tungstenite::tungstenite::handshake::client::Request {
    let mut request = url.into_client_request().expect("valid ws url");
    request
        .headers_mut()
        .insert("Tailscale-User-Login", TAILSCALE_LOGIN.parse().unwrap());
    request
}

async fn spawn_hub() -> SocketAddr {
    let registry = Arc::new(SessionRegistry::new());
    let state = AppState::new(registry, AGENT_TOKEN.to_string());

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
                "cwd": "/home/user/Code/pi-piper",
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
    assert_eq!(client["cwd"], "/home/user/Code/pi-piper");
    assert_eq!(client["sessionName"], "Refactor auth module");
    assert_eq!(client["connected"], true);

    // Control channel's initial snapshot sees it too.
    let (mut control, _) = connect_async(authorized_request(&format!("ws://{addr}/ws/control")))
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

    let (mut phone, _) = connect_async(authorized_request(&format!(
        "ws://{addr}/ws?session=sess-2"
    )))
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
async fn duplicate_registration_closes_the_previous_connection() {
    let addr = spawn_hub().await;

    let (mut first, _) = connect_async(format!("ws://{addr}/agent?token={AGENT_TOKEN}"))
        .await
        .expect("first agent connects");
    first
        .send(Message::Text(
            json!({"type": "register", "sessionId": "sess-dup", "cwd": "/tmp/a"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let _ack = recv_json(&mut first).await;

    // A second connection registers under the *same* sessionId (e.g.
    // the same session file resumed in two terminals at once). The
    // first connection must be told to close rather than left as a
    // zombie — see `SessionRegistry::register`.
    let (mut second, _) = connect_async(format!("ws://{addr}/agent?token={AGENT_TOKEN}"))
        .await
        .expect("second agent connects");
    second
        .send(Message::Text(
            json!({"type": "register", "sessionId": "sess-dup", "cwd": "/tmp/b"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let _ack = recv_json(&mut second).await;

    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match first.next().await {
                Some(Ok(Message::Close(_))) | None => return true,
                Some(Ok(_)) => continue,
                Some(Err(_)) => return true,
            }
        }
    })
    .await
    .expect("timed out waiting for the first connection to be closed");
    assert!(closed, "the superseded connection should have been closed");

    // The surviving registration reflects the second connection's meta.
    let sessions: Value = reqwest::Client::new()
        .get(format!("http://{addr}/api/sessions"))
        .header("Tailscale-User-Login", TAILSCALE_LOGIN)
        .send()
        .await
        .expect("request /api/sessions")
        .json()
        .await
        .expect("valid json body");
    let entry = sessions
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["sessionId"] == "sess-dup")
        .expect("sess-dup present");
    assert_eq!(entry["cwd"], "/tmp/b");
    assert_eq!(entry["connected"], true);
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

    let (mut control, _) = connect_async(authorized_request(&format!("ws://{addr}/ws/control")))
        .await
        .expect("control connects");
    let _snapshot = recv_json(&mut control).await;

    agent.close(None).await.unwrap();
    drop(agent);

    let update = recv_json(&mut control).await;
    assert_eq!(update["type"], "session_disconnected");
    assert_eq!(update["sessionId"], "sess-3");
}

#[tokio::test]
async fn sessions_api_accepts_any_tailscale_identity_header() {
    let addr = spawn_hub().await;

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/sessions"))
        .header("Tailscale-User-Login", "anybody@example.com")
        .send()
        .await
        .expect("request /api/sessions");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn sessions_api_rejects_missing_tailscale_identity_header() {
    let addr = spawn_hub().await;

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/sessions"))
        .send()
        .await
        .expect("request /api/sessions");
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sessions_api_rejects_tailscale_identity_header_on_funnel_requests() {
    let addr = spawn_hub().await;

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/sessions"))
        .header("Tailscale-User-Login", "anybody@example.com")
        .header("Tailscale-Funnel-Request", "?1")
        .send()
        .await
        .expect("request /api/sessions");
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

/// Minimal HTTP GET without pulling in `reqwest` as a dev-dependency:
/// finds `sess-1`'s entry from `/api/sessions`.
async fn fetch_session(addr: &SocketAddr, session_id: &str) -> Value {
    let sessions: Value = reqwest::Client::new()
        .get(format!("http://{addr}/api/sessions"))
        .header("Tailscale-User-Login", TAILSCALE_LOGIN)
        .send()
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
