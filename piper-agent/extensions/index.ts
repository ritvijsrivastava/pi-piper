// piper-agent: adds `/rc` to connect the current interactive `pi`
// session to a piper Hub for remote control from your phone. See
// SPEC.md (in the piper repo) for the architecture and
// piper-agent/README.md for setup and known limitations.

import type { ExtensionAPI, ExtensionCommandContext } from "@earendil-works/pi-coding-agent";
import { createCommandHandler } from "./command-dispatch.ts";
import { rememberCommandCtx } from "./context-cache.ts";
import { registerEventForwarding } from "./event-bridge.ts";
import { HubClient } from "./hub-client.ts";

export default function (pi: ExtensionAPI) {
  let client: HubClient | undefined;
  let clientGeneration = 0;
  let replacingSession = false;

  const handleCommand = createCommandHandler(
    pi,
    (newCtx: ExtensionCommandContext) => {
      // Re-registers under the replacement session after a remotely
      // triggered /new, /fork, or switch_session (/resume) — see
      // piper-agent/README.md "Following across /new, /fork, /resume".
      start(newCtx);
    },
    () => {
      // A remote `/new` must follow the same lifecycle as typing `/rc stop`,
      // `/new`, then `/rc`: close the old registration before pi replaces
      // the session, and keep the replacement's shutdown hook from closing
      // the freshly reconnected client.
      replacingSession = true;
      stop();
    },
  );

  function start(ctx: ExtensionCommandContext): void {
    rememberCommandCtx(ctx);
    const generation = ++clientGeneration;
    client?.close();
    // Tracks the last status a notification was shown for, so a Hub
    // outage doesn't produce a fresh toast on every 1–10s reconnect
    // attempt: "error"/"disconnected" only notify once, right after a
    // transition worth telling the user about, not on every retry that
    // repeats the same outcome.
    let lastNotified: string | undefined;
    client = new HubClient(
      {
        sessionId: ctx.sessionManager.getSessionId(),
        sessionFile: ctx.sessionManager.getSessionFile(),
        sessionName: ctx.sessionManager.getSessionName(),
        cwd: ctx.cwd,
      },
      handleCommand,
      (status, detail) => {
        // A closed client can emit one final status asynchronously. Its
        // context may already be stale after /new, so never touch that
        // context unless this callback still belongs to the active client.
        if (generation !== clientGeneration) return;
        if (status === "connected") {
          ctx.ui.notify("piper: connected.", "info");
        } else if (status === "error" && lastNotified !== "error") {
          ctx.ui.notify(`piper: ${detail}`, "warning");
        } else if (status === "disconnected" && lastNotified === "connected") {
          ctx.ui.notify("piper: disconnected from the Hub \u2013 retrying\u2026", "warning");
        }
        lastNotified = status;
      },
    );
    client.connect();
  }

  function stop(): void {
    clientGeneration += 1;
    client?.close();
    client = undefined;
  }

  registerEventForwarding(pi, (event) => client?.sendEvent(event));

  // Lightweight rename (e.g. `/name`); full session replacement (`/new`,
  // `/resume`, `/fork`) is handled via `session_shutdown` below plus the
  // `reconnect` callback passed to createCommandHandler above, since
  // those tear down and recreate the whole extension runtime.
  pi.on("session_info_changed", () => {
    client?.updateInfo({ sessionName: pi.getSessionName() });
  });

  pi.on("session_shutdown", () => {
    if (replacingSession) {
      replacingSession = false;
      return;
    }
    stop();
  });

  pi.registerCommand("rc", {
    description: "Connect this session to a piper Hub for remote control from your phone",
    handler: async (args, ctx) => {
      const sub = args.trim();
      if (sub === "stop") {
        stop();
        ctx.ui.notify("piper: disconnected", "info");
        return;
      }
      if (sub === "status") {
        if (!client) {
          ctx.ui.notify("piper: not connected", "info");
          return;
        }
        const { status, detail } = client.getStatus();
        const message = detail ? `piper: ${status} (${detail})` : `piper: ${status}`;
        ctx.ui.notify(message, status === "error" ? "warning" : "info");
        return;
      }
      try {
        start(ctx);
        ctx.ui.notify("piper: connecting to the Hub…", "info");
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        ctx.ui.notify(`piper: failed to start: ${message}`, "error");
        console.error("piper-agent /rc failed:", err);
      }
    },
    getArgumentCompletions: (prefix) => {
      const options = ["status", "stop"].filter((o) => o.startsWith(prefix));
      return options.length > 0 ? options.map((value) => ({ value, label: value })) : null;
    },
  });
}
