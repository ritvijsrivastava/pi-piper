//! Authorization for WebSocket and HTTP connections.
//!
//! This is defense-in-depth, not the primary security boundary: Pi Piper is
//! designed to be reachable only over a private Tailscale network (see
//! `README.md`). There are two independent checks, for two different
//! reasons:
//!
//! - The phone-facing routes (`/ws`, `/ws/control`, `/api/sessions`)
//!   trust the `Tailscale-User-Login` identity header that `tailscale
//!   serve` stamps onto every request it proxies in: if that header is
//!   present (and the request isn't a Funnel request), the caller is on
//!   the tailnet, full stop — no shared secret, no per-login allowlist.
//!   Any device signed into the tailnet is authorized.
//! - `/agent` (where `pi-piper-agent` extensions register live sessions)
//!   still gates on a shared secret (`PI_PIPER_AGENT_TOKEN`), because it's
//!   reached over loopback directly by a local process, never proxied
//!   through `tailscale serve`, so there is no identity header to trust
//!   there at all.

use axum::http::HeaderMap;

/// Returns true if `presented` matches `expected_token`. Used only for
/// `/agent`'s agent-token check; a plain equality check is sufficient
/// because that token never crosses a network Pi Piper doesn't already
/// trust.
pub fn is_authorized(expected_token: &str, presented: Option<&str>) -> bool {
    presented.is_some_and(|token| token == expected_token)
}

/// Header `tailscaled` stamps (see `addTailscaleIdentityHeaders`) onto
/// requests it proxies in via `tailscale serve`, naming the tailnet
/// identity that made the request (e.g. `alice@github`).
const TAILSCALE_LOGIN_HEADER: &str = "tailscale-user-login";

/// Header `tailscaled` sets (to any value) on requests that arrived via
/// `tailscale funnel` (public internet) rather than tailnet-only
/// `serve`.
/// Pi Piper's threat model assumes funnel is never used in front of it (see
/// `README.md`); this is a belt-and-suspenders reject in case it is
/// anyway, so a funnel-exposed Pi Piper can't be walked in via a forged or
/// coincidentally-valid login.
const TAILSCALE_FUNNEL_HEADER: &str = "tailscale-funnel-request";

/// Returns true if `headers` carries a non-empty `Tailscale-User-Login`
/// identity header (added by `tailscale serve`) and
/// the request isn't flagged as a Funnel request.
///
/// Deliberately accepts *any* tailnet login, not just an allowlisted
/// one: Pi Piper's access-control boundary is "reachable on the tailnet at
/// all" (enforced by `tailscale serve` + Tailscale ACLs), not which
/// specific login made the request.
///
/// **Caveat**: this trusts the header at face value.
/// It is only meaningful because Pi Piper binds to loopback only and is
/// meant to be reached exclusively through `tailscale serve`'s proxy —
/// but Pi Piper cannot cryptographically distinguish a connection actually
/// proxied in by `tailscaled` from any other local process on the same
/// machine that opens a loopback connection and sets this header itself.
/// Every local user on the Hub's machine must already be trusted with
/// full control of your `pi` sessions.
pub fn is_authorized_tailscale(headers: &HeaderMap) -> bool {
    if headers.contains_key(TAILSCALE_FUNNEL_HEADER) {
        return false;
    }
    headers
        .get(TAILSCALE_LOGIN_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|login| !login.is_empty())
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
    fn tailscale_rejects_missing_header() {
        assert!(!is_authorized_tailscale(&HeaderMap::new()));
    }

    #[test]
    fn tailscale_accepts_any_login() {
        let headers = headers_with(&[("Tailscale-User-Login", "alice@github")]);
        assert!(is_authorized_tailscale(&headers));

        let headers = headers_with(&[("Tailscale-User-Login", "mallory@github")]);
        assert!(is_authorized_tailscale(&headers));
    }

    #[test]
    fn tailscale_rejects_empty_login() {
        let headers = headers_with(&[("Tailscale-User-Login", "")]);
        assert!(!is_authorized_tailscale(&headers));
    }

    #[test]
    fn tailscale_rejects_funnel_requests() {
        let headers = headers_with(&[
            ("Tailscale-User-Login", "alice@github"),
            ("Tailscale-Funnel-Request", "true"),
        ]);
        assert!(!is_authorized_tailscale(&headers));
    }
}
