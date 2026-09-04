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
- [ ] HTTP + WebSocket bridge server
- [ ] Mobile-friendly PWA client
- [ ] systemd unit + Tailscale deployment instructions

## Building

```bash
cargo build --release
```

## Configuration and usage

Full usage lands with the WebSocket server (see checklist above). For now,
`piper` is a manual smoke test for the `pi` process bridge: it spawns
`pi --mode rpc` in `--project-dir`, sends one prompt, prints the streamed
reply, and exits.

```bash
cargo run -- --project-dir /path/to/project --no-session "List the files here"
```

Crash handling note: Piper does not implement its own restart/backoff for
a crashed `pi` process. If `pi` exits, Piper exits too, and the systemd
unit (added later) restarts Piper via `Restart=on-failure`. This avoids
reimplementing a process supervisor that the OS already provides.

## License

MIT
