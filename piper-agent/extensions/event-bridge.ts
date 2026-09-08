// Forwards pi's extension-level events to the Hub as pi RPC-shaped JSON
// (SPEC.md §6.2). Most extension events already match `docs/rpc.md`'s
// documented event shapes closely; a few are remapped or reshaped here
// where the interactive-mode extension event differs from the
// documented RPC wire event. See piper-agent/README.md "Known gaps" for
// event types that cannot be forwarded at all (no extension hook
// exists): `queue_update`, `extension_ui_request`/`_response`,
// `auto_retry_*`, `summarization_retry_*`, `extension_error`.

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { rememberCtx } from "./context-cache.ts";

export function registerEventForwarding(pi: ExtensionAPI, forward: (event: unknown) => void): void {
  pi.on("session_start", (_event, ctx) => {
    rememberCtx(ctx);
  });

  pi.on("agent_start", (_event, ctx) => {
    rememberCtx(ctx);
    forward({ type: "agent_start" });
  });

  pi.on("agent_end", (event, ctx) => {
    rememberCtx(ctx);
    // The interactive-mode `agent_end` extension event has no
    // `willRetry` field (unlike the RPC-mode wire event); always
    // reporting `false` here is a known simplification — see README.
    forward({ type: "agent_end", messages: event.messages, willRetry: false });
  });

  pi.on("agent_settled", (_event, ctx) => {
    rememberCtx(ctx);
    forward({ type: "agent_settled" });
  });

  pi.on("turn_start", (_event, ctx) => {
    rememberCtx(ctx);
    forward({ type: "turn_start" });
  });

  pi.on("turn_end", (event, ctx) => {
    rememberCtx(ctx);
    forward({ type: "turn_end", message: event.message, toolResults: event.toolResults });
  });

  pi.on("message_start", (event, ctx) => {
    rememberCtx(ctx);
    forward({ type: "message_start", message: event.message });
  });

  pi.on("message_update", (event, ctx) => {
    rememberCtx(ctx);
    // RPC's `message_update` carries a top-level cumulative `usage` and
    // no `message`; the extension event carries `message` (from which
    // usage is available on assistant messages) and no top-level usage.
    // Reshaped here to match the documented wire event exactly.
    const usage = (event.message as { usage?: unknown } | undefined)?.usage;
    forward({ type: "message_update", usage, assistantMessageEvent: event.assistantMessageEvent });
  });

  pi.on("message_end", (event, ctx) => {
    rememberCtx(ctx);
    forward({ type: "message_end", message: event.message });
  });

  pi.on("tool_execution_start", (event, ctx) => {
    rememberCtx(ctx);
    forward({
      type: "tool_execution_start",
      toolCallId: event.toolCallId,
      toolName: event.toolName,
      args: event.args,
    });
  });

  pi.on("tool_execution_update", (event, ctx) => {
    rememberCtx(ctx);
    forward({
      type: "tool_execution_update",
      toolCallId: event.toolCallId,
      toolName: event.toolName,
      args: event.args,
      partialResult: event.partialResult,
    });
  });

  pi.on("tool_execution_end", (event, ctx) => {
    rememberCtx(ctx);
    forward({
      type: "tool_execution_end",
      toolCallId: event.toolCallId,
      toolName: event.toolName,
      result: event.result,
      isError: event.isError,
    });
  });

  // pi's RPC mode emits `compaction_start`/`compaction_end`; interactive
  // mode's equivalents are named and shaped differently. Remapped here
  // for wire compatibility with docs/rpc.md and the PWA client.
  pi.on("session_before_compact", (event, ctx) => {
    rememberCtx(ctx);
    forward({ type: "compaction_start", reason: event.reason });
  });

  pi.on("session_compact", (event, ctx) => {
    rememberCtx(ctx);
    forward({
      type: "compaction_end",
      reason: event.reason,
      result: {
        summary: event.compactionEntry.summary,
        firstKeptEntryId: event.compactionEntry.firstKeptEntryId,
        tokensBefore: event.compactionEntry.tokensBefore,
        usage: event.compactionEntry.usage,
        details: event.compactionEntry.details,
      },
      aborted: false,
      willRetry: event.willRetry,
    });
  });

  pi.on("session_compact_failed", (event, ctx) => {
    rememberCtx(ctx);
    forward({
      type: "compaction_end",
      reason: event.reason,
      result: null,
      aborted: event.aborted,
      willRetry: event.willRetry,
      errorMessage: event.errorMessage,
    });
  });
}
