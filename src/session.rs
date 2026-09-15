//! Session identity, metadata, and the `AgentLink` abstraction.
//!
//! A "session" is one running `pi` process,
//! reached either by spawning it ourselves (`AgentLink::Headless`, the
//! Pi Piper v1 behavior) or by a `pi-piper-agent` extension inside an
//! already-running interactive `pi` dialing in over `/agent`
//! (`AgentLink::Remote`, new in v2). Both are driven identically by
//! everything above this module: subscribe for events, send for
//! commands.

use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{broadcast, watch};

use crate::rpc::process::PiProcess;
use crate::rpc::remote_agent::RemoteAgent;

/// Milliseconds since the Unix epoch, used for all timestamps in this
/// module so the PWA can format them client-side without pulling in a
/// datetime dependency on the Rust side.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// How a session's `AgentLink` was established.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    /// `pi-piper-agent` extension inside an interactive `pi` dialed in.
    Remote,
    /// Pi Piper itself spawned `pi --mode rpc` (v1 behavior, kept as a
    /// fallback for projects with no terminal open).
    Headless,
}

/// Display/identity metadata for one session. Mutable over the
/// session's lifetime (see `session_info_changed` -> `meta_update`).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub session_id: String,
    pub session_file: Option<String>,
    pub session_name: Option<String>,
    pub cwd: String,
    pub connected_at_ms: i64,
    pub kind: SessionKind,
}

/// Passively-derived, frequently-updated status. See
/// `registry::watch_status`.
#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub is_streaming: bool,
    pub last_activity_at_ms: i64,
    pub last_preview: Option<String>,
}

/// Everything the phone's session list needs for one entry.
#[derive(Clone, Serialize)]
pub struct SessionSummary {
    #[serde(flatten)]
    pub meta: SessionMeta,
    #[serde(flatten)]
    pub status: SessionStatus,
    pub connected: bool,
}

/// The transport-level connection to one running `pi` process. Kept as
/// a closed two-variant enum (rather than a `dyn Trait`) since there are
/// only ever these two implementations and it avoids an extra
/// dyn-async-trait dependency for two methods.
pub enum AgentLink {
    Headless(Arc<PiProcess>),
    Remote(Arc<RemoteAgent>),
}

impl AgentLink {
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        match self {
            AgentLink::Headless(p) => p.subscribe(),
            AgentLink::Remote(r) => r.subscribe(),
        }
    }

    pub async fn send(&self, command: &Value) -> Result<()> {
        match self {
            AgentLink::Headless(p) => p.send(command).await,
            AgentLink::Remote(r) => r.send(command).await,
        }
    }

    pub async fn shutdown(&self) {
        match self {
            AgentLink::Headless(p) => {
                let _ = p.shutdown().await;
            }
            AgentLink::Remote(r) => r.close().await,
        }
    }
}

/// One registered session: its link, its mutable meta/status, and a
/// `connected` watch so long-lived tasks (the status watcher in
/// `registry.rs`, a phone's `/ws` bridge) can notice disconnection even
/// after the registry itself has dropped its own reference.
pub struct SessionHandle {
    pub link: AgentLink,
    pub meta: RwLock<SessionMeta>,
    pub status: RwLock<SessionStatus>,
    connected: watch::Sender<bool>,
}

impl SessionHandle {
    pub fn new(link: AgentLink, meta: SessionMeta) -> Self {
        let (connected, _) = watch::channel(true);
        Self {
            link,
            meta: RwLock::new(meta),
            status: RwLock::new(SessionStatus::default()),
            connected,
        }
    }

    pub fn connected_rx(&self) -> watch::Receiver<bool> {
        self.connected.subscribe()
    }

    pub fn is_connected(&self) -> bool {
        *self.connected.borrow()
    }

    /// Marks this handle disconnected. Idempotent.
    pub fn mark_disconnected(&self) {
        let _ = self.connected.send(false);
    }

    pub fn summary(&self) -> SessionSummary {
        SessionSummary {
            meta: self.meta.read().unwrap().clone(),
            status: self.status.read().unwrap().clone(),
            connected: self.is_connected(),
        }
    }
}
