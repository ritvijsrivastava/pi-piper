//! Shared-secret authorization for WebSocket connections.
//!
//! This is defense-in-depth, not the primary security boundary: Piper is
//! designed to be reachable only over a private Tailscale network (see
//! `README.md`). A plain equality check is sufficient here because the
//! token never crosses a network Piper doesn't already trust.

/// Returns true if `presented` matches `expected_token`.
pub fn is_authorized(expected_token: &str, presented: Option<&str>) -> bool {
    presented.is_some_and(|token| token == expected_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_matching_token() {
        assert!(is_authorized("secret", Some("secret")));
    }

    #[test]
    fn rejects_wrong_or_missing_token() {
        assert!(!is_authorized("secret", Some("wrong")));
        assert!(!is_authorized("secret", None));
    }
}
