# Piper

Piper is a small Rust bridge that lets you remotely control a
[pi](https://pi.dev) coding-agent session from your phone, over a private
[Tailscale](https://tailscale.com) network. No public hosting, no app store
deployment — just a webpage served privately on your tailnet.

## Architecture

```
Phone (browser / PWA) --wss--> Piper (Rust: axum + WebSocket) --stdio JSONL--> `pi --mode rpc` (child process)
                                          ^
                               reachable only via Tailscale (tailscale serve)
```

Piper spawns `pi --mode rpc` as a child process and speaks pi's documented
RPC protocol (see pi's `docs/rpc.md`) over the child's stdin/stdout
(newline-delimited JSON). It relays that same JSON protocol to one or more
connected browser WebSocket clients, so the browser talks the RPC protocol
directly — no separate protocol to design or keep in sync.

Piper binds only to `127.0.0.1` by default; `tailscale serve` is
responsible for exposing it (with a valid HTTPS certificate) to your
tailnet only. Nothing is exposed to the public internet.

## Status

This project is built incrementally. Checklist:

- [x] JSONL line-protocol relay primitives
- [x] `pi` child process management
- [x] HTTP + WebSocket bridge server
- [x] Mobile-friendly PWA client
- [x] systemd unit + Tailscale deployment instructions

## Configuration and usage

```bash
cargo run -- \
  --project-dir /path/to/project \
  --token "$(openssl rand -hex 32)" \
  --bind 127.0.0.1:4390
```

All flags also accept an environment variable (see `--help`), which is
what the systemd deployment uses so the token never appears in `ps`
output: `PIPER_PROJECT_DIR`, `PIPER_TOKEN`, `PIPER_BIND`, `PIPER_SESSION`,
`PIPER_NO_SESSION`, `PIPER_PI_COMMAND`.

Piper binds to `127.0.0.1` by default. Point `tailscale serve` at that
port to expose it to your tailnet with a valid HTTPS certificate (see
[Deployment](#deployment) below).

Once running, a client opens `wss://<host>/ws?token=<token>` and speaks
pi's RPC protocol directly (see pi's `docs/rpc.md` for the full command
and event reference) — Piper does not wrap or translate it.

**Protocol design note:** Piper deliberately relays the RPC JSON verbatim
in both directions instead of defining its own browser-facing protocol.
This means the browser client and pi's RPC docs are the only protocol
reference needed; Piper has nothing of its own to keep in sync.

**Crash handling note:** Piper does not implement its own restart/backoff
for a crashed `pi` process. If `pi` exits, Piper exits too, and the
systemd unit (added later) restarts Piper via `Restart=on-failure`. This
avoids reimplementing a process supervisor the OS already provides.

**Auth note:** the `?token=` check is defense-in-depth, not the primary
security boundary — Piper is designed to be reachable only over a
private Tailscale network in the first place.

### Mobile client

Open `https://<host>/` (or `http://` while testing over loopback) in a
phone browser. On first load it asks for the access token and stores it
in `localStorage`; you can also open a link like
`https://<host>/?token=<token>` to set it automatically, which is handy
for a one-time bookmark/home-screen setup. "Add to Home Screen" installs
it as a standalone PWA using `manifest.json` and `sw.js` (the service
worker only caches the static app shell — chat data always goes over the
live WebSocket, there is no offline chat mode).

The composer has two buttons: **Send** submits a prompt when idle, or a
*steering* message (delivered after the current tool calls finish) while
the agent is already streaming — the button relabels itself accordingly.
**Abort** appears only while streaming and cancels the current run.

**Known limitation:** extension UI dialogs (`ctx.ui.select/confirm/input`
from pi extensions) are not yet rendered by this client. If a project's
extensions rely on those without a timeout, they will stall waiting for a
response this client never sends. Piper's own defaults don't use them.

## Deployment

This deploys Piper as a systemd service on the machine that already runs
`pi` (a desktop, home server, NAS, etc.), reachable from your phone only
over Tailscale.

### 1. Build and install the binary

```bash
cargo build --release
sudo install -m 755 target/release/piper /usr/local/bin/piper
```

### 2. Configure

```bash
sudo install -d /etc/piper
sudo install -m 600 deploy/piper.env.example /etc/piper/piper.env
sudo $EDITOR /etc/piper/piper.env   # set PIPER_PROJECT_DIR and PIPER_TOKEN
```

### 3. Install the systemd unit

```bash
sudo cp deploy/piper.service /etc/systemd/system/piper.service
sudo $EDITOR /etc/systemd/system/piper.service   # set User=
sudo systemctl daemon-reload
sudo systemctl enable --now piper
sudo systemctl status piper
```

`Restart=on-failure` means systemd restarts Piper (and thus respawns
`pi`) if the `pi` child process ever crashes — see the crash-handling note
above for why that logic lives in systemd rather than in Piper itself.

### 4. Expose it on your tailnet with `tailscale serve`

Install [Tailscale](https://tailscale.com/download) on this machine and
sign in (`tailscale up`), then:

```bash
tailscale serve --bg 4390
```

This proxies your tailnet's HTTPS address (with a certificate Tailscale
manages automatically) to Piper's loopback port. It persists across
reboots as part of `tailscaled`'s own state — no separate service to
manage. Useful commands:

```bash
tailscale serve status   # see the current mapping
tailscale serve reset    # remove it
```

Exact flags can shift between Tailscale versions; run `tailscale serve
--help` if the above doesn't match your installed version. Do **not** use
`tailscale funnel`, which exposes the service to the public internet
instead of just your tailnet.

### 5. Connect from your phone

1. Install the Tailscale app on your phone and sign in to the same
   tailnet.
2. Find this machine's tailnet hostname: `tailscale status` (looks like
   `your-machine.your-tailnet.ts.net`).
3. Open `https://your-machine.your-tailnet.ts.net/?token=<PIPER_TOKEN>`
   in your phone's browser once, to store the token.
4. "Add to Home Screen" to install it as a standalone app icon.

Nothing here is reachable from the public internet — only devices signed
into your tailnet can resolve or reach that hostname at all.

## Development

```bash
cargo fmt
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
```

The integration test in `tests/websocket_bridge.rs` spawns a real `pi
--mode rpc` process and drives it over a real WebSocket using
`get_state`, which never calls the configured LLM, so it runs without
network access or API costs (beyond whatever `pi` itself needs to start
up).

## License

MIT
