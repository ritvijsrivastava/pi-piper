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

  const handleCommand = createCommandHandler(pi, (newCtx: ExtensionCommandContext) => {
    // Re-registers under the replacement session after a remotely
    // triggered /new, /fork, or switch_session (/resume) — see
    // piper-agent/README.md "Following across /new, /fork, /resume".
    start(newCtx);
  });

  function start(ctx: ExtensionCommandContext): void {
    rememberCommandCtx(ctx);
    client?.close();
    client = new HubClient(
      {
        sessionId: ctx.sessionManager.getSessionId(),
        sessionFile: ctx.sessionManager.getSessionFile(),
        sessionName: ctx.sessionManager.getSessionName(),
        cwd: ctx.cwd,
      },
      handleCommand,
      (status, detail) => {
        if (status === "error") {
          ctx.ui.notify(`piper: ${detail}`, "warning");
        }
      },
    );
    client.connect();
  }

  function stop(): void {
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
        ctx.ui.notify(client ? "piper: connecting/connected (see logs for detail)" : "piper: not connected", "info");
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
