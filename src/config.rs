//! Command-line and environment-variable configuration.
//!
//! Every option can also be set via an environment variable (see the
//! `env = "..."` attributes below), which is what the systemd deployment
//! uses so secrets like the tokens never appear in `ps` output.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;

/// Piper: multi-session remote control hub for `pi` coding-agent
/// sessions. See `SPEC.md` for the full architecture.
#[derive(Debug, Parser)]
#[command(name = "piper", version, about)]
pub struct Config {
    /// Path or name of the `pi` executable to spawn for headless
    /// (fallback) sessions. Only used when `--project-dir` is set.
    #[arg(long, env = "PIPER_PI_COMMAND", default_value = "pi")]
    pub pi_command: String,

    /// Working directory for an optional spawned headless `pi` process
    /// (Piper v1 behavior, see `SPEC.md` §10). Omit this to run Piper as
    /// a pure hub with no spawned child, relying entirely on
    /// `piper-agent` (`/rc`) connections for sessions.
    #[arg(long, env = "PIPER_PROJECT_DIR")]
    pub project_dir: Option<PathBuf>,

    /// Resume a specific pi session file instead of starting a new one.
    /// Only used with `--project-dir`. Mutually exclusive with
    /// `--no-session`.
    #[arg(long, env = "PIPER_SESSION", conflicts_with = "no_session")]
    pub session: Option<PathBuf>,

    /// Run the headless pi in ephemeral mode (`--no-session`); nothing
    /// is persisted to disk. Only used with `--project-dir`.
    #[arg(long, env = "PIPER_NO_SESSION")]
    pub no_session: bool,

    /// Additional raw arguments forwarded verbatim to the headless
    /// `pi --mode rpc` (e.g. `--pi-arg --model --pi-arg
    /// anthropic/claude-opus-4-5`). Only used with `--project-dir`.
    /// This avoids re-declaring every pi CLI flag as a Piper flag.
    #[arg(long = "pi-arg")]
    pub extra_pi_args: Vec<String>,

    /// Address to bind the HTTP/WebSocket server to. Defaults to loopback
    /// only: expose it to your phone via `tailscale serve`, which proxies
    /// from the tailnet (with a valid HTTPS certificate) to this local
    /// address, rather than binding Piper itself to a non-loopback address.
    #[arg(long, env = "PIPER_BIND", default_value = "127.0.0.1:4390")]
    pub bind: SocketAddr,

    /// Shared secret `piper-agent` extensions must present as `?token=`
    /// to open `/agent`. `/ws`, `/ws/control`, and `/api/sessions` have
    /// no analogous secret — they authorize any request carrying a
    /// `Tailscale-User-Login` identity header instead, since
    /// `tailscale serve` proxies those connections through loopback and
    /// a peer-address check alone can't tell them apart from a local
    /// `/agent` connection. If unset, Piper generates one on first run
    /// and persists it to `--agent-token-path`; `piper-agent` reads it
    /// directly from that file, so it never needs to be typed or copied
    /// to the phone. See `SPEC.md` §7.
    #[arg(long, env = "PIPER_AGENT_TOKEN")]
    pub agent_token: Option<String>,

    /// Where to persist a generated agent token. Defaults to
    /// `~/.pi/agent/piper/agent-token` (mode 0600 on Unix).
    #[arg(long, env = "PIPER_AGENT_TOKEN_PATH")]
    pub agent_token_path: Option<PathBuf>,
}

impl Config {
    /// Builds the headless `pi` child process argument list from this
    /// configuration. Only meaningful when `project_dir` is set.
    pub fn pi_args(&self) -> Vec<String> {
        let mut args = vec!["--mode".to_string(), "rpc".to_string()];

        if self.no_session {
            args.push("--no-session".to_string());
        } else if let Some(session) = &self.session {
            args.push("--session".to_string());
            args.push(session.display().to_string());
        }

        args.extend(self.extra_pi_args.iter().cloned());
        args
    }
}
