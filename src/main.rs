//! Piper: a small Rust bridge that exposes a `pi` coding-agent RPC session
//! over a WebSocket, so it can be controlled remotely (e.g. from a phone
//! browser) over a private Tailscale network.
//!
//! See `README.md` for the architecture overview and setup instructions.
//!
//! This binary is currently a manual smoke test for the `pi` process
//! management layer: it spawns `pi --mode rpc`, sends one prompt taken
//! from the command line, prints the streamed reply, and exits. The
//! WebSocket server that replaces this entry point lands in a later
//! commit.

mod config;
mod rpc;

use anyhow::Result;
use clap::Parser;
use serde_json::{json, Value};
use tokio::sync::broadcast::error::RecvError;
use tracing_subscriber::EnvFilter;

use config::Config;
use rpc::process::PiProcess;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::parse();
    let mut process = PiProcess::spawn(&config)?;

    // Subscribe immediately after spawning to minimize the window in
    // which early events could be missed before anyone is listening.
    let mut events = process.subscribe();

    process
        .send(&json!({"type": "prompt", "message": config.message}))
        .await?;

    loop {
        tokio::select! {
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if print_text_delta(&event) {
                            continue;
                        }
                        if is_agent_settled(&event) {
                            println!();
                            break;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "event receiver lagged, some events were dropped");
                    }
                    Err(RecvError::Closed) => {
                        tracing::warn!("pi process ended before settling");
                        break;
                    }
                }
            }
            status = process.wait() => {
                tracing::warn!(?status, "pi process exited unexpectedly");
                break;
            }
        }
    }

    process.shutdown().await?;
    Ok(())
}

/// Prints a streamed assistant text chunk, if `event` is one, and reports
/// whether it handled the event.
fn print_text_delta(event: &Value) -> bool {
    let delta = event
        .get("assistantMessageEvent")
        .filter(|_| event.get("type").and_then(Value::as_str) == Some("message_update"))
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("text_delta"))
        .and_then(|e| e.get("delta"))
        .and_then(Value::as_str);

    match delta {
        Some(text) => {
            print!("{text}");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            true
        }
        None => false,
    }
}

/// Returns true once the agent has fully settled (no more automatic
/// retries, compaction, or queued continuations), which is the signal
/// that this one-shot prompt is done.
fn is_agent_settled(event: &Value) -> bool {
    event.get("type").and_then(Value::as_str) == Some("agent_settled")
}
