//! Command-line and environment-variable configuration.
//!
//! Every option can also be set via an environment variable (see the
//! `env = "..."` attributes below), which is what the systemd deployment
//! uses so secrets like the auth token never appear in `ps` output.

use std::path::PathBuf;

use clap::Parser;

/// Piper: remote control bridge for a `pi` coding-agent session.
#[derive(Debug, Parser)]
#[command(name = "piper", version, about)]
pub struct Config {
    /// Path or name of the `pi` executable to spawn.
    #[arg(long, env = "PIPER_PI_COMMAND", default_value = "pi")]
    pub pi_command: String,

    /// Working directory for the spawned `pi` process, i.e. the project
    /// you want to control remotely.
    #[arg(long, env = "PIPER_PROJECT_DIR")]
    pub project_dir: PathBuf,

    /// Resume a specific pi session file instead of starting a new one.
    /// Mutually exclusive with `--no-session`.
    #[arg(long, env = "PIPER_SESSION", conflicts_with = "no_session")]
    pub session: Option<PathBuf>,

    /// Run pi in ephemeral mode (`--no-session`); nothing is persisted to
    /// disk. Useful for testing.
    #[arg(long, env = "PIPER_NO_SESSION")]
    pub no_session: bool,

    /// Additional raw arguments forwarded verbatim to `pi --mode rpc`
    /// (e.g. `--pi-arg --model --pi-arg anthropic/claude-opus-4-5`).
    /// This avoids re-declaring every pi CLI flag as a Piper flag.
    #[arg(long = "pi-arg")]
    pub extra_pi_args: Vec<String>,

    /// One-shot prompt to send for manual testing from the command line.
    /// The real WebSocket bridge (added in a later commit) does not use
    /// this; it exists so process management can be verified end-to-end
    /// without a browser client.
    pub message: String,
}

impl Config {
    /// Builds the `pi` child process argument list from this configuration.
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
