# piper-agent

A pi extension that adds `/rc` to connect your **current interactive
`pi` session** to a [piper](../README.md) Hub, so it can be controlled
remotely (e.g. from your phone) while you keep using the real terminal
UI normally. See [`SPEC.md`](../SPEC.md) for the full architecture —
this file covers setup and the practical details/limitations of this
extension specifically.

Unlike piper v1 (which spawned a second, separate `pi --mode rpc`
process), this extension makes your terminal session itself the thing
being controlled: the phone and the terminal are two clients of the
same live session.

## Setup

1. Make sure a piper Hub is running somewhere reachable from this
   machine (usually on the same desktop — see the main
   [`README.md`](../README.md)). The Hub generates an agent-token file
   on first run at `~/.pi/agent/piper/agent-token`.
2. Install this extension globally so it's available in every project:
   ```bash
   pi install /absolute/path/to/piper-agent   # local checkout
   # or, once published:
   pi install npm:piper-agent
   pi install git:you/piper-agent
   ```
   `npm install` runs automatically as part of `pi install`, resolving
   the `ws` dependency.
3. In any `pi` session (interactive or headless), run:
   ```
   /rc
   ```
   This connects the session to the Hub. The pi footer shows `RC: connected`,
   `RC: connecting`, or `RC: reconnecting` while the bridge is active.
   `/rc status` reports the current connection state; `/rc stop` disconnects.

### Configuration (environment variables)

| Variable | Default | Purpose |
|---|---|---|
| `PIPER_HUB_URL` | `ws://127.0.0.1:4390/agent` | Where the Hub's `/agent` endpoint is. Override if the Hub runs on another host. |
| `PIPER_AGENT_TOKEN` | *(reads from file)* | The agent token. Normally left unset so it's read from disk instead. |
| `PIPER_AGENT_TOKEN_PATH` | `~/.pi/agent/piper/agent-token` | Where to read the agent token from, if `PIPER_AGENT_TOKEN` isn't set. |
| `PIPER_OWNER` | *(auto-detected)* | Tailnet login recorded as this session's owner, e.g. `alice@github`. Only the owner sees the session from the phone (see SPEC.md §7.2). By default it's auto-detected when running over Tailscale SSH via `tailscale whois` on the client IP from `$SSH_CLIENT`; without it, the session registers unowned and is visible to every tailnet user. |

You should not normally need to set any of these on a single-desktop
setup — the defaults match the Hub's own defaults exactly.

## Following across `/new`, `/fork`, `/resume`

`/new`, `/fork`, and `switch_session` (`/resume`) tear down and recreate
pi's entire extension runtime for the replacement session (this is how
pi itself works, not specific to this extension). Rather than just
disconnecting, this extension reconnects automatically under the new
session, so a phone that was watching a session before the switch keeps
watching (now pointed at a "session disconnected" -> "new session
connected" transition) without you having to type `/rc` again. This
works both ways:

- **Remote switch** (phone sends `new_session` / `fork` /
  `switch_session`, or a prompt of exactly `/new`): the extension
  intercepts the command, creates the replacement session itself, and
  re-runs `/rc` through the fresh replacement context.
- **Local switch** (you type `/new`, `/resume`, or `/fork` in the
  terminal, or `/reload`): pi's built-in command tears the extension
  runtime down with no chance to run code in the replacement, so the
  outgoing session instead records a small handoff entry on disk
  (`~/.pi/agent/piper/rc-handoff.json`) and the replacement instance's
  `session_start` hook picks it up and reconnects. The entry is
  only removed by `/rc stop` or quitting pi, so RC also follows a
  session across a pi restart + `/resume` of the same session file.

A plain rename (`/name`) is even lighter-weight: it patches the existing
registry entry in place and never disconnects at all.

## Known gaps

This extension maps pi RPC commands (`SPEC.md` §8) onto pi's
**extension API**, not onto RPC mode itself, so a few things don't have
a confirmed equivalent yet and fail cleanly with a clear error instead
of silently no-op-ing:

- `clear_queue`, `set_steering_mode`, `set_follow_up_mode`,
  `set_auto_compaction`, `set_auto_retry`, `abort_retry`, `export_html`,
  `abort_bash` — no extension-API equivalent found as of this pi
  version.
- `bash` runs via `pi.exec()` and returns output/exit code, but —
  unlike RPC mode's native `bash` command — does **not** add a
  `BashExecutionMessage` into session context (no extension API for
  that), and can't be cancelled (`abort_bash` is unsupported).
- `queue_update` is never forwarded as an event: there is no
  `pi.on("queue_update", ...)` hook for extensions. The phone won't see
  a "queued messages" indicator for a `piper-agent`-linked session.
- `extension_ui_request`/`extension_ui_response` (the dialogs *other*
  extensions in your project raise via `ctx.ui.select/confirm/input`)
  are **not** proxied to the phone. There is no extension-level hook to
  intercept another extension's `ctx.ui` call; it's answered locally at
  the terminal exactly as it would be without this extension installed.
  If nobody is at the terminal and a dialog has no `timeout`, it will
  stall waiting for input the phone never sends — same caveat as piper
  v1's own README.
- `auto_retry_start`/`auto_retry_end`,
  `summarization_retry_scheduled`/`_attempt_start`/`_finished`, and
  `extension_error` are also not forwarded (no matching extension
  hooks).
- `get_state.isCompacting`, `steeringMode`, `followUpMode`,
  `autoCompactionEnabled`, and `pendingMessageCount` are approximated
  (fixed defaults or a 0/1 flag) rather than genuinely observed, since
  the extension API doesn't expose the real values.
- `get_session_stats` and `get_messages` only cover the **active
  branch** (`ctx.sessionManager.buildContextEntries()`), not the full
  session including abandoned branches / pre-compaction history the way
  RPC mode's equivalents do.
- `fork`'s response omits the `text` field (the original prompt text)
  that RPC mode's `fork` command returns — `ctx.fork()` doesn't provide
  it.
- `compact`'s response omits `estimatedTokensAfter` in some cases,
  depending on what the underlying `CompactionResult` includes.

None of these affect the core flow (prompting, steering, aborting,
streaming responses, tool call visibility, model/thinking-level
switching, session stats, session navigation) — they're edge cases
around less commonly used RPC commands.

## Development

```bash
cd piper-agent
npm install
npx tsc --noEmit    # type-check against the real @earendil-works/pi-coding-agent types
```

There's no build step at runtime — pi loads `.ts` files directly via
`jiti`. `npm install`/`tsc` here are purely for local type-checking
during development.
