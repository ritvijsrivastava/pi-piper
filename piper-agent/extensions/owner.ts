// Resolves the tailnet identity of whoever started this `pi` session,
// sent to the Hub as the register frame's `owner` (SPEC.md §6.1). The
// Hub stamps it on the session and only shows/attaches it to requests
// whose `Tailscale-User-Login` matches — this is what makes one user's
// sessions invisible to every other tailnet user.
//
// Resolution order:
//  1. `PIPER_OWNER` env var (explicit override, e.g. for split
//     DNS setups where whois can't run or for testing).
//  2. Tailscale SSH: if the session runs inside an SSH session, the
//     client IP in `$SSH_CLIENT` is the peer's tailnet address, and
//     `tailscale whois` maps it back to the login (`alice@github`).
//  3. Nothing — the session registers unowned and stays visible to all
//     tailnet users (the Hub's pre-ownership behavior).

import { execFileSync } from "node:child_process";

/** Tailscale addresses live in the CGNAT range (100.64.0.0/10); only
 * those can be resolved by `tailscale whois`. A cheap prefix check is
 * enough to skip plain-LAN SSH sessions. */
const TAILSCALE_IP_PREFIX = "100.";

export function resolveOwner(): string | undefined {
  const fromEnv = process.env.PIPER_OWNER;
  if (fromEnv && fromEnv.trim().length > 0) {
    return fromEnv.trim();
  }

  // $SSH_CLIENT = "<client-ip> <client-port> <server-port>"
  const sshClient = process.env.SSH_CLIENT;
  if (sshClient) {
    const clientIp = sshClient.trim().split(/\s+/)[0];
    if (clientIp.startsWith(TAILSCALE_IP_PREFIX)) {
      try {
        const out = execFileSync(
          "tailscale",
          ["whois", "--format=json", clientIp],
          { timeout: 5000 },
        ).toString();
        const parsed = JSON.parse(out) as {
          Profile?: { LoginName?: string };
          LoginName?: string;
        };
        const login = parsed.Profile?.LoginName ?? parsed.LoginName;
        if (typeof login === "string" && login.length > 0) return login;
      } catch {
        // whois unavailable (no tailscaled, not the tailnet's IP, …) —
        // fall through to unowned rather than failing registration.
      }
    }
  }

  return undefined;
}
