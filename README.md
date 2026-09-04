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
- [ ] Mobile-friendly PWA client
- [ ] systemd unit + Tailscale deployment instructions

## Building

```bash
cargo build --release
```

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
port to expose it to your tailnet with a valid HTTPS certificate (see the
deployment docs, added in a later commit).

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

## License

MIT
