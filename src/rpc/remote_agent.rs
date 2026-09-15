//! WebSocket-backed `AgentLink`: represents one `piper-agent` extension
//! connected to the Hub over `/agent` (registration protocol in
//! `ARCHITECTURE.md`).
//!
//! Structurally this mirrors `rpc::process::PiProcess`: an
//! `events_tx` broadcast channel fans inbound JSON out to any number of
//! subscribers (the phone's `/ws` bridge, the registry's status
//! watcher), and `send()` writes one framed command out to the
//! extension. The only difference is the transport (a WebSocket split
//! into a reader task owned by `server::agent` and a writer task owned
//! by this struct) instead of a child process's stdio pipes.

use anyhow::{Context, Result};
use axum::extract::ws::Message;
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

/// Capacity of the broadcast channel fanning events out to subscribers.
/// See `rpc::process::EVENT_CHANNEL_CAPACITY` for the same rationale.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

pub struct RemoteAgent {
    /// Outbound envelope messages to the extension's WebSocket. The
    /// actual `SplitSink::send` calls happen in a writer task owned by
    /// `server::agent::handle`; this struct only needs to hand it
    /// frames.
    to_agent: mpsc::UnboundedSender<Message>,
    /// Fan-out of unwrapped `event`/`response` payloads read from the
    /// extension, verbatim pi RPC JSON, exactly like `PiProcess`'s
    /// stdout broadcast.
    events_tx: broadcast::Sender<Value>,
}

impl RemoteAgent {
    /// `to_agent` is the sending half of the channel the `/agent`
    /// handler's writer task drains; see `server::agent::handle`.
    pub fn new(to_agent: mpsc::UnboundedSender<Message>) -> Self {
        let (events_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            to_agent,
            events_tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events_tx.subscribe()
    }

    /// Called by the `/agent` reader loop for each inbound
    /// `{"type":"event"|"response", ...}` frame's unwrapped payload.
    /// Broadcasting responses and events on the same channel matches
    /// `PiProcess`, which also mixes both on one stdout stream.
    pub fn publish(&self, payload: Value) {
        let _ = self.events_tx.send(payload);
    }

    /// Wraps `command` in the `/agent` command envelope (wire format
    /// in `ARCHITECTURE.md`) and hands it to the writer task. The envelope id
    /// is only for the extension's own bookkeeping; the Hub does not
    /// correlate responses back to a specific `send()` call, since
    /// responses are re-broadcast to every subscriber the same way
    /// `PiProcess` broadcasts everything from a child's stdout.
    pub async fn send(&self, command: &Value) -> Result<()> {
        let envelope = json!({
            "type": "command",
            "id": Uuid::new_v4().to_string(),
            "command": command,
        });
        let text = serde_json::to_string(&envelope).context("serializing agent command")?;
        self.to_agent
            .send(Message::Text(text.into()))
            .map_err(|_| anyhow::anyhow!("agent connection is closed"))
    }

    /// Best-effort: asks the extension's WebSocket to close. The
    /// registry considers the session gone as soon as the `/agent`
    /// reader loop observes the socket close, not when this returns.
    pub async fn close(&self) {
        let _ = self.to_agent.send(Message::Close(None));
    }
}
