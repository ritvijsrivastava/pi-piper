//! `SessionRegistry`: the Hub's switchboard. See `SPEC.md` §5.1 and
//! §6.3.
//!
//! Keyed by session id (not project path — two terminals in the same
//! repo are two different sessions, see `SPEC.md` §12). Holds every
//! currently-registered `SessionHandle` and a broadcast channel of
//! registry-change notifications consumed by `/ws/control`.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::session::{now_ms, AgentLink, SessionHandle, SessionMeta, SessionSummary};

/// Capacity of the `/ws/control` broadcast channel. Generous since
/// registry-change events are rare (session connects/disconnects,
/// streaming-state flips) compared to the per-token event traffic on an
/// individual session's own event channel.
const CONTROL_CHANNEL_CAPACITY: usize = 256;

pub struct SessionRegistry {
    sessions: RwLock<HashMap<String, Arc<SessionHandle>>>,
    control_tx: broadcast::Sender<Value>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        let (control_tx, _) = broadcast::channel(CONTROL_CHANNEL_CAPACITY);
        Self {
            sessions: RwLock::new(HashMap::new()),
            control_tx,
        }
    }

    /// Registers a new session and spawns its passive status watcher.
    /// If a session with this id is already registered (e.g. a
    /// reconnect race), the old entry is replaced.
    pub fn register(
        self: &Arc<Self>,
        id: String,
        link: AgentLink,
        meta: SessionMeta,
    ) -> Arc<SessionHandle> {
        let handle = Arc::new(SessionHandle::new(link, meta));

        if let Some(old) = self
            .sessions
            .write()
            .unwrap()
            .insert(id.clone(), handle.clone())
        {
            old.mark_disconnected();
        }

        let _ = self.control_tx.send(json!({
            "type": "session_connected",
            "session": handle.summary(),
        }));

        tokio::spawn(watch_status(handle.clone(), self.control_tx.clone()));

        handle
    }

    /// Removes `id` from the registry and marks its handle
    /// disconnected. Any task still holding an `Arc<SessionHandle>`
    /// from before this call (a phone `/ws` bridge, the status
    /// watcher) observes the disconnect via `connected_rx()` and winds
    /// down; it does not keep the session "live" from the registry's
    /// point of view.
    pub fn unregister(&self, id: &str) {
        let removed = self.sessions.write().unwrap().remove(id);
        if let Some(handle) = removed {
            handle.mark_disconnected();
            let _ = self
                .control_tx
                .send(json!({"type": "session_disconnected", "sessionId": id}));
        }
    }

    /// Applies a `meta_update` patch (see `SPEC.md` §6.1) for a
    /// `session_info_changed` event forwarded by `piper-agent`.
    pub fn update_meta(&self, id: &str, patch: &Value) {
        let sessions = self.sessions.read().unwrap();
        let Some(handle) = sessions.get(id) else {
            return;
        };
        {
            let mut meta = handle.meta.write().unwrap();
            if let Some(v) = patch.get("sessionFile").and_then(Value::as_str) {
                meta.session_file = Some(v.to_string());
            }
            if let Some(v) = patch.get("sessionName").and_then(Value::as_str) {
                meta.session_name = Some(v.to_string());
            }
            if let Some(v) = patch.get("cwd").and_then(Value::as_str) {
                meta.cwd = v.to_string();
            }
        }
        let _ = self
            .control_tx
            .send(json!({"type": "session_meta", "session": handle.summary()}));
    }

    /// Forwards an `event` or `response` payload from a `Remote` agent
    /// link into that session's own broadcast channel. No-op for
    /// `Headless` sessions, whose `PiProcess` already broadcasts its
    /// own stdout directly.
    pub fn publish(&self, id: &str, payload: Value) {
        if let Some(handle) = self.sessions.read().unwrap().get(id) {
            if let AgentLink::Remote(remote) = &handle.link {
                remote.publish(payload);
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<Arc<SessionHandle>> {
        self.sessions.read().unwrap().get(id).cloned()
    }

    /// Returns the sole registered session, if exactly one is
    /// registered. Used by `/ws` when the phone omits `?session=`, to
    /// keep the single-session case as simple as Piper v1.
    pub fn get_default(&self) -> Option<Arc<SessionHandle>> {
        let sessions = self.sessions.read().unwrap();
        if sessions.len() == 1 {
            sessions.values().next().cloned()
        } else {
            None
        }
    }

    pub fn list(&self) -> Vec<SessionSummary> {
        self.sessions
            .read()
            .unwrap()
            .values()
            .map(|h| h.summary())
            .collect()
    }

    pub fn control_subscribe(&self) -> broadcast::Receiver<Value> {
        self.control_tx.subscribe()
    }
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Background task, one per registered session: passively derives
/// `is_streaming` / `last_preview` / `last_activity_at_ms` by peeking at
/// events already flowing to other subscribers, and pushes a
/// consolidated `session_update` control message when they change. Ends
/// when the session disconnects (`connected_rx` flips false) or its
/// link's event channel truly closes.
async fn watch_status(handle: Arc<SessionHandle>, control_tx: broadcast::Sender<Value>) {
    let mut events = handle.link.subscribe();
    let mut connected = handle.connected_rx();

    loop {
        tokio::select! {
            changed = connected.changed() => {
                if changed.is_err() || !*connected.borrow() {
                    break;
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => handle_event(&handle, &control_tx, &event),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

fn handle_event(handle: &Arc<SessionHandle>, control_tx: &broadcast::Sender<Value>, event: &Value) {
    let ty = event.get("type").and_then(Value::as_str).unwrap_or("");
    // Only inspect the handful of event types relevant to list-screen
    // status; everything else (e.g. per-token `message_update` deltas)
    // is left to the session's own `/ws` subscribers and would otherwise
    // flood the control channel for no benefit to the session list.
    if !matches!(ty, "agent_start" | "agent_settled" | "message_end") {
        return;
    }

    let mut status = handle.status.write().unwrap();
    let prev_streaming = status.is_streaming;
    let prev_preview = status.last_preview.clone();
    status.last_activity_at_ms = now_ms();

    match ty {
        "agent_start" => status.is_streaming = true,
        "agent_settled" => status.is_streaming = false,
        "message_end" => {
            if let Some(preview) = extract_preview(event) {
                status.last_preview = Some(preview);
            }
        }
        _ => {}
    }

    let changed = status.is_streaming != prev_streaming || status.last_preview != prev_preview;
    drop(status);

    if changed {
        let _ = control_tx.send(json!({"type": "session_update", "session": handle.summary()}));
    }
}

/// Extracts a short preview string from a `message_end` event's
/// `message.content` (user or assistant), truncated to ~120 characters.
fn extract_preview(event: &Value) -> Option<String> {
    let message = event.get("message")?;
    let role = message.get("role")?.as_str()?;
    if role != "user" && role != "assistant" {
        return None;
    }
    let text = extract_text_from_content(message.get("content")?)?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate_chars(trimmed, 120))
}

fn extract_text_from_content(content: &Value) -> Option<String> {
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    let arr = content.as_array()?;
    let mut out = String::new();
    for block in arr {
        if block.get("type").and_then(Value::as_str) == Some("text") {
            if let Some(t) = block.get("text").and_then(Value::as_str) {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(t);
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max).collect();
    format!("{truncated}\u{2026}")
}
