// Small piece of shared mutable state: the most recently observed
// extension context.
//
// Command dispatch (command-dispatch.ts) needs a live `ExtensionContext`
// to call things like `ctx.abort()`, and a live `ExtensionCommandContext`
// (a strict superset, only available inside command handlers) for
// session-control operations like `ctx.newSession()`. Neither is passed
// to `pi.*` free functions, so we cache the latest one seen from any
// event handler / command handler invocation. This is safe within one
// session's lifetime; it is intentionally *not* carried across a session
// replacement (`/new`, `/resume`, `/fork`) — see index.ts's `reconnect`
// callback, which re-seeds this from the replacement session's own
// `withSession` callback instead.

import type { ExtensionCommandContext, ExtensionContext } from "@earendil-works/pi-coding-agent";

let latestCtx: ExtensionContext | undefined;
let latestCommandCtx: ExtensionCommandContext | undefined;

export function rememberCtx(ctx: ExtensionContext): void {
  latestCtx = ctx;
}

export function rememberCommandCtx(ctx: ExtensionCommandContext): void {
  latestCtx = ctx;
  latestCommandCtx = ctx;
}

export function getCtx(): ExtensionContext {
  if (!latestCtx) {
    throw new Error("pi-piper-agent: no extension context observed yet in this session");
  }
  return latestCtx;
}

/** Throws with a clear message if the last command-capable context we
 * have is stale (e.g. right after a session replacement before the new
 * session's `/rc` — or automatic reconnect — has run). */
export function getCommandCtx(): ExtensionCommandContext {
  if (!latestCommandCtx) {
    throw new Error(
      "pi-piper-agent: this command needs a command-context snapshot that isn't available yet; run /rc again in this terminal",
    );
  }
  return latestCommandCtx;
}
