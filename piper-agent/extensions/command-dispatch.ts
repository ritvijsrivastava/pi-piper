// Executes an inbound pi RPC-shaped command (SPEC.md §6.1/§8) against
// the real running session via `pi`'s extension API, and returns an
// RPC-shaped response. This is the heart of "the phone controls the
// same live session your terminal is showing" — see SPEC.md §8 for the
// full command -> extension-API mapping table this file implements.
//
// Commands marked unsupported() below have no confirmed extension-API
// equivalent as of this pi version; see piper-agent/README.md "Known
// gaps". They fail cleanly with a clear error rather than silently
// no-op-ing.

import type { ExtensionAPI, ExtensionCommandContext } from "@earendil-works/pi-coding-agent";
import { getCommandCtx, getCtx, rememberCommandCtx } from "./context-cache.ts";

type Command = { type: string; [key: string]: unknown };
type Response = { type: "response"; command: string; success: boolean; data?: unknown; error?: string };

const THINKING_LEVEL_ORDER = ["off", "minimal", "low", "medium", "high", "xhigh", "max"] as const;

function ok(command: string, data?: unknown): Response {
  return { type: "response", command, success: true, data };
}

function fail(command: string, error: string): Response {
  return { type: "response", command, success: false, error };
}

function unsupported(command: string): Response {
  return fail(
    command,
    `"${command}" is not implemented yet for piper-agent (remote) sessions — see piper-agent/README.md "Known gaps"`,
  );
}

/** `reconnect` re-registers under a replacement session's context after
 * `/new`, `/fork`, or `switch_session` — see index.ts. */
export type ReconnectFn = (ctx: ExtensionCommandContext) => void;

export function createCommandHandler(pi: ExtensionAPI, reconnect: ReconnectFn) {
  return async function handle(raw: unknown): Promise<Response> {
    const command = raw as Command;
    const type = command?.type ?? "unknown";
    try {
      switch (type) {
        case "prompt":
          return handlePrompt(pi, command);
        case "steer":
          pi.sendUserMessage(command.message as string, { deliverAs: "steer" });
          return ok(type);
        case "follow_up":
          pi.sendUserMessage(command.message as string, { deliverAs: "followUp" });
          return ok(type);
        case "abort":
          getCtx().abort();
          return ok(type);
        case "get_state":
          return ok(type, buildState());
        case "get_messages":
          return ok(type, { messages: buildMessages() });
        case "get_commands":
          return ok(type, { commands: pi.getCommands() });
        case "set_model":
          return await handleSetModel(pi, command);
        case "cycle_model":
          return await handleCycleModel(pi);
        case "get_available_models":
          return ok(type, { models: getCtx().modelRegistry.getAvailable() });
        case "set_thinking_level":
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          pi.setThinkingLevel(command.level as any);
          return ok(type);
        case "cycle_thinking_level":
          return handleCycleThinkingLevel(pi);
        case "get_available_thinking_levels":
          return ok(type, { levels: getAvailableThinkingLevels() });
        case "compact":
          return await handleCompact(command);
        case "set_session_name":
          pi.setSessionName(command.name as string);
          return ok(type);
        case "get_session_name":
          return ok(type, { name: pi.getSessionName() });
        case "get_session_stats":
          return ok(type, buildSessionStats());
        case "get_fork_messages":
          return ok(type, { messages: buildForkMessages() });
        case "get_entries":
          return ok(type, buildEntries(command.since as string | undefined));
        case "get_tree":
          return ok(type, {
            tree: getCtx().sessionManager.getTree(),
            leafId: getCtx().sessionManager.getLeafId(),
          });
        case "get_last_assistant_text":
          return ok(type, { text: getLastAssistantText() });
        case "new_session":
          return await handleNewSession(reconnect);
        case "switch_session":
          return await handleSwitchSession(command, reconnect);
        case "fork":
          return await handleFork(command, reconnect);
        case "bash":
          return await handleBash(pi, command);
        case "clear_queue":
        case "set_steering_mode":
        case "set_follow_up_mode":
        case "set_auto_compaction":
        case "set_auto_retry":
        case "abort_retry":
        case "abort_bash":
        case "export_html":
          return unsupported(type);
        default:
          return fail(type, `unknown command type: ${String(type)}`);
      }
    } catch (err) {
      return fail(type, err instanceof Error ? err.message : String(err));
    }
  };
}

function extractText(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .filter((block: { type?: string }) => block?.type === "text")
      .map((block: { text?: string }) => block.text ?? "")
      .join(" ");
  }
  return "";
}

function handlePrompt(pi: ExtensionAPI, command: Command): Response {
  const message = command.message as string;
  // RPC's `ImageContent` ({type:"image", data, mimeType}) matches
  // pi-ai's `ImageContent` shape exactly, so these pass through as-is.
  const images = command.images as { type: "image"; data: string; mimeType: string }[] | undefined;
  const content = images && images.length > 0 ? [{ type: "text" as const, text: message }, ...images] : message;
  const deliverAs = command.streamingBehavior as "steer" | "followUp" | undefined;
  pi.sendUserMessage(content, { deliverAs, expandPromptTemplates: true });
  return ok("prompt");
}

function buildState() {
  const ctx = getCtx();
  const sm = ctx.sessionManager;
  const messageCount = sm.buildContextEntries().filter((e) => e.type === "message").length;
  return {
    model: ctx.model ?? null,
    thinkingLevel: ctx.thinkingLevel ?? "off",
    isStreaming: !ctx.isIdle(),
    // Not observable via the extension API as of this pi version;
    // reported as a fixed best guess rather than omitted, since PWA
    // code expects the field to exist. See README "Known gaps".
    isCompacting: false,
    steeringMode: "all",
    followUpMode: "one-at-a-time",
    autoCompactionEnabled: true,
    sessionFile: sm.getSessionFile(),
    sessionId: sm.getSessionId(),
    sessionName: sm.getSessionName(),
    messageCount,
    // ctx.hasPendingMessages() is boolean-only; approximated as 0/1
    // rather than the true queue length. See README "Known gaps".
    pendingMessageCount: ctx.hasPendingMessages() ? 1 : 0,
  };
}

function buildMessages(): unknown[] {
  return getCtx()
    .sessionManager.buildContextEntries()
    .filter((e) => e.type === "message")
    .map((e) => (e as { message: unknown }).message);
}

async function handleSetModel(pi: ExtensionAPI, command: Command): Promise<Response> {
  const ctx = getCtx();
  const provider = command.provider as string;
  const modelId = command.modelId as string;
  const model = ctx.modelRegistry.getAvailable().find((m) => m.provider === provider && m.id === modelId);
  if (!model) return fail("set_model", `model not found: ${provider}/${modelId}`);
  const success = await pi.setModel(model);
  if (!success) return fail("set_model", `authentication not configured for ${provider}`);
  return ok("set_model", model);
}

async function handleCycleModel(pi: ExtensionAPI): Promise<Response> {
  const ctx = getCtx();
  const pool = ctx.scopedModels.length > 0 ? ctx.scopedModels.map((s) => s.model) : ctx.modelRegistry.getAvailable();
  if (pool.length <= 1) return ok("cycle_model", null);
  const current = ctx.model;
  const currentIndex = current ? pool.findIndex((m) => m.provider === current.provider && m.id === current.id) : -1;
  const next = pool[(currentIndex + 1) % pool.length];
  await pi.setModel(next);
  return ok("cycle_model", {
    model: next,
    thinkingLevel: ctx.thinkingLevel ?? "off",
    isScoped: ctx.scopedModels.length > 0,
  });
}

function getAvailableThinkingLevels(): string[] {
  const ctx = getCtx();
  const model = ctx.model;
  if (!model || !model.reasoning) return ["off"];
  const map = (model.thinkingLevelMap ?? {}) as Record<string, unknown>;
  return THINKING_LEVEL_ORDER.filter((level) => level === "off" || map[level] !== null);
}

function handleCycleThinkingLevel(pi: ExtensionAPI): Response {
  const ctx = getCtx();
  const levels = getAvailableThinkingLevels();
  if (levels.length <= 1) return ok("cycle_thinking_level", null);
  const currentIndex = levels.indexOf(ctx.thinkingLevel ?? "off");
  const next = levels[(currentIndex + 1) % levels.length];
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  pi.setThinkingLevel(next as any);
  return ok("cycle_thinking_level", { level: next });
}

function handleCompact(command: Command): Promise<Response> {
  const ctx = getCtx();
  return new Promise((resolve) => {
    ctx.compact({
      customInstructions: command.customInstructions as string | undefined,
      onComplete: (result) => resolve(ok("compact", result)),
      onError: (err) => resolve(fail("compact", err.message)),
    });
  });
}

function buildSessionStats() {
  const ctx = getCtx();
  const sm = ctx.sessionManager;
  const entries = sm.buildContextEntries();
  let userMessages = 0;
  let assistantMessages = 0;
  let toolCalls = 0;
  let toolResults = 0;
  let input = 0;
  let output = 0;
  let cacheRead = 0;
  let cacheWrite = 0;
  let cost = 0;

  for (const entry of entries) {
    if (entry.type !== "message") continue;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const message = (entry as any).message;
    const usage = message.usage as
      | { input?: number; output?: number; cacheRead?: number; cacheWrite?: number; cost?: { total?: number } }
      | undefined;

    if (message.role === "user") userMessages++;
    if (message.role === "assistant") {
      assistantMessages++;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      toolCalls += (message.content ?? []).filter((b: any) => b?.type === "toolCall").length;
    }
    if (message.role === "toolResult") toolResults++;

    if (usage) {
      input += usage.input ?? 0;
      output += usage.output ?? 0;
      cacheRead += usage.cacheRead ?? 0;
      cacheWrite += usage.cacheWrite ?? 0;
      cost += usage.cost?.total ?? 0;
    }
  }

  const contextUsage = ctx.getContextUsage();

  // Note: this only aggregates the *active branch* (buildContextEntries),
  // not the full session including abandoned branches/pre-compaction
  // history the way RPC mode's get_session_stats does. See README.
  return {
    sessionFile: sm.getSessionFile(),
    sessionId: sm.getSessionId(),
    userMessages,
    assistantMessages,
    toolCalls,
    toolResults,
    totalMessages: entries.length,
    tokens: { input, output, cacheRead, cacheWrite, total: input + output + cacheRead + cacheWrite },
    cost,
    contextUsage: contextUsage
      ? { tokens: contextUsage.tokens, contextWindow: contextUsage.contextWindow, percent: contextUsage.percent }
      : undefined,
  };
}

function buildForkMessages() {
  return getCtx()
    .sessionManager.getEntries()
    .filter((e) => e.type === "message" && (e as { message: { role: string } }).message.role === "user")
    .map((e) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const message = (e as any).message;
      return { entryId: e.id, text: extractText(message.content) };
    });
}

function buildEntries(since?: string) {
  const sm = getCtx().sessionManager;
  const all = sm.getEntries();
  const leafId = sm.getLeafId();
  if (!since) return { entries: all, leafId };
  const idx = all.findIndex((e) => e.id === since);
  if (idx === -1) throw new Error(`no entry with id ${since}`);
  return { entries: all.slice(idx + 1), leafId };
}

function getLastAssistantText(): string | null {
  const messages = buildMessages();
  for (let i = messages.length - 1; i >= 0; i--) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const message = messages[i] as any;
    if (message.role === "assistant") return extractText(message.content);
  }
  return null;
}

async function handleNewSession(reconnect: ReconnectFn): Promise<Response> {
  const ctx = getCommandCtx();
  const result = await ctx.newSession({
    withSession: async (newCtx) => {
      rememberCommandCtx(newCtx);
      reconnect(newCtx);
    },
  });
  return ok("new_session", result);
}

async function handleSwitchSession(command: Command, reconnect: ReconnectFn): Promise<Response> {
  const ctx = getCommandCtx();
  const result = await ctx.switchSession(command.sessionPath as string, {
    withSession: async (newCtx) => {
      rememberCommandCtx(newCtx);
      reconnect(newCtx);
    },
  });
  return ok("switch_session", result);
}

async function handleFork(command: Command, reconnect: ReconnectFn): Promise<Response> {
  const ctx = getCommandCtx();
  const result = await ctx.fork(command.entryId as string, {
    position: command.position as "before" | "at" | undefined,
    withSession: async (newCtx) => {
      rememberCommandCtx(newCtx);
      reconnect(newCtx);
    },
  });
  // rpc.md's `fork` response also includes `text` (the original prompt
  // text being forked from); ExtensionCommandContext.fork() doesn't
  // return it, so it's omitted here. See README "Known gaps".
  return ok("fork", result);
}

async function handleBash(pi: ExtensionAPI, command: Command): Promise<Response> {
  const ctx = getCtx();
  // Best-effort: runs the command and returns output, but — unlike
  // RPC mode's native `bash` command — does not add a
  // BashExecutionMessage into session context (no extension API for
  // that). See README "Known gaps". `abort_bash` is intentionally
  // unsupported since no cancellation handle is retained here.
  const result = await pi.exec("bash", ["-lc", command.command as string], { cwd: ctx.cwd });
  return ok("bash", {
    output: result.stdout + (result.stderr ? `\n${result.stderr}` : ""),
    exitCode: result.code,
    cancelled: result.killed,
    truncated: false,
  });
}
