# Architecture

This is a code-oriented tour of how Pi Piper works — including the wire
protocol, authorization model, and the places where the implementation
deliberately diverged from the original design. For the PWA's visual
design system, see [`DESIGN.md`](DESIGN.md).

## The one-paragraph version

Pi Piper is a relay. A `pi` coding-agent session and a phone browser are
two clients of the same live session; Pi Piper sits between them and
forwards pi's own RPC protocol (JSON lines over WebSocket, see pi's
`docs/rpc.md`) almost verbatim in both directions, plus a small
transport-only envelope for multi-session routing. It invents no
application protocol of its own, keeps no durable state, and serves
everything from a single Rust binary over a private Tailscale network.

## System diagram

```
Desktop terminal (any project)         Desktop terminal (another project)
┌─────────────────────────────┐        ┌─────────────────────────────┐
│ pi (interactive TUI)         │        │ pi (interactive TUI)         │
│ + pi-piper-agent extension      │        │ + pi-piper-agent extension      │
│ /rc  ── connects out ────────┼──┐     │ /rc  ── connects out ────────┼──┐
└─────────────────────────────┘  │     └─────────────────────────────┘  │
                                  ▼                                     ▼
                     ┌──────────────────────────────────────────────────┐
                     │                Pi Piper Hub (this repo, Rust)        │
                     │  SessionRegistry keyed by session id              │
                     │  /agent  /ws  /ws/control  /api/sessions          │
                     │  + embedded PWA (assets/, rust-embed)             │
                     └───────────────────────┬────────────────────────┘
                                             │ wss (tailscale serve)
                                             ▼
                                   Phone (PWA): session list + chat
```

The Hub binds `127.0.0.1:4390` by default and is exposed to the owner's
tailnet with `tailscale serve`; nothing is reachable from the public
internet.

## Components

### Pi Piper Hub (`src/`, Rust)

| Module | Role |
|---|---|
| `main.rs` | Thin binary: parse config, load/create the agent token, optionally spawn the headless fallback, run the server until Ctrl+C. |
| `config.rs` | CLI + env configuration (clap with `env` attrs), so secrets can live in a systemd `EnvironmentFile` instead of `ps` output. |
| `server/` | The axum router: `/agent`, `/ws`, `/ws/control`, `/api/sessions`, and the embedded PWA fallback. `auth.rs` holds the two authorization checks (see below). |
| `registry.rs` | The switchboard. A `HashMap<session id, Arc<SessionHandle>>` plus a broadcast channel of registry-change events for the phone's session list. Also hosts one passive `watch_status` task per session that derives streaming/preview state by peeking at events already flowing to subscribers. |
| `session.rs` | Session identity and the `AgentLink` abstraction (see below). |
| `rpc/` | JSONL framing helpers and the two agent-link implementations: `process.rs` (spawned `pi --mode rpc`) and `remote_agent.rs` (a `pi-piper-agent` WebSocket). |
| `agent_token.rs` | Generates and persists the `/agent` shared secret on first run (`~/.pi/agent/pi-piper/agent-token`, mode 0600). |
| `state.rs` | `AppState`: registry + agent token handed to all handlers. |

### `pi-piper-agent` (`pi-piper-agent/`, TypeScript pi extension)

Installed once into pi, available in every project. `/rc` dials out to
the Hub's `/agent` endpoint over a WebSocket, registers the current
live session, then translates between pi's extension API and the
relayed RPC protocol: it subscribes to pi's event hooks and forwards
them to the Hub, and maps incoming RPC commands onto extension-API
calls (`ctx.prompt`, `ctx.steer`, `ctx.abort`, session management,
etc.). It also reconnects automatically across `/new`, `/fork`, and
`/resume`, which tear down pi's extension runtime. See
[`pi-piper-agent/README.md`](pi-piper-agent/README.md) for the exact mapping
and its known gaps.

### Mobile PWA (`assets/`, embedded in the binary)

Vanilla JS/CSS, no build step, compiled into the Hub via `rust-embed`.
Two screens: a live session list (fed by `/ws/control` +
`/api/sessions`) and a chat view per session (fed by `/ws`), which
renders pi's RPC event stream and sends prompts, steering, and aborts.

## The key abstraction: `AgentLink`

A "session" is one running `pi` process. The Hub reaches one two ways,
behind a closed two-variant enum (`session.rs`):

- **`AgentLink::Remote`** — the primary path. A `pi-piper-agent` extension
  inside an already-running interactive `pi` dialed out to `/agent`.
  The Hub never spawned anything; it's attached to a session the user
  started themselves.
- **`AgentLink::Headless`** — the v1-compatible fallback (`--project-dir`).
  The Hub itself spawned `pi --mode rpc` for a project with no terminal
  open.

Both expose the same two operations — `subscribe()` for the event
broadcast, `send()` for commands — so the registry, the phone bridges,
and the API handlers are identical for both. Everything else about a
session (`SessionMeta`, `SessionStatus`, a `connected` watch channel)
lives in the `SessionHandle` the registry holds.

## Data flow

**Events (session → phone).** An agent link broadcasts every event it
receives (extension event forward, or child stdout line) on that
session's `tokio::sync::broadcast` channel. Each phone connected to
`/ws?session=<id>` gets its own task relaying from that channel to its
socket. The registry's per-session `watch_status` task subscribes to
the same channel but only inspects `agent_start`, `agent_settled`, and
`message_end` to derive the session list's streaming indicator and
message preview — keeping per-token traffic off the control channel.

**Commands (phone → session).** `/ws` reads JSON messages from the
phone and calls `AgentLink::send()`: stdin for a headless child, the
agent WebSocket for a remote session. There is no response
correlation at the Hub: responses and events are re-broadcast the same
way, and the phone matches them up, exactly as it would against a raw
`pi --mode rpc` process.

**Registry changes.** Connects, disconnects, metadata renames, and
derived status changes are broadcast on one shared control channel.
`/ws/control` streams it to every open phone; `GET /api/sessions`
returns a one-shot snapshot of the same summaries, so the session list
can render immediately and then stay current.

## Wire protocol

All WebSocket messages are single JSON objects per text frame. The
phone-facing `/ws` connection carries pi's own RPC protocol (see pi's
`docs/rpc.md`) byte-for-byte, unchanged — Pi Piper defines only the two
thin layers around it:

**Agent registration (`/agent`, Hub ↔ `pi-piper-agent`).** Connect with
`/agent?token=<agent_token>` (rejected with `401` otherwise), then:

```jsonc
// extension → Hub, on connect:
{"type": "register", "sessionId": "abc123", "sessionFile": ".../abc123.jsonl",
 "sessionName": "Refactor auth module", "cwd": "/home/user/Code/pi-piper"}
// Hub → extension:
{"type": "registered", "sessionId": "abc123"}
// extension → Hub, on a session rename (in-place registry patch):
{"type": "meta_update", "sessionId": "abc123", "patch": {"sessionName": "..."}}
// extension → Hub, every forwarded pi event (verbatim RPC event JSON):
{"type": "event", "sessionId": "abc123", "event": { ... }}
// Hub → extension, an RPC command; extension → Hub, its response:
{"type": "command", "id": "req-1", "command": {"type": "prompt", "message": "..."}}
{"type": "response", "id": "req-1", "response": { ... }}
```

`sessionId` is technically redundant (one connection = one session) but
kept so messages are self-describing when debugging or replaying. The
Hub does no response correlation — responses are re-broadcast like any
event, and the phone matches them up.

**Control channel (`/ws/control`, Hub → phone).** An initial
`{"type": "sessions_snapshot", "sessions": [...]}` on connect, then
incremental updates: `session_connected`, `session_update`, and
`session_meta` each carry the full current session summary (upsert by
`sessionId`); `session_disconnected` carries only `sessionId`.
Summaries are `camelCase` with millisecond-since-epoch integer
timestamps (`connectedAtMs`, `lastActivityAtMs`, …), formatted
client-side by the PWA.

**Phone session bridge (`/ws?session=<id>`).** After upgrade, exactly
pi's RPC JSON in both directions. Omitting `?session=` is allowed only
when exactly one session is registered (keeps the single-session case
simple); otherwise the request is rejected with `400` rather than
guessing. `GET /api/sessions` returns the same summary array as the
control channel's snapshot, for first render and WS-less fallbacks.

**Command set.** The commands the phone can send are pi's RPC
commands, mapped by `pi-piper-agent` onto pi's extension API (see the
table in `pi-piper-agent/extensions/command-dispatch.ts`). A few have no
extension-API equivalent and fail with a clear error instead — the
authoritative list is "Known gaps" in
[`pi-piper-agent/README.md`](pi-piper-agent/README.md).

## Authorization model

Two independent checks, both in `server/auth.rs` (details and rationale
in the paragraphs below):

- **Phone-facing routes** (`/ws`, `/ws/control`, `/api/sessions`):
  authorized by the `Tailscale-User-Login` identity header that
  `tailscale serve` stamps on everything it proxies in — any device
  signed into the tailnet is authorized, and Funnel-flagged requests
  are rejected outright. There is deliberately no second secret here.
- **`/agent`** (extension registration): a shared secret
  (`?token=…`), because it's reached directly over loopback by a local
  process that is never proxied through `tailscale serve`, so there's
  no identity header to trust. On first run the Hub generates the
  token and persists it where `pi-piper-agent` reads it automatically.

The honest caveat, documented in the code too: the Hub cannot
cryptographically distinguish a real `tailscale serve` proxy from any
other local process setting the same header. The security boundary is
the tailnet (plus trusting the Hub machine's local users), not this
header — which is also why none of this is a substitute for Tailscale
ACLs, and why `tailscale funnel` must never sit in front of Pi Piper
(funnel-flagged requests are rejected outright regardless of login).

## Implementation notes

Places where the build deliberately diverged from the original design
doc, kept here so the code's shape doesn't look unmotivated:

- The control channel was simplified from five message types to four:
  status and preview updates merged into one `session_update` carrying
  the full summary, so the client has a single upsert-by-id handler.
- `session_info_changed` (renames) patches the registry entry in place
  via `meta_update`; full session replacement (`/new`, `/fork`,
  `/resume`) tears down pi's extension runtime entirely, so
  `pi-piper-agent` instead reconnects from inside the replacement — and
  records a handoff on disk so the reconnect survives a pi restart.
- pi's extension-level events differ from RPC-mode wire events in a
  few places (`message_update` carries a cumulative `message` instead
  of `usage`; compaction events have different names/shapes).
  `pi-piper-agent` reshapes them so the PWA can be written against the
  documented RPC shape unmodified.
- `get_session_stats`/`get_messages` only cover the active branch of a
  session, and a few RPC commands have no extension-API equivalent at
  all — see "Known gaps" in `pi-piper-agent/README.md` for the full list.

## Repository layout

```
src/                 Rust Hub (see table above)
assets/              Mobile PWA, embedded in the binary
pi-piper-agent/         TypeScript pi extension (pi package: adds /rc)
tests/               Integration tests (drive the real router + a real pi process)
deploy/              systemd unit + environment file example
DESIGN.md            PWA design tokens (used with the impeccable design system)
PRODUCT.md           Product framing
```

## Testing

`cargo test` runs 22 tests: unit tests inline in each module, plus two
integration suites in `tests/` that drive the real axum router.
`tests/agent_bridge.rs` exercises the `/agent` ↔ `/ws` ↔ `/ws/control`
↔ `/api/sessions` machinery with a plain WebSocket client standing in
for `pi-piper-agent` (the wire protocol is small and transport-only, so
that's enough). `tests/websocket_bridge.rs` spawns a real
`pi --mode rpc` process for the headless-link path. Neither suite
calls the configured LLM, so everything runs without network access or
API cost.
