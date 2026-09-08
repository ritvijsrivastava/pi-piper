//! Shared-secret authorization for WebSocket and HTTP connections.
//!
//! This is defense-in-depth, not the primary security boundary: Piper is
//! designed to be reachable only over a private Tailscale network (see
//! `README.md`). A plain equality check is sufficient here because the
//! token never crosses a network Piper doesn't already trust.
//!
//! Two independent tokens exist (phone vs. agent, see `SPEC.md` §7);
//! this module only implements the comparison, callers pick which
//! expected token applies to which route.

/// Returns true if `presented` matches `expected_token`.
pub fn is_authorized(expected_token: &str, presented: Option<&str>) -> bool {
    presented.is_some_and(|token| token == expected_token)
}

/// Extracts a bearer token from an `Authorization: Bearer <token>` header
/// value, used by `/api/sessions` as an alternative to `?token=` for
/// plain HTTP requests.
pub fn extract_bearer(header_value: Option<&str>) -> Option<&str> {
    header_value?.strip_prefix("Bearer ")
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

    #[test]
    fn extracts_bearer_token() {
        assert_eq!(extract_bearer(Some("Bearer abc123")), Some("abc123"));
        assert_eq!(extract_bearer(Some("abc123")), None);
        assert_eq!(extract_bearer(None), None);
    }
}
