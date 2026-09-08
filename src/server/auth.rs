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

use axum::http::HeaderMap;

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

/// Header `tailscaled` stamps (see `addTailscaleIdentityHeaders`) onto
/// requests it proxies in via `tailscale serve`, naming the tailnet
/// identity that made the request (e.g. `alice@github`).
const TAILSCALE_LOGIN_HEADER: &str = "tailscale-user-login";

/// Header `tailscaled` sets (to any value) on requests that arrived via
/// `tailscale funnel` (public internet) rather than tailnet-only `serve`.
/// Piper's threat model assumes funnel is never used in front of it (see
/// `README.md`); this is a belt-and-suspenders reject in case it is
/// anyway, so a funnel-exposed Piper can't be walked in via a forged or
/// coincidentally-valid login.
const TAILSCALE_FUNNEL_HEADER: &str = "tailscale-funnel-request";

/// Returns true if `headers` carries a `Tailscale-User-Login` identity
/// header (added by `tailscale serve`, see `SPEC.md` §7) naming a login
/// present in `allowed_logins`, and the request isn't flagged as a
/// Funnel request.
///
/// An empty `allowed_logins` always returns false: this auth path is
/// opt-in and off by default, leaving `--token` as the only way in.
///
/// **Caveat** (see `SPEC.md` §7): this trusts the header at face value.
/// It is only meaningful because Piper binds to loopback only and is
/// meant to be reached exclusively through `tailscale serve`'s proxy —
/// but Piper cannot cryptographically distinguish a connection actually
/// proxied in by `tailscaled` from any other local process on the same
/// machine that opens a loopback connection and sets this header itself.
/// Only enable `--allowed-tailscale-login` on machines where every local
/// user is already trusted with full control of your `pi` sessions.
pub fn is_authorized_tailscale_identity(allowed_logins: &[String], headers: &HeaderMap) -> bool {
    if allowed_logins.is_empty() {
        return false;
    }
    if headers.contains_key(TAILSCALE_FUNNEL_HEADER) {
        return false;
    }
    let Some(login) = headers
        .get(TAILSCALE_LOGIN_HEADER)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    allowed_logins.iter().any(|allowed| allowed == login)
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

    fn headers_with(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        headers
    }

    #[test]
    fn tailscale_identity_disabled_when_allowlist_empty() {
        let headers = headers_with(&[("Tailscale-User-Login", "alice@github")]);
        assert!(!is_authorized_tailscale_identity(&[], &headers));
    }

    #[test]
    fn tailscale_identity_accepts_allowed_login() {
        let allowed = vec!["alice@github".to_string()];
        let headers = headers_with(&[("Tailscale-User-Login", "alice@github")]);
        assert!(is_authorized_tailscale_identity(&allowed, &headers));
    }

    #[test]
    fn tailscale_identity_rejects_unlisted_login() {
        let allowed = vec!["alice@github".to_string()];
        let headers = headers_with(&[("Tailscale-User-Login", "mallory@github")]);
        assert!(!is_authorized_tailscale_identity(&allowed, &headers));
    }

    #[test]
    fn tailscale_identity_rejects_missing_header() {
        let allowed = vec!["alice@github".to_string()];
        assert!(!is_authorized_tailscale_identity(
            &allowed,
            &HeaderMap::new()
        ));
    }

    #[test]
    fn tailscale_identity_rejects_funnel_requests() {
        let allowed = vec!["alice@github".to_string()];
        let headers = headers_with(&[
            ("Tailscale-User-Login", "alice@github"),
            ("Tailscale-Funnel-Request", "true"),
        ]);
        assert!(!is_authorized_tailscale_identity(&allowed, &headers));
    }
}
