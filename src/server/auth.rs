//! Authorization for WebSocket and HTTP connections.
//!
//! This is defense-in-depth, not the primary security boundary: Piper is
//! designed to be reachable only over a private Tailscale network (see
//! `README.md`). There are two independent checks, for two different
//! reasons (see `SPEC.md` §7):
//!
//! - The phone-facing routes (`/ws`, `/ws/control`, `/api/sessions`)
//!   trust the `Tailscale-User-Login` identity header that `tailscale
//!   serve` stamps onto every request it proxies in: if that header is
//!   present (and the request isn't a Funnel request), the caller is on
//!   the tailnet, full stop — no shared secret, no per-login allowlist.
//!   Any device signed into the tailnet is authorized.
//! - `/agent` (where `piper-agent` extensions register live sessions)
//!   still gates on a shared secret (`PIPER_AGENT_TOKEN`), because it's
//!   reached over loopback directly by a local process, never proxied
//!   through `tailscale serve`, so there is no identity header to trust
//!   there at all.

use axum::http::HeaderMap;

/// Returns true if `presented` matches `expected_token`. Used only for
/// `/agent`'s agent-token check; a plain equality check is sufficient
/// because that token never crosses a network Piper doesn't already
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
/// Piper's threat model assumes funnel is never used in front of it (see
/// `README.md`); this is a belt-and-suspenders reject in case it is
/// anyway, so a funnel-exposed Piper can't be walked in via a forged or
/// coincidentally-valid login.
const TAILSCALE_FUNNEL_HEADER: &str = "tailscale-funnel-request";

/// Extracts the tailnet login identity (`tailscaled` stamps it onto
/// requests it proxies in via `tailscale serve`, see `SPEC.md` §7), or
/// `None` when the request has no trustworthy identity: missing/empty
/// login header, or a Funnel flag (public internet, not tailnet).
///
/// This is both the gate (handlers reject with 401 when it returns
/// `None`) and the *viewer identity* used to enforce per-user session
/// ownership (see `SessionMeta::owner`), so every phone-facing handler
/// must call this once and use the result for both purposes.
///
/// **Caveat** (see `SPEC.md` §7): this trusts the header at face value.
/// It is only meaningful because Piper binds to loopback only and is
/// meant to be reached exclusively through `tailscale serve`'s proxy —
/// but Piper cannot cryptographically distinguish a connection actually
/// proxied in by `tailscaled` from any other local process on the same
/// machine that opens a loopback connection and sets this header itself.
/// Every local user on the Hub's machine must already be trusted with
/// full control of your `pi` sessions.
pub fn tailscale_login(headers: &HeaderMap) -> Option<String> {
    if headers.contains_key(TAILSCALE_FUNNEL_HEADER) {
        return None;
    }
    headers
        .get(TAILSCALE_LOGIN_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|login| !login.is_empty())
        .map(str::to_string)
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
        assert_eq!(tailscale_login(&HeaderMap::new()), None);
    }

    #[test]
    fn tailscale_accepts_any_login() {
        let headers = headers_with(&[("Tailscale-User-Login", "alice@github")]);
        assert_eq!(tailscale_login(&headers).as_deref(), Some("alice@github"));

        // Accepts any tailnet login, not just an allowlisted one: the
        // access-control boundary is "reachable on the tailnet at all"
        // (enforced by `tailscale serve` + Tailscale ACLs); per-user
        // isolation beyond that is ownership filtering, not
        // authentication.
        let headers = headers_with(&[("Tailscale-User-Login", "mallory@github")]);
        assert_eq!(tailscale_login(&headers).as_deref(), Some("mallory@github"));
    }

    #[test]
    fn tailscale_rejects_empty_login() {
        let headers = headers_with(&[("Tailscale-User-Login", "")]);
        assert_eq!(tailscale_login(&headers), None);
    }

    #[test]
    fn tailscale_rejects_funnel_requests() {
        let headers = headers_with(&[
            ("Tailscale-User-Login", "alice@github"),
            ("Tailscale-Funnel-Request", "true"),
        ]);
        assert_eq!(tailscale_login(&headers), None);
    }
}
