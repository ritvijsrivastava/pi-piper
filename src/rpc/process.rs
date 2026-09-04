//! Spawns and manages the `pi --mode rpc` child process.
//!
//! Design note: this module deliberately does *not* implement crash
//! restart/backoff logic. If the child process exits, [`PiProcess::wait`]
//! returns and the caller treats that as fatal, exiting the whole Piper
//! process with a non-zero status. The systemd unit (see `deploy/`) is
//! configured with `Restart=on-failure`, so the operating system's own,
//! well-tested process supervisor handles restarts instead of a
//! hand-rolled one here (KISS/YAGNI: don't reimplement what systemd
//! already does).

use std::process::Stdio;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, warn};

use crate::config::Config;
use crate::rpc::{write_json_line, JsonLineReader};

/// Capacity of the broadcast channel fanning RPC events out to subscribers
/// (WebSocket clients). A generous bound absorbs bursts of streaming
/// deltas; a slow subscriber that falls behind by more than this many
/// events will observe a `Lagged` error and skip ahead rather than block
/// everyone else.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// A running `pi --mode rpc` child process.
///
/// Commands are written to the child's stdin via [`PiProcess::send`].
/// Events read from the child's stdout are fanned out to any number of
/// subscribers via [`PiProcess::subscribe`].
pub struct PiProcess {
    child: Child,
    stdin: Mutex<ChildStdin>,
    events_tx: broadcast::Sender<Value>,
}

impl PiProcess {
    /// Spawns `pi --mode rpc` according to `config` and starts background
    /// tasks that forward its stdout (as parsed JSON events) and stderr
    /// (as log lines) onward.
    pub fn spawn(config: &Config) -> Result<Self> {
        let mut child = Command::new(&config.pi_command)
            .args(config.pi_args())
            .current_dir(&config.project_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawning `{}`", config.pi_command))?;

        let stdin = child.stdin.take().context("child stdin was not piped")?;
        let stdout = child.stdout.take().context("child stdout was not piped")?;
        let stderr = child.stderr.take().context("child stderr was not piped")?;

        let (events_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

        // Forward stdout: one JSON event per line, fanned out to subscribers.
        let stdout_tx = events_tx.clone();
        tokio::spawn(async move {
            let mut reader = JsonLineReader::new(stdout);
            loop {
                match reader.next().await {
                    Ok(Some(event)) => {
                        // No subscribers yet is normal (e.g. at startup) and
                        // not an error worth logging.
                        let _ = stdout_tx.send(event);
                    }
                    Ok(None) => {
                        debug!("pi process stdout closed");
                        break;
                    }
                    Err(err) => {
                        warn!(%err, "failed to parse line from pi stdout, skipping");
                    }
                }
            }
        });

        // Forward stderr as plain log lines; pi's own diagnostics are not
        // part of the JSONL protocol.
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!(target: "pi_stderr", "{line}");
            }
        });

        Ok(Self {
            child,
            stdin: Mutex::new(stdin),
            events_tx,
        })
    }

    /// Subscribes to the stream of RPC events read from the child's
    /// stdout. Each subscriber gets its own independent receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events_tx.subscribe()
    }

    /// Sends one RPC command to the child's stdin.
    pub async fn send(&self, command: &Value) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        write_json_line(&mut *stdin, command).await
    }

    /// Waits for the child process to exit. Callers should treat any
    /// return from this (success or failure) as fatal for the current
    /// Piper process; see the module-level docs for why restarts are left
    /// to systemd.
    pub async fn wait(&mut self) -> Result<std::process::ExitStatus> {
        self.child.wait().await.context("waiting for pi process")
    }

    /// Best-effort shutdown: closes stdin (so pi sees EOF and can exit
    /// cleanly) and asks the OS to terminate the process if it doesn't.
    pub async fn shutdown(&mut self) -> Result<()> {
        if let Ok(mut stdin) = self.stdin.try_lock() {
            let _ = stdin.shutdown().await;
        }
        let _ = self.child.start_kill();
        Ok(())
    }
}
