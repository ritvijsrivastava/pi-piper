//! Generation and persistence of the agent token.
//!
//! This is the only shared secret left in Piper: it gates `/agent`
//! (where `piper-agent` extensions register live sessions) and is
//! never sent to the phone. `/ws`, `/ws/control`, and `/api/sessions`
//! have no analogous token — they authorize on the `Tailscale-User-Login`
//! identity header instead. This token lives in a
//! local file readable only by the desktop user, so `piper-agent` can
//! read it without the user ever typing or copying it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use uuid::Uuid;

/// Default location: `~/.pi/agent/piper/agent-token`. Mirrors pi's own
/// `~/.pi/agent/` config directory so the two tools' state lives
/// side by side.
pub fn default_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".pi")
        .join("agent")
        .join("piper")
        .join("agent-token")
}

/// Loads the agent token from `path`, generating and persisting a new
/// random one if the file doesn't exist yet or is empty. The file is
/// created with mode `0600` on Unix.
pub fn load_or_create(path: &Path) -> Result<String> {
    if let Ok(existing) = fs::read_to_string(path) {
        let token = existing.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // Two concatenated v4 UUIDs: simple way to get a long random token
    // without adding a dedicated RNG dependency beyond `uuid` (already a
    // dependency for session/command envelope ids).
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    write_private(path, &token)?;
    tracing::info!(path = %path.display(), "generated a new piper agent token");
    Ok(token)
}

#[cfg(unix)]
fn write_private(path: &Path, token: &str) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    writeln!(file, "{token}").with_context(|| format!("writing {}", path.display()))
}

#[cfg(not(unix))]
fn write_private(path: &Path, token: &str) -> Result<()> {
    fs::write(path, format!("{token}\n")).with_context(|| format!("creating {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_reuses_token() {
        let dir = tempdir();
        let path = dir.join("agent-token");

        let first = load_or_create(&path).expect("generate token");
        assert!(!first.is_empty());

        let second = load_or_create(&path).expect("reload token");
        assert_eq!(first, second, "reload must return the same persisted token");

        std::fs::remove_dir_all(&dir).ok();
    }

    fn tempdir() -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("piper-agent-token-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
