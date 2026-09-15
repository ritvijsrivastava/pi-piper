// piper-agent: adds `/rc` to connect the current interactive `pi`
// session to a piper Hub for remote control from your phone. See
// ARCHITECTURE.md (in the piper repo) for the architecture and
// piper-agent/README.md for setup and known limitations.

import type {
  ExtensionAPI,
  ExtensionCommandContext,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { createCommandHandler } from "./command-dispatch.ts";
import { rememberCommandCtx, rememberCtx } from "./context-cache.ts";
import { dropHandoff, hasHandoff, markHandoff } from "./handoff.ts";
import { registerEventForwarding } from "./event-bridge.ts";
import { HubClient } from "./hub-client.ts";

const RC_STATUS_ID = "piper-rc";

export default function (pi: ExtensionAPI) {
  let client: HubClient | undefined;
  let activeStatusContext: ExtensionContext | undefined;
  let activeSessionFile: string | undefined;
  let clientGeneration = 0;

  const handleCommand = createCommandHandler(
    pi,
    async (newCtx) => {
      // Run /rc through the fresh replacement context. The old extension
      // instance is torn down during session replacement, so starting a
      // HubClient from this old closure does not register the new runtime.
      await newCtx.sendUserMessage("/rc", { expandPromptTemplates: true });
    },
    () => {
      // A remote `/new` must follow the same lifecycle as typing `/rc stop`,
      // `/new`, then `/rc`: close the old registration before pi replaces
      // the session, and keep the replacement's shutdown hook from closing
      // the freshly reconnected client. The handoff entry is dropped here so
      // the replacement's session_start hook (which serves locally-typed
      // `/new` etc.) does not reconnect a second time on top of the
      // sendUserMessage("/rc") reconnect above.
      if (activeSessionFile) dropHandoff(activeSessionFile);
      stop();
    },
  );

  function start(ctx: ExtensionContext): void {
    // Only seed the command-context cache when this really is a command
    // context: the session_start auto-reconnect path only has a plain
    // ExtensionContext, and caching that would leave remote
    // new_session/fork/switch_session failing on missing methods.
    if (typeof (ctx as ExtensionCommandContext).newSession === "function") {
      rememberCommandCtx(ctx as ExtensionCommandContext);
    } else {
      rememberCtx(ctx);
    }
    const sessionFile = ctx.sessionManager.getSessionFile();
    if (sessionFile) {
      activeSessionFile = sessionFile;
      // Survives the runtime teardown of /new, /resume, /fork and /reload so
      // the replacement instance's session_start hook can reconnect.
      markHandoff(sessionFile);
    }
    activeStatusContext = ctx;
    ctx.ui.setStatus(RC_STATUS_ID, ctx.ui.theme.fg("warning", "RC: connecting"));
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
          ctx.ui.setStatus(RC_STATUS_ID, ctx.ui.theme.fg("success", "RC: connected"));
          ctx.ui.notify("piper: connected.", "info");
        } else if (status === "error") {
          ctx.ui.setStatus(RC_STATUS_ID, ctx.ui.theme.fg("error", "RC: error"));
          if (lastNotified !== "error") ctx.ui.notify(`piper: ${detail}`, "warning");
        } else if (status === "disconnected") {
          ctx.ui.setStatus(RC_STATUS_ID, ctx.ui.theme.fg("warning", "RC: reconnecting"));
          if (lastNotified === "connected") {
            ctx.ui.notify("piper: disconnected from the Hub \u2013 retrying\u2026", "warning");
          }
        } else if (status === "connecting") {
          ctx.ui.setStatus(RC_STATUS_ID, ctx.ui.theme.fg("warning", "RC: connecting"));
        }
        lastNotified = status;
      },
    );
    client.connect();
  }

  function stop(ctx?: ExtensionContext, options?: { clearHandoff?: boolean }): void {
    clientGeneration += 1;
    const statusContext = ctx ?? activeStatusContext;
    statusContext?.ui.setStatus(RC_STATUS_ID, undefined);
    activeStatusContext = undefined;
    client?.close();
    client = undefined;
    // Only an explicit /rc stop (or quitting pi) ends the handoff; on
    // session replacement the entry must survive — it is how the
    // replacement session's session_start hook learns to reconnect.
    if (options?.clearHandoff && activeSessionFile) dropHandoff(activeSessionFile);
    activeSessionFile = undefined;
  }

  // A locally-typed `/new`, `/resume` or `/fork` (unlike the phone-issued
  // commands, which go through command-dispatch's `withSession` reconnect)
  // tears down and recreates the whole extension runtime, so the reconnect
  // callback never runs for it. `/reload` rebuilds the runtime in place.
  // In all those cases the replacement instance learns it should reconnect
  // from the handoff file written by start() in the outgoing session.
  pi.on("session_start", (event, ctx) => {
    const previous =
      event.reason === "reload" ? ctx.sessionManager.getSessionFile() : event.previousSessionFile;
    if (!previous || !hasHandoff(previous)) return;
    dropHandoff(previous);
    try {
      start(ctx);
      ctx.ui.notify("piper: RC followed the session change — reconnecting…", "info");
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      ctx.ui.notify(`piper: failed to reconnect after session change: ${message}`, "error");
    }
  });

  registerEventForwarding(pi, (event) => client?.sendEvent(event));

  // Lightweight rename (e.g. `/name`); full session replacement (`/new`,
  // `/resume`, `/fork`) reconnects via the `reconnect` callback passed to
  // createCommandHandler (remote commands) or the `session_start` hook
  // above (locally-typed commands, `/reload`), since those tear down and
  // recreate the whole extension runtime.
  pi.on("session_info_changed", () => {
    client?.updateInfo({ sessionName: pi.getSessionName() });
  });

  pi.on("session_shutdown", (event, ctx) => {
    // Leaving pi entirely: end the handoff so a later /resume of this
    // session doesn't silently reconnect RC. For "new"/"resume"/"fork" the
    // entry must survive — the replacement session's session_start hook
    // consumes it to reconnect.
    if (event.reason === "quit") stop(ctx, { clearHandoff: true });
    else stop(ctx);
  });

  pi.registerCommand("rc", {
    description: "Connect this session to a piper Hub for remote control from your phone",
    handler: async (args, ctx) => {
      const sub = args.trim();
      if (sub === "stop") {
        stop(ctx, { clearHandoff: true });
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
