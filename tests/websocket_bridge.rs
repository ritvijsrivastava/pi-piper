//! End-to-end test: spawns a real `pi --mode rpc` process, serves the
//! real axum router over a real TCP socket, and drives it with a real
//! WebSocket client. Uses `get_state`, which never calls the configured
//! LLM, so this test has no external cost and needs no model credentials
//! beyond whatever `pi` is already configured to use locally.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use piper::config::Config;
use piper::rpc::process::PiProcess;
use piper::server;
use piper::state::AppState;

const TOKEN: &str = "integration-test-token";

fn test_config() -> Config {
    Config {
        pi_command: "pi".to_string(),
        project_dir: PathBuf::from("."),
        session: None,
        no_session: true,
        extra_pi_args: Vec::new(),
        bind: "127.0.0.1:0".parse().unwrap(),
        token: TOKEN.to_string(),
    }
}

/// Spawns the real server on an ephemeral port and returns its address
/// plus the `PiProcess` handle (kept alive for the duration of the test).
async fn spawn_test_server() -> (SocketAddr, Arc<PiProcess>) {
    let config = test_config();
    let pi = Arc::new(PiProcess::spawn(&config).expect("spawn pi process"));
    let state = AppState::new(pi.clone(), config.token.clone());

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
async fn rejects_connection_without_valid_token() {
    let (addr, pi) = spawn_test_server().await;

    let result = connect_async(format!("ws://{addr}/ws")).await;
    assert!(
        result.is_err(),
        "connection without a token must be rejected"
    );

    let result = connect_async(format!("ws://{addr}/ws?token=wrong")).await;
    assert!(
        result.is_err(),
        "connection with a wrong token must be rejected"
    );

    pi.shutdown().await.ok();
}

#[tokio::test]
async fn relays_get_state_round_trip() {
    let (addr, pi) = spawn_test_server().await;

    let (mut socket, _response) = connect_async(format!("ws://{addr}/ws?token={TOKEN}"))
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
