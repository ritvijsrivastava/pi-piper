//! End-to-end test: spawns a real `pi --mode rpc` process registered as
//! a headless session, serves the real axum router over a real TCP
//! socket, and drives it with a real WebSocket client. Uses
//! `get_state`, which never calls the configured LLM, so this test has
//! no external cost and needs no model credentials beyond whatever `pi`
//! is already configured to use locally.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use pi_piper::config::Config;
use pi_piper::registry::SessionRegistry;
use pi_piper::rpc::process::PiProcess;
use pi_piper::server;
use pi_piper::session::{now_ms, AgentLink, SessionKind, SessionMeta};
use pi_piper::state::AppState;

/// Builds a WebSocket upgrade request that looks like it was proxied in
/// by `tailscale serve` for tailnet user `test-user@github` — the only
/// thing that authorizes `/ws` now that there
/// is no phone token.
fn authorized_request(url: &str) -> tokio_tungstenite::tungstenite::handshake::client::Request {
    let mut request = url.into_client_request().expect("valid ws url");
    request
        .headers_mut()
        .insert("Tailscale-User-Login", "test-user@github".parse().unwrap());
    request
}

fn test_config() -> Config {
    Config {
        pi_command: "pi".to_string(),
        project_dir: Some(PathBuf::from(".")),
        session: None,
        no_session: true,
        extra_pi_args: Vec::new(),
        bind: "127.0.0.1:0".parse().unwrap(),
        agent_token: Some("unused-in-this-test".to_string()),
        agent_token_path: None,
    }
}

/// Spawns the real server on an ephemeral port, with one headless
/// session registered, and returns its address plus the `PiProcess`
/// handle (kept alive for the duration of the test).
async fn spawn_test_server() -> (SocketAddr, Arc<PiProcess>) {
    let config = test_config();
    let pi = Arc::new(PiProcess::spawn(&config).expect("spawn pi process"));

    let registry = Arc::new(SessionRegistry::new());
    let meta = SessionMeta {
        session_id: "test-session".to_string(),
        session_file: None,
        session_name: None,
        cwd: config.project_dir.clone().unwrap().display().to_string(),
        connected_at_ms: now_ms(),
        kind: SessionKind::Headless,
    };
    registry.register(
        "test-session".to_string(),
        AgentLink::Headless(pi.clone()),
        meta,
    );

    let state = AppState::new(registry, "unused".to_string());

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");

    let app = server::router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server error");
    });

    // Give the spawned server task a moment to start accepting.
    tokio::time::sleep(Duration::from_millis(50)).await;

    (addr, pi)
}

#[tokio::test]
async fn rejects_connection_without_tailscale_identity_header() {
    let (addr, pi) = spawn_test_server().await;

    let result = connect_async(format!("ws://{addr}/ws")).await;
    assert!(
        result.is_err(),
        "connection with no Tailscale-User-Login header must be rejected"
    );

    pi.shutdown().await.ok();
}

#[tokio::test]
async fn relays_get_state_round_trip() {
    let (addr, pi) = spawn_test_server().await;

    // No `?session=` needed: exactly one session is registered, so `/ws`
    // defaults to it.
    let (mut socket, _response) = connect_async(authorized_request(&format!("ws://{addr}/ws")))
        .await
        .expect("authorized connection must succeed");

    socket
        .send(Message::Text(
            json!({"type": "get_state"}).to_string().into(),
        ))
        .await
        .expect("send get_state");

    // pi may emit unrelated events first (e.g. extension `setStatus`/
    // `setWidget` requests), so scan until the `get_state` response shows
    // up rather than assuming it's the very first message.
    let value = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let message = socket
                .next()
                .await
                .expect("stream ended unexpectedly")
                .expect("websocket error");
            let Message::Text(text) = message else {
                continue;
            };
            let value: Value = serde_json::from_str(&text).expect("response must be valid JSON");
            if value["type"] == "response" && value["command"] == "get_state" {
                return value;
            }
        }
    })
    .await
    .expect("did not receive the get_state response in time");

    assert_eq!(value["success"], true);

    pi.shutdown().await.ok();
}

#[tokio::test]
async fn rejects_ws_when_session_id_required_but_missing() {
    // Two sessions registered -> `/ws` without `?session=` must be
    // rejected rather than guessing.
    let config = test_config();
    let pi_a = Arc::new(PiProcess::spawn(&config).expect("spawn pi process a"));
    let pi_b = Arc::new(PiProcess::spawn(&config).expect("spawn pi process b"));

    let registry = Arc::new(SessionRegistry::new());
    for (id, pi) in [("session-a", pi_a.clone()), ("session-b", pi_b.clone())] {
        let meta = SessionMeta {
            session_id: id.to_string(),
            session_file: None,
            session_name: None,
            cwd: ".".to_string(),
            connected_at_ms: now_ms(),
            kind: SessionKind::Headless,
        };
        registry.register(id.to_string(), AgentLink::Headless(pi), meta);
    }

    let state = AppState::new(registry, "unused".to_string());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = server::router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server error");
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = connect_async(authorized_request(&format!("ws://{addr}/ws"))).await;
    assert!(
        result.is_err(),
        "must reject an ambiguous /ws request when multiple sessions are registered"
    );

    pi_a.shutdown().await.ok();
    pi_b.shutdown().await.ok();
}
