# Piper: Multi-Session Remote Control — SPEC

Status: **draft, not implemented**. This document describes the target
architecture for Piper v2. It supersedes the single-project,
spawn-a-child-process model described in the current `README.md`. Nothing
in this file has been built yet; see [Milestones](#milestones) for the
implementation order.

## 1. Goals

1. Run `pi` normally, interactively, in a terminal on your desktop —
   full native TUI, no feature loss.
2. From that same terminal, opt a session into remote control with a
   slash command (`/rc`), without restarting it.
3. Control that same, live, already-running session from a phone
   browser (PWA) over a private Tailscale network — prompts, steering,
   aborts, model switches, etc. — synced in both directions in real
   time.
4. Support multiple such sessions at once (multiple terminals, possibly
   in the same project or different projects) and let the phone list
   and switch between them, similar to Claude's mobile chat list.
5. Slash commands (extension commands, skills, prompt templates) work
   from the phone, with autocomplete.
6. Keep the "no public hosting, Tailscale-only, defense-in-depth token"
   security posture of Piper v1.

## 2. Non-goals (for this iteration)

- Spawning brand-new `pi` processes remotely from the phone (starting a
  session for a project that has no terminal open at all). The old
  spawn-a-child-process mode is kept as an opt-in fallback (see
  [§10](#10-backward-compatibility-headless-mode)) for this use case,
  but is not the primary flow.
- Web Push notifications (background session finished while app is
  closed). Flagged as a stretch milestone, not required for v2.
- Mirroring `ctx.ui` dialogs raised by *other* project extensions to the
  phone when no one is at the terminal. Known limitation, same as
  Piper v1.
- Multi-user access control (this is a single-operator, single-tailnet
  tool; concurrent input from terminal + phone is "last one wins", not
  a locking/turn-taking model).

## 3. Architecture overview

```
Desktop terminal A                     Desktop terminal B
┌───────────────────────────┐          ┌───────────────────────────┐
│ pi (interactive TUI)      │          │ pi (interactive TUI)      │
│ + piper-agent extension   │          │ + piper-agent extension   │
│ /rc → outbound WS ────────┼───┐      │ /rc → outbound WS ────────┼───┐
└───────────────────────────┘   │      └───────────────────────────┘   │
                                 ▼                                     ▼
                    ┌──────────────────────────────────────────────────┐
                    │                  Piper Hub (Rust/axum)            │
                    │                                                    │
                    │  Session Registry: SessionId -> AgentLink + meta   │
                    │                                                    │
                    │  /agent            (agent-token, extensions dial in)│
                    │  /ws?session=<id>  (phone-token, RPC relay)         │
                    │  /ws/control       (phone-token, list/status push)  │
                    │  /api/sessions     (phone-token, HTTP snapshot)      │
                    │  /            (PWA static assets)                   │
                    └───────────────────────┬────────────────────────────┘
                                            │ wss (tailscale serve)
                                            ▼
                                  Phone (PWA, installed as app)
                                  - Session list screen
                                  - Chat screen (per session)
```

Key property: the terminal and the phone are two independent clients of
the **same live pi process**. Piper relays pi's own documented RPC
protocol (`pi`'s `docs/rpc.md`) verbatim in both directions wherever
possible; Piper/`piper-agent` only add a thin registration/listing layer
on top, not a second application protocol.

## 4. Terminology

- **Session**: one running `pi` process (interactive TUI or headless),
  identified by its session id / session file (`get_state.sessionId` /
  `sessionFile`), not by project path. Two terminals in the same repo
  are two different sessions.
- **Agent link**: the transport-level connection between the Hub and
  one running `pi` process. Two implementations:
  - *WebSocket agent link*: backed by `piper-agent`'s outbound
    connection from an interactive session (new in v2).
  - *Child-process agent link*: backed by a spawned `pi --mode rpc`
    child over stdio (existing Piper v1 behavior, kept as fallback).
- **Hub**: the always-on Rust service (`piper`), now a switchboard
  rather than a single-process supervisor.
- **`piper-agent`**: new pi extension, installed globally, providing
  `/rc` and the agent-side half of the bridge.

## 5. Components

### 5.1 Piper Hub (Rust)

Evolves the existing crate. Suggested module layout (relative to
current code):

```
src/
  config.rs           # + Hub-level config: bind, phone token, agent token path
  state.rs            # AppState now holds the SessionRegistry
  registry.rs          # NEW: SessionId -> SessionHandle map, register/unregister
  rpc/
    mod.rs             # unchanged (JSONL framing helpers)
    agent_link.rs       # NEW: trait AgentLink { subscribe(), send(), meta() }
    process_link.rs     # RENAMED from process.rs: child-process AgentLink impl
    ws_link.rs          # NEW: WebSocket-backed AgentLink impl (Hub side of /agent)
  server/
    mod.rs              # router: /agent, /ws, /ws/control, /api/sessions, static
    auth.rs              # + separate check for agent-token vs phone-token
    agent.rs             # NEW: /agent WS upgrade handler (registration handshake)
    ws.rs                 # CHANGED: looks up session by id from registry
    control.rs             # NEW: /ws/control push handler
    sessions_api.rs         # NEW: GET /api/sessions handler
    static_files.rs         # unchanged
```

`AppState`:

```rust
struct AppState {
    registry: Arc<SessionRegistry>,   // replaces the single Arc<PiProcess>
    phone_token: Arc<str>,
    agent_token: Arc<str>,
}
```

`SessionRegistry` responsibilities:
- `register(link: Arc<dyn AgentLink>, meta: SessionMeta) -> SessionId`
- `unregister(id: SessionId)` (on agent WS close, or child process exit)
- `update_meta(id: SessionId, patch: SessionMetaPatch)` (for
  `session_info_changed`)
- `list() -> Vec<SessionSummary>`
- `get(id) -> Option<Arc<dyn AgentLink>>`
- Passive status tracking: watches events flowing through each link
  (it already has to parse them to broadcast) to derive
  `is_streaming`, `last_activity_at`, `last_preview` without a second
  subscription — see [§6.3](#63-control-channel-hub--phone-wscontrol).
- Broadcasts registry-change notifications to any `/ws/control`
  subscribers.

### 5.2 `piper-agent` (new pi extension, TypeScript)

Installed globally (`~/.pi/agent/extensions/piper-agent/`, or via
`pi install git:<you>/piper-agent` / `npm:piper-agent`) so it is active
in every project without per-project setup. Suggested layout:

```
piper-agent/
  package.json          # pi manifest, npm deps ("ws" or rely on global WebSocket)
  extensions/
    index.ts             # default export: registers /rc command + hooks
    hub-client.ts          # WS client: connect, reconnect/backoff, register
    event-bridge.ts         # pi.on(...) -> forward as RPC-shaped JSON
    command-dispatch.ts       # inbound Hub command -> pi/ctx API calls
    token.ts                   # read agent-token from local file
```

Behavior:
- `pi.registerCommand("rc", { handler })`:
  - `/rc` (no args): connect to the Hub and register. Default Hub
    address `ws://127.0.0.1:4390/agent`; overridable via
    `PIPER_HUB_URL` env var or `.pi/settings.json` key
    `piperAgent.hubUrl` for machines where the Hub isn't local.
  - `/rc status`: report connection state, Hub URL, session id.
  - `/rc stop`: disconnect and unregister (also happens automatically
    on `session_shutdown`).
  - Optional future flag: `/rc --auto` semantics via a setting that
    calls the same handler from a `session_start` hook, so every
    session self-registers without typing `/rc` each time. Not default
    in v2 (explicit opt-in per session is safer/less surprising).
- Reads the agent token from disk at connect time (never prompts,
  never stored in the session file). See [§7](#7-security-model).
- On connect, sends the `register` handshake ([§6.1](#61-agent-registration-protocol-hub--extension-agent)).
- Subscribes via `pi.on(...)` to the full event set Piper needs to
  mirror (see table in [§6.2](#62-relayed-rpc-protocol-unchanged-shape)) and forwards each
  as-is to the Hub.
- Forwards `session_info_changed` as a `meta_update` message
  ([§6.1](#61-agent-registration-protocol-hub--extension-agent)) instead of re-registering, so the Hub can
  patch the existing registry entry in place.
- Listens for inbound `command` envelopes from the Hub and dispatches
  them per the mapping table in [§8](#8-command-mapping-table).
- Reconnects with exponential backoff (e.g. 1s, 2s, 5s, 10s, capped) if
  the Hub connection drops; re-sends `register` on reconnect. Hub-side
  state is not persisted across Hub restarts — it is rebuilt purely
  from active connections.

### 5.3 Mobile/Web PWA client (`assets/`)

Two screens (see [§9](#9-mobile-pwa-ux-spec) for full UX spec):
1. **Session list** (new): fed by `/api/sessions` (initial snapshot) +
   `/ws/control` (live updates). Claude-style chat list.
2. **Chat view** (evolution of today's single-screen client): connects
   to `/ws?session=<id>`, same RPC relay as Piper v1's `/ws`, plus
   slash-command autocomplete, extension UI dialog rendering, and
   richer message rendering.

Routing via the History API (`#/sessions`, `#/session/<id>`) so back
gesture/back button behaves like a native app.

## 6. Protocols

All protocols are newline-free JSON messages over WebSocket text
frames (standard `JSON.stringify`/`JSON.parse`, no custom framing needed
since browsers/WS already frame messages — unlike the child-process
stdio transport, which still uses the JSONL framing in `rpc/mod.rs`).

### 6.1 Agent registration protocol (Hub ↔ extension, `/agent`)

New envelope, not part of pi's own RPC protocol. Kept intentionally
tiny.

Connect: `wss://<hub>/agent?token=<agent_token>`. Rejected with `401` if
the token doesn't match (see [§7](#7-security-model)).

**Extension → Hub, on connect:**
```json
{
  "type": "register",
  "sessionId": "abc123",
  "sessionFile": "/home/user/.pi/agent/sessions/.../abc123.jsonl",
  "sessionName": "Refactor auth module",
  "cwd": "/home/user/Code/piper",
  "pid": 84213,
  "startedAt": "2024-06-01T12:00:00Z"
}
```
Hub responds:
```json
{"type": "registered", "sessionId": "abc123"}
```
(If a session with this id is already registered — e.g. extension
reconnect race — the Hub replaces the old link and keeps the same
`sessionId`.)

**Extension → Hub, on `session_info_changed`:**
```json
{
  "type": "meta_update",
  "sessionId": "abc123",
  "patch": {"sessionFile": "...", "sessionName": "...", "cwd": "..."}
}
```

**Extension → Hub, event forwarding:** every subscribed pi event,
wrapped so the Hub knows which session it belongs to (the Hub owns one
WS per agent, so this wrapping is actually redundant over the wire
*if* we keep one `/agent` connection per session — see Implementation
Note below — but is included here for symmetry with the two-way
command channel and to keep the message self-describing):
```json
{"type": "event", "sessionId": "abc123", "event": { /* verbatim pi RPC event JSON, e.g. message_update */ }}
```

**Hub → Extension: commands** (mirrors pi's RPC commands 1:1, plus a
few v2-only ones — see [§8](#8-command-mapping-table)):
```json
{"type": "command", "id": "req-1", "command": {"type": "prompt", "message": "..."}}
```

**Extension → Hub: command responses:**
```json
{"type": "response", "id": "req-1", "response": { /* verbatim RPC-shaped response */ }}
```

*Implementation note:* since each `/agent` WS connection represents
exactly one session, the Hub does not strictly need `sessionId` on
every frame — it can tag messages by connection identity internally.
The `sessionId` field is included in the wire format anyway so the
protocol is self-describing (easier to debug/log/replay) and so a
future multi-session-per-connection optimization is possible without a
breaking change.

### 6.2 Relayed RPC protocol (unchanged shape)

Everything pi's `docs/rpc.md` defines — commands, responses, events,
extension UI request/response — is reused verbatim as the payload
inside `command`/`event`/`response` envelopes above, and verbatim as
the direct payload on the phone-facing `/ws?session=<id>` (which stays
byte-for-byte compatible with Piper v1's `/ws`). Piper does not
redefine or version this protocol; pi's own docs remain the single
reference.

Event types `piper-agent` subscribes to and forwards (superset of what
a spawned `pi --mode rpc` would emit, since interactive-mode events are
equivalent):

| Event | Forwarded for |
|---|---|
| `agent_start` / `agent_end` / `agent_settled` | streaming status, chat view |
| `turn_start` / `turn_end` | chat view |
| `message_start` / `message_update` / `message_end` | chat view (live text/tool deltas) |
| `tool_execution_start` / `_update` / `_end` | chat view (tool cards) |
| `queue_update` | chat view (pending steering/follow-up indicator) |
| `compaction_start` / `compaction_end` | chat view |
| `extension_ui_request` (+ `extension_ui_response` inbound) | chat view dialogs |
| `session_info_changed` | forwarded as `meta_update` (§6.1), not a raw event |

### 6.3 Control channel (Hub ↔ phone, `/ws/control`)

Connect: `wss://<hub>/ws/control?token=<phone_token>`. Purely
Hub-authored; no commands flow phone → Hub on this channel other than
an optional `ping`.

**Initial snapshot on connect:**
```json
{
  "type": "sessions_snapshot",
  "sessions": [
    {
      "sessionId": "abc123",
      "sessionName": "Refactor auth module",
      "cwd": "/home/user/Code/piper",
      "connected": true,
      "isStreaming": true,
      "lastActivityAt": "2024-06-01T12:03:41Z",
      "lastPreview": "Now updating the ws.rs handler to...",
      "model": {"provider": "anthropic", "id": "claude-sonnet-4-20250514"}
    }
  ]
}
```

**Incremental updates thereafter:**
```json
{"type": "session_connected", "session": { /* same shape as above item */ }}
{"type": "session_disconnected", "sessionId": "abc123"}
{"type": "session_status", "sessionId": "abc123", "isStreaming": true, "lastActivityAt": "..."}
{"type": "session_preview", "sessionId": "abc123", "lastPreview": "...", "lastActivityAt": "..."}
{"type": "session_meta", "sessionId": "abc123", "sessionName": "...", "cwd": "..."}
```

`isStreaming` is derived by the Hub from `agent_start`/`agent_settled`
on that session's agent link. `lastPreview` is derived from the text
content of the most recent `message_end` (assistant) or the prompt of
the most recent `turn_start` (user), truncated to ~120 chars. Both are
display-only derivations — the Hub does not persist or index full
message history; `get_messages` on the session's own `/ws` connection
remains the source of truth for the full transcript.

`session_disconnected` does not delete the entry from the phone's list
immediately — the client keeps showing it (dimmed, "disconnected") for
recent-history continuity; it is removed from the *Hub's* registry
immediately, so re-selecting it would fail until the terminal
reconnects.

### 6.4 Phone session WebSocket (`/ws?session=<id>`)

Same contract as Piper v1's `/ws`, with one addition: the `session`
query parameter selects which registry entry to bridge to. If omitted
and exactly one session is registered, default to it (keeps the
single-session case as simple as v1). If omitted and zero or multiple
sessions are registered, reject with `400` and a body pointing at
`/api/sessions` — the PWA should never hit this path in practice since
it always picks a session first, but the API should not silently guess
in the ambiguous case.

Everything sent/received after upgrade is exactly pi's RPC JSON,
unchanged from v1.

### 6.5 HTTP snapshot (`GET /api/sessions`)

Auth: `?token=<phone_token>` (same as `/ws`) or `Authorization: Bearer
<phone_token>` header — used for the initial page load before the
control WS is open, and as a fallback if WS is unavailable.

Response: same array shape as `sessions_snapshot.sessions` in
[§6.3](#63-control-channel-hub--phone-wscontrol).

## 7. Security model

Two independent shared secrets, because `tailscale serve` proxies
connections through loopback, which means **peer-address checks cannot
distinguish "a local `pi` extension" from "a request proxied in from
the tailnet"** once `tailscale serve` is in front of the Hub — both
arrive at axum looking like they came from `127.0.0.1`. This is a
correction versus a naive design that tries to "only allow `/agent`
from localhost."

| Secret | Purpose | Where it lives | Who sees it |
|---|---|---|---|
| `PIPER_TOKEN` (phone token) | Gates `/ws`, `/ws/control`, `/api/sessions` | Env var / systemd unit file, pasted into phone browser once | You (typed into your phone) |
| `PIPER_AGENT_TOKEN` (agent token) | Gates `/agent` | Generated by the Hub on first run, written to a local file `0600` owned by the desktop user (default `~/.pi/agent/piper/agent-token`) | Never leaves the desktop machine; never sent to the phone; not in any URL the phone opens |

`piper-agent` reads the agent token straight from disk at connect
time — no manual copy/paste, no prompt. If the file doesn't exist yet
(Hub never run on this machine), `/rc` fails with a clear error telling
the user to start the Hub first.

Everything else about Piper v1's posture is unchanged:
- Hub binds `127.0.0.1` only.
- Exposed to the tailnet exclusively via `tailscale serve` (never
  `tailscale funnel`).
- Tokens are defense-in-depth on top of Tailscale's network-level
  access control, not the primary boundary.

### 7.1 Optional: Tailscale identity headers instead of the phone token

When `tailscale serve` proxies a request to the Hub, `tailscaled`
itself has already resolved the caller's tailnet identity (it's how it
authenticated the connection in the first place) and stamps it onto the
forwarded HTTP request as a `Tailscale-User-Login` header (plus
`Tailscale-User-Name` / `Tailscale-User-Profile-Pic`, unused here) —
confirmed against the `tailscaled` binary's `addTailscaleIdentityHeaders`
function. It also stamps `Tailscale-Funnel-Request` on anything that
arrived via Funnel rather than tailnet-only Serve.

Set `--allowed-tailscale-login` (repeatable) or
`PIPER_ALLOWED_TAILSCALE_LOGINS` (comma-separated) to a list of tailnet
logins (e.g. `alice@github`) to let `/ws`, `/ws/control`, and
`/api/sessions` accept a matching `Tailscale-User-Login` header as an
**additional**, independent way in, on top of (not instead of)
`PIPER_TOKEN` — either one authorizes the request. Requests carrying
`Tailscale-Funnel-Request` are always rejected on this path regardless
of login, as a belt-and-suspenders measure given Piper's Funnel-never
posture above. Leaving the allowlist empty (the default) disables this
path entirely; `PIPER_TOKEN` remains required exactly as before.

**Caveat, and why this isn't the default:** the Hub trusts this header
at face value. That's only sound because it binds to loopback and is
meant to be reached exclusively through `tailscale serve`'s proxy — but
unlike a real reverse-proxy setup with a Unix socket only `tailscaled`
can write to, Piper has no way to cryptographically distinguish a
connection `tailscaled` actually proxied in from any other local
process on the same machine that opens a loopback connection and sets
the same header itself. Only enable `--allowed-tailscale-login` on a
machine where every local user is already trusted with full control of
your `pi` sessions (true of most single-user desktops, the primary
target here). The heavier alternative that closes this gap — making the
Hub its own tailnet node via `tsnet` instead of sitting behind
`tailscale serve`, so it can call `LocalAPI`'s `WhoIs` against a real
tailnet peer address rather than trusting a header — is not implemented.

## 8. Command mapping table

Commands the phone can send over `/ws?session=<id>` (unchanged RPC
shape) and how `piper-agent` executes each one inside the real
interactive session. "RPC mode" column notes whether stock
`pi --mode rpc` (headless fallback link) can also serve this command,
for parity tracking.

| Command | `piper-agent` implementation | RPC mode (headless fallback) |
|---|---|---|
| `prompt` | `pi.sendUserMessage(message, { deliverAs, images, expandPromptTemplates: true })` | ✅ native |
| `steer` | `pi.sendUserMessage(message, { deliverAs: "steer" })` | ✅ native |
| `follow_up` | `pi.sendUserMessage(message, { deliverAs: "followUp" })` | ✅ native |
| `abort` | `ctx.abort()`, then wait via `ctx.isIdle()` polling or `ctx.waitForIdle()` | ✅ native |
| `clear_queue` | not directly exposed by extension API — mark as **v2 gap**, investigate `ctx.sessionManager` queue accessors before implementing | ✅ native |
| `new_session` | `ctx.newSession({ withSession })` | ✅ native |
| `get_state` | assemble from `ctx.model`, `ctx.thinkingLevel`, `ctx.isIdle()`, `ctx.sessionManager`, `pi.getSessionName()` | ✅ native |
| `get_messages` | `ctx.sessionManager.buildContextEntries()` (or equivalent messages accessor) | ✅ native |
| `set_model` / `cycle_model` | `pi.setModel(...)`; cycle = compute next from `ctx.modelRegistry`/`ctx.scopedModels` | ✅ native |
| `get_available_models` | `ctx.modelRegistry.getAvailable()` / `ctx.scopedModels` | ✅ native |
| `set_thinking_level` / `cycle_thinking_level` | `pi.setThinkingLevel(...)` | ✅ native |
| `get_available_thinking_levels` | derive from `ctx.model` capabilities | ✅ native |
| `set_steering_mode` / `set_follow_up_mode` | investigate extension-level equivalent; if absent, **v2 gap** | ✅ native |
| `compact` | `ctx.compact({ customInstructions, onComplete, onError })` | ✅ native |
| `set_auto_compaction` / `set_auto_retry` / `abort_retry` | investigate extension-level equivalents; likely **v2 gap**, low priority | ✅ native |
| `bash` / `abort_bash` | out of scope for v2 (no documented extension-level equivalent of direct RPC `bash`); use the model's own `bash` tool instead | ✅ native |
| `get_session_stats` | derive from `ctx.sessionManager` + `ctx.getContextUsage()` | ✅ native |
| `export_html` | investigate extension-level equivalent; else **v2 gap** | ✅ native |
| `switch_session` | `ctx.switchSession(path, { withSession })` | ✅ native |
| `fork` / `clone` | `ctx.fork(entryId, { position })` | ✅ native |
| `get_fork_messages` | derive from `ctx.sessionManager.getEntries()` | ✅ native |
| `get_entries` / `get_tree` | derive from `ctx.sessionManager` | ✅ native |
| `get_last_assistant_text` | derive from `ctx.sessionManager` | ✅ native |
| `set_session_name` / `get_session_name` | `pi.setSessionName(...)` / `pi.getSessionName()` | ✅ native |
| `get_commands` | `pi.getCommands()` | ✅ native |
| `extension_ui_response` | route to the same in-process mechanism the TUI uses for dialog responses (needs investigation — this may require pi core support for "resolve a pending `ctx.ui` dialog from an external responder"; possible **v2 gap**, flag as a risk) | ✅ native |

Entries marked **v2 gap** are commands whose extension-API equivalent is
not confirmed to exist from documentation alone; each needs a short
spike against pi's actual extension types before implementation
(milestone 3). Where a gap is confirmed unfixable at the extension
level, the phone UI degrades gracefully (hide the affected control) for
WS-agent-linked sessions, while it stays fully available for
child-process-linked (headless) sessions.

## 9. Mobile PWA UX spec

### 9.1 Session list (home screen)

```
┌─────────────────────────────┐
│  🔍  Search sessions          │
├─────────────────────────────┤
│ ● Refactor auth module        │  ← green pulsing dot: isStreaming
│   ~/Code/piper · 2s ago       │
│   "Now updating the ws.rs..." │
├─────────────────────────────┤
│   Fix flaky test              │  ← solid gray dot: connected, idle
│   ~/Code/other-repo · 4m ago  │
│   "Done — 3 tests fixed."     │
├─────────────────────────────┤
│ ○ Explore new API design      │  ← hollow dot: disconnected
│   ~/Code/piper · 1h ago       │
│   "Here's a draft schema..."  │
└─────────────────────────────┘
```

- Sort: most-recently-active first (`lastActivityAt` desc).
- Search filters client-side over `sessionName`, `cwd`, `lastPreview`.
- Tapping a disconnected entry shows a toast ("session not connected —
  reopen it in a terminal with `/rc`") rather than navigating.
- Empty state: "No sessions connected. Run `/rc` in a pi terminal to
  see it here." with a link to a short in-app help panel.
- Pull-to-refresh re-fetches `/api/sessions` (mostly a fallback; the
  control WS should make this unnecessary in normal operation).

### 9.2 Chat view

- Header: back button, session name (tap to rename via
  `set_session_name`), model badge (tap → model picker), `⋮` menu.
- `⋮` menu actions: `/compact`, `/new` (new session), `/resume` (via
  `switch_session`, with a session-file picker sourced from
  `SessionManager.list`-equivalent data if exposed, else omitted),
  `/fork`, session stats (`get_session_stats`), disconnect this view.
- Message list:
  - Markdown rendering + code block syntax highlighting for
    text content.
  - Collapsible tool-call cards: header = tool name + one-line args
    summary; expand to see full args/output. Live-updates from
    `tool_execution_update`.
  - Collapsible "thinking" blocks from `thinking_delta` content.
  - Streaming cursor while `message_update` is active.
  - Queue indicator (small chip: "1 queued message") from
    `queue_update`.
- Composer:
  - Send / Steer / Abort button, relabeling per streaming state
    (unchanged behavior from v1).
  - `/` triggers autocomplete populated from `get_commands` (fuzzy
    match on name/description).
  - Image attach (existing `ImageContent` support retained).
- Extension UI dialogs: `select`/`confirm`/`input`/`editor` render as
  native-feeling modal sheets; `notify`/`setStatus`/`setWidget` render
  as toasts / a small status chip / an inline banner respectively.

### 9.3 Navigation

`#/sessions` and `#/session/<id>` as History API states; Android/iOS
back gesture and browser back button pop the stack naturally instead of
exiting the installed PWA.

## 10. Backward compatibility: headless mode

The v1 behavior (Hub spawns `pi --mode rpc --project-dir <dir>` itself)
is kept as the `process_link.rs` `AgentLink` implementation, usable for
a project you want to drive purely from the phone with no terminal
open. Configured the same way as today (`--project-dir`, `--session`,
`--no-session`, `--pi-arg`, etc.), it self-registers into the same
`SessionRegistry` at Hub startup, so it shows up in the session list
like any `/rc`-connected session, just with `connected: true` for as
long as the Hub itself runs (Hub restart respawns it, matching v1's
`Restart=on-failure` story). Everything in [§8](#8-command-mapping-table)'s "RPC mode" column
applies unmodified to this link type, since it *is* literally
`pi --mode rpc` under the hood.

## 11. Milestones

1. **Registry refactor.** Introduce `AgentLink` trait; move existing
   `PiProcess` to `process_link.rs` implementing it; introduce
   `SessionRegistry` keyed by session id; existing spawn-mode
   self-registers as the sole entry at startup. No externally visible
   behavior change yet. Existing integration test
   (`tests/websocket_bridge.rs`) continues to pass unmodified (single
   session, `/ws` defaults to it).
2. **`/agent` transport.** Implement `ws_link.rs` + `server/agent.rs` +
   agent-token generation/loading. Prove the transport with a minimal
   throwaway script (not the real extension yet) that registers and
   round-trips one command/event.
3. **`piper-agent` extension, core commands.** Implement `/rc`
   connect/disconnect/status, event forwarding for the table in
   [§6.2](#62-relayed-rpc-protocol-unchanged-shape), and command dispatch for the non-gap rows of
   [§8](#8-command-mapping-table)'s core set (`prompt`, `steer`, `follow_up`, `abort`,
   `get_state`, `get_messages`, `get_commands`). Resolve the **v2 gap**
   items during this milestone by inspecting pi's actual TypeScript
   extension types (not just the docs) before deciding to drop or keep
   each one.
4. **Session list + control channel.** Implement `/api/sessions`,
   `/ws/control`, registry passive status tracking
   (`isStreaming`/`lastActivityAt`/`lastPreview`), and the PWA session
   list screen ([§9.1](#91-session-list-home-screen)). End-to-end test: two real terminals,
   two projects, both `/rc`-connected, phone lists and switches between
   them.
5. **Extended commands.** `set_model`/`cycle_model`,
   `set_thinking_level`, `compact`, `new_session`, `switch_session`,
   `fork`/`clone`, tree navigation, session rename.
6. **Chat view polish.** Markdown/code rendering, collapsible tool
   cards, thinking blocks, extension UI dialog rendering, `/`
   autocomplete.
7. **Hardening.** Reconnect/backoff verification (kill/restart Hub
   while sessions connected; kill/restart a terminal while phone is
   viewing it), stale-connection detection, updated README, packaging
   `piper-agent` for `pi install` (npm and/or git source).
8. **Stretch: Web Push notifications.** Only after 1–7 are solid.

## 12. Open questions / risks

- **`extension_ui_response` routing** ([§8](#8-command-mapping-table)): needs confirmation that
  pi's extension API exposes a way to resolve a dialog request that was
  raised via `ctx.ui` from *outside* the handler that raised it (i.e.
  from `piper-agent`'s own command-dispatch code, not from the dialog's
  caller). If not currently possible, this is a real feature gap
  versus RPC mode's built-in extension-UI sub-protocol, not just a
  missing convenience method.
- **`clear_queue`, `set_steering_mode`/`set_follow_up_mode`,
  `set_auto_compaction`/`set_auto_retry`/`abort_retry`, `export_html`**:
  need a spike against actual extension types to confirm equivalents
  exist; see milestone 3.
- **Concurrent input** (terminal + phone at the same instant): no
  locking; behaves like pi's normal steering/queueing rules. Consider a
  "last active client" indicator in a later iteration if this proves
  confusing in practice — not required for v2.
- **`sessionId` stability across `/new`/`/resume`**: handled via
  `meta_update` patching an existing registry entry rather than
  creating a new one; needs verification that `session_info_changed`
  fires reliably for all of `/new`, `/resume`, `/fork`, `/clone` inside
  the same process.
- **Node WebSocket client dependency** in `piper-agent`: confirm
  minimum Node version bundled with pi installs before deciding between
  a global `WebSocket` and an explicit `ws` npm dependency.
- **Hub URL discovery for `/rc`** on a machine other than the one
  running the Hub (e.g. two desktops, one Hub): v2 assumes Hub and
  `pi` run on the same machine (`127.0.0.1:4390` default); multi-machine
  setups need `PIPER_HUB_URL` pointed at the Hub's tailnet address
  instead, which then also needs the agent token distributed to that
  second machine — deferred design, not required for the primary use
  case in this spec.

## 13. Implementation notes (post-build corrections)

Milestones 1-6 have been implemented (Rust Hub, `piper-agent` extension,
PWA). Everything below is a place where the real build diverged from
this design doc, discovered by actually type-checking against pi's real
`.d.ts` files and running real end-to-end tests rather than assuming the
documented API surface — recorded here so the spec stays a trustworthy
reference rather than silently going stale.

- **§4 session identity**: implemented as designed (keyed by session id,
  not `cwd`) from the start; the `SessionMeta`/`SessionStatus` split and
  the `connected` watch channel match §5.1/§6.3 as specified.
- **§6.3 control channel message taxonomy**: simplified from five
  message types (`session_connected`, `session_status`,
  `session_preview`, `session_meta`, `session_disconnected`) down to
  four by merging `session_status`/`session_preview` into one
  `session_update` carrying the full current `SessionSummary` (not a
  diff). Simpler client code (one upsert-by-id handler covers
  `session_connected`/`session_update`/`session_meta`), at the cost of a
  few more bytes per update. `session_disconnected` still carries only
  `sessionId` (the session is gone, there's nothing else to send).
- **§6.1 command envelope**: implemented without response correlation on
  the Hub side, exactly per the "Implementation note" already called out
  in the original §6.1 text — confirmed as the right call once built:
  the Hub never needs the envelope `id` for anything, since responses
  are re-broadcast to every subscriber the same way a spawned child's
  stdout would be.
- **§7 wire format casing**: all `SessionMeta`/`SessionStatus`/
  `SessionSummary` JSON fields are `camelCase` on the wire (`sessionId`,
  `connectedAtMs`, `lastActivityAtMs`, etc.) via `#[serde(rename_all =
  "camelCase")]` — the original spec text used camelCase in examples but
  didn't call out the Rust attribute explicitly; a first draft without it
  serialized as `snake_case` and was only caught by the
  `tests/agent_bridge.rs` integration test, not by `cargo check`. Timestamps
  are plain milliseconds-since-epoch integers (`*Ms` suffix), not RFC3339
  strings, to avoid a datetime dependency on the Rust side — the PWA
  formats them client-side.
- **§8 command mapping, resolved gaps**: checked against pi's actual
  `.d.ts` files (not just `docs/extensions.md`/`docs/rpc.md`) before
  implementing. Confirmed genuinely unsupported at the extension-API
  level (not just "undocumented"): `clear_queue`,
  `set_steering_mode`/`set_follow_up_mode`, `set_auto_compaction`/
  `set_auto_retry`/`abort_retry`, `export_html`, `abort_bash`,
  `queue_update` as a forwardable event, and — significantly — proxying
  *other* extensions' `ctx.ui` dialogs to the phone (no hook exists for
  an extension to intercept another's dialog request). `bash` is
  implemented as a best-effort `pi.exec()` call rather than left
  unsupported, since it covers the common case even without producing a
  `BashExecutionMessage`. See `piper-agent/README.md` "Known gaps" for
  the full list kept in sync with the actual code.
- **`ImageContent` shape**: turned out to be flat (`{type:"image", data,
  mimeType}`, identical to RPC's own wire shape) rather than the nested
  `{type:"image", source:{type:"base64", mediaType, data}}` shown in one
  `pi.sendUserMessage` doc example — caught by `tsc`, not by reading
  docs. `piper-agent`'s `prompt` handler passes RPC image content straight
  through with no conversion as a result.
- **`session_info_changed` is narrower than assumed**: it only fires for
  a display-name change, not for full session replacement (`/new`,
  `/fork`, `/resume` tear down and recreate the whole extension runtime
  and fire `session_shutdown`/`session_start` instead, per pi's actual
  `SessionInfoChangedEvent`/`SessionShutdownEvent` types). The original
  §6.1 design of "send `meta_update` instead of disconnecting" only
  applies to renames now. Full session replacement is instead handled by
  reconnecting from inside the `withSession` callback of `ctx.newSession`/
  `ctx.fork`/`ctx.switchSession` — see `piper-agent/README.md` "Following
  across `/new`, `/fork`, `/resume`" — which is a better outcome than the
  original design anyway (seamless follow rather than a bare disconnect).
- **`message_update` reshaping**: pi's extension-level
  `MessageUpdateEvent` carries a cumulative `message` field and no
  top-level `usage`; RPC's wire event is the opposite (`usage`, no
  `message`). `piper-agent` reshapes one into the other so the PWA can
  stay written against the documented RPC shape unmodified.
- **`compaction_start`/`compaction_end` remapping**: pi's interactive/
  extension-level events for this are `session_before_compact`/
  `session_compact`/`session_compact_failed`, differently named and
  shaped from RPC mode's `compaction_start`/`compaction_end`.
  `piper-agent` remaps them (§ "Events" in `event-bridge.ts`); fidelity is
  good (`summary`, `firstKeptEntryId`, `tokensBefore`, `usage`, `details`,
  `willRetry`, `aborted` all present) but `agent_end.willRetry` is always
  reported `false` since the extension-level `AgentEndEvent` doesn't carry
  it.
- **Validated end-to-end**, not just unit/type-checked: a real
  `pi --mode rpc` process with the real `piper-agent` extension loaded
  (via a project-local `.pi/settings.json`, not `-e`, since `-e` installs
  to a temporary directory that doesn't preserve `piper-agent`'s own
  `node_modules/ws`), registering against a real Hub binary, driven by a
  plain `ws` client standing in for the phone: `/rc`, `get_state`,
  `get_session_name`, `get_commands`, `get_available_models`,
  `get_messages`, and `set_session_name` (confirming the `meta_update`
  in-place rename path) were all exercised successfully without any LLM
  calls. `cargo test` covers the Hub-only paths (registration, relay,
  disconnect notification, ambiguous-session rejection) as permanent
  regression tests; the full extension round-trip was a one-time manual
  verification during this build and is not (yet) part of an automated
  CI job — see `piper-agent/README.md`'s Development section for how to
  repeat it by hand.
