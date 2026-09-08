# Piper

Piper is a small Rust hub that lets you control your **existing,
already-running, interactive `pi`** coding-agent sessions remotely from
your phone, over a private [Tailscale](https://tailscale.com) network —
and keep using the real terminal at the same time, fully synced. No
public hosting, no app store deployment: a webpage served privately on
your tailnet.

See [`SPEC.md`](SPEC.md) for the full architecture and design rationale.
This file is setup-oriented.

## Status

Implemented: the Hub's session registry, `/agent` + `/ws` + `/ws/control`
+ `/api/sessions`, the `piper-agent` extension (`/rc`, event forwarding,
command dispatch per `SPEC.md` §8), and the PWA's session list + chat
view. See `SPEC.md` §13 for exactly what was validated end-to-end versus
covered only by automated tests, and `piper-agent/README.md` "Known
gaps" for the handful of RPC commands without a confirmed extension-API
equivalent yet. Not implemented: Web Push notifications (`SPEC.md`
milestone 8, stretch).

## How it fits together

```
Desktop terminal (any project)         Desktop terminal (another project)
┌─────────────────────────────┐        ┌─────────────────────────────┐
│ pi (interactive TUI)         │        │ pi (interactive TUI)         │
│ + piper-agent extension      │        │ + piper-agent extension      │
│ /rc  ── connects out ────────┼──┐     │ /rc  ── connects out ────────┼──┐
└─────────────────────────────┘  │     └─────────────────────────────┘  │
                                  ▼                                     ▼
                     ┌──────────────────────────────────────────────────┐
                     │                Piper Hub (this repo)              │
                     │  session registry, keyed by session id            │
                     │  /agent  /ws  /ws/control  /api/sessions          │
                     └───────────────────────┬────────────────────────┘
                                             │ wss (tailscale serve)
                                             ▼
                                   Phone (PWA), session list + chat
```

Type `/rc` once in any interactive `pi` session (any project, any
number of them at once) to connect it to the Hub. Your phone gets a
Claude-style list of every currently connected session and can open any
of them — the phone and the terminal are two clients of the **same live
session**, not two separate conversations: whatever you type in one
shows up in the other, live.

Piper relays pi's own documented RPC protocol (see pi's `docs/rpc.md`)
almost verbatim between the phone and each session; there's no separate
protocol invented to keep in sync. See [`SPEC.md`](SPEC.md) §6 for the
exact wire shapes.

## Components

- **This repo (Rust)**: the Hub. Binds `127.0.0.1:4390` by default;
  expose it to your tailnet with `tailscale serve` (see below). Also
  serves the mobile PWA.
- **[`piper-agent/`](piper-agent/README.md)** (TypeScript pi extension):
  install this once, globally, so `/rc` is available in every project.
  See its README for setup and known limitations.

## Quickstart

```bash
cargo build --release
./target/release/piper --token "$(openssl rand -hex 32)"
```

This starts the Hub with no project pre-configured — it's a pure
switchboard, waiting for `piper-agent` connections. On first run it also
generates an **agent token** at `~/.pi/agent/piper/agent-token` (mode
`0600`); `piper-agent` reads this automatically, you never type or copy
it anywhere.

Install `piper-agent` once (see its README), then in any `pi` session:

```
/rc
```

Open `https://<host>/` on your phone (see [Deployment](#deployment) for
exposing it over Tailscale) — you'll see that session in the list.

### Configuration

All flags also accept an environment variable (see `--help`), which is
what the systemd deployment uses so secrets never appear in `ps` output:

| Flag | Env | Purpose |
|---|---|---|
| `--token` | `PIPER_TOKEN` | Phone-facing shared secret (`/ws`, `/ws/control`, `/api/sessions`). |
| `--bind` | `PIPER_BIND` | Address to listen on. Defaults to `127.0.0.1:4390`. |
| `--agent-token` | `PIPER_AGENT_TOKEN` | Pin the agent token instead of auto-generating one. Usually left unset. |
| `--agent-token-path` | `PIPER_AGENT_TOKEN_PATH` | Where to persist a generated agent token. Defaults to `~/.pi/agent/piper/agent-token`. |
| `--project-dir` | `PIPER_PROJECT_DIR` | Optional: also spawn a headless `pi --mode rpc` for one project with no terminal open (v1-compatible fallback, see `SPEC.md` §10). Most setups don't need this. |
| `--session`, `--no-session`, `--pi-arg` | `PIPER_SESSION`, `PIPER_NO_SESSION` | Only relevant together with `--project-dir`. |

**Auth note:** there are *two* independent tokens — the phone token
above, and a separate agent token that gates `/agent` (where
`piper-agent` extensions register sessions). They're deliberately
different secrets: `tailscale serve` proxies phone connections through
loopback, so a peer-address check alone can't tell a genuine local
`piper-agent` connection apart from a proxied phone request. Both are
defense-in-depth, not the primary security boundary — Piper is designed
to be reachable only over a private Tailscale network in the first
place.

### Mobile client

Open `https://<host>/` (or `http://` while testing over loopback) in a
phone browser. On first load it asks for the (phone) access token and
stores it in `localStorage`; you can also open a link like
`https://<host>/?token=<token>` to set it automatically. "Add to Home
Screen" installs it as a standalone PWA.

- **Session list**: every connected session, live-updated, with a
  streaming indicator and a preview of the last message — search to
  filter. Tap one to open it.
- **Chat view**: the composer's Send button relabels to **Steer** while
  the agent is already streaming; **Abort** appears only while
  streaming. `/` in the composer triggers slash-command autocomplete
  (sourced from `get_commands`). Tool calls render as collapsible cards.
  Extension `select`/`confirm`/`input` dialogs render via the browser's
  native prompt/confirm dialogs (functional, not fancy — see
  `piper-agent`'s README for the nicer-UI backlog item).

**Known limitation:** dialogs raised by *other* project extensions
(via `ctx.ui.select/confirm/input`) are answered at the terminal, not
proxied to the phone — see `piper-agent/README.md` "Known gaps" for why
and what else doesn't have full parity yet when a session is connected
via `/rc` rather than the optional headless fallback.

## Deployment

Reachable from your phone only over Tailscale.

### 1. Build and install the binary

```bash
cargo build --release
sudo install -m 755 target/release/piper /usr/local/bin/piper
```

### 2. Configure

```bash
sudo install -d /etc/piper
sudo install -m 600 deploy/piper.env.example /etc/piper/piper.env
sudo $EDITOR /etc/piper/piper.env   # set PIPER_TOKEN at minimum
```

### 3. Install the systemd unit

```bash
sudo cp deploy/piper.service /etc/systemd/system/piper.service
sudo $EDITOR /etc/systemd/system/piper.service   # set User=
sudo systemctl daemon-reload
sudo systemctl enable --now piper
sudo systemctl status piper
```

The `User=` you pick is also the user whose `pi` sessions can `/rc` into
this Hub (it's the user whose `~/.pi/agent/piper/agent-token` gets
generated and read).

### 4. Install `piper-agent`

See [`piper-agent/README.md`](piper-agent/README.md). Do this as the
same user configured above.

### 5. Expose it on your tailnet with `tailscale serve`

Install [Tailscale](https://tailscale.com/download) on this machine and
sign in (`tailscale up`), then:

```bash
tailscale serve --bg 4390
```

This proxies your tailnet's HTTPS address (with a certificate Tailscale
manages automatically) to Piper's loopback port. Useful commands:

```bash
tailscale serve status   # see the current mapping
tailscale serve reset    # remove it
```

Do **not** use `tailscale funnel`, which exposes the service to the
public internet instead of just your tailnet.

### 6. Connect from your phone

1. Install the Tailscale app on your phone and sign in to the same
   tailnet.
2. Find this machine's tailnet hostname: `tailscale status` (looks like
   `your-machine.your-tailnet.ts.net`).
3. Open `https://your-machine.your-tailnet.ts.net/?token=<PIPER_TOKEN>`
   in your phone's browser once, to store the token.
4. "Add to Home Screen" to install it as a standalone app icon.
5. In any `pi` terminal session (on the Hub's machine), run `/rc`. It
   appears in the phone's session list within a second or two.

Nothing here is reachable from the public internet — only devices signed
into your tailnet can resolve or reach that hostname at all.

## Development

```bash
cargo fmt
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
```

Integration tests spawn a real `pi --mode rpc` process (for the
headless-link tests, `tests/websocket_bridge.rs`) and drive the full
axum router over a real WebSocket. `tests/agent_bridge.rs` covers the
`/agent` <-> `/ws` <-> `/ws/control` <-> `/api/sessions` machinery with a
plain WebSocket client standing in for `piper-agent` (the wire protocol
is deliberately small and transport-only, see `SPEC.md` §6.1, so this
doesn't need a real extension to exercise). Neither test suite calls the
configured LLM, so both run without network access or API cost beyond
whatever `pi` itself needs to start up.

For the extension side, see
[`piper-agent/README.md`](piper-agent/README.md#development).

## License

MIT
