// Agent-token and Hub-URL resolution.
//
// The agent token is never typed or copied by the user: piper (the Rust
// Hub) generates one on first run and persists it to a local file this
// extension reads directly, since both processes run on the same
// desktop machine as the same user.

import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

/** Mirrors `piper::agent_token::default_path()` (Rust) exactly, so a
 * freshly-generated Hub token is found with zero configuration. */
export function defaultAgentTokenPath(): string {
  return join(homedir(), ".pi", "agent", "piper", "agent-token");
}

export function resolveAgentToken(): string {
  const fromEnv = process.env.PIPER_AGENT_TOKEN;
  if (fromEnv && fromEnv.trim().length > 0) {
    return fromEnv.trim();
  }

  const path = process.env.PIPER_AGENT_TOKEN_PATH || defaultAgentTokenPath();
  try {
    const token = readFileSync(path, "utf8").trim();
    if (token.length > 0) return token;
  } catch {
    // fall through to the error below
  }
  throw new Error(
    `piper-agent: could not read an agent token from ${path}. Start the piper Hub at least once first (it generates this file automatically), or set PIPER_AGENT_TOKEN.`,
  );
}

export function resolveHubUrl(): string {
  return process.env.PIPER_HUB_URL || "ws://127.0.0.1:4390/agent";
}
