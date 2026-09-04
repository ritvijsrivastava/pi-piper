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

- [ ] JSONL line-protocol relay primitives
- [ ] `pi` child process management (spawn, restart, backoff)
- [ ] HTTP + WebSocket bridge server
- [ ] Mobile-friendly PWA client
- [ ] systemd unit + Tailscale deployment instructions

## Building

```bash
cargo build --release
```

## Configuration and usage

Documented once the server is implemented (see checklist above).

## License

MIT
