# Pi Piper

Pi Piper is a small Rust hub that lets you control your **existing,
already-running, interactive `pi`** coding-agent sessions remotely from
your phone, over a private [Tailscale](https://tailscale.com) network —
and keep using the real terminal at the same time, fully synced. No
public hosting, no app store deployment: a webpage served privately on
your tailnet.

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for a code-oriented tour of how
it works, including the wire protocol and security model. This file is
setup-oriented.

## Status

Implemented: the Hub's session registry, `/agent` + `/ws` + `/ws/control`
+ `/api/sessions`, the `pi-piper-agent` extension (`/rc`, event forwarding,
command dispatch), and the PWA's session list + chat view. The handful
of RPC commands without a confirmed extension-API equivalent are listed
under "Known gaps" in [`pi-piper-agent/README.md`](pi-piper-agent/README.md).
Not implemented: Web Push notifications.

## How it fits together

```
Desktop terminal (any project)         Desktop terminal (another project)
┌─────────────────────────────┐        ┌─────────────────────────────┐
│ pi (interactive TUI)         │        │ pi (interactive TUI)         │
│ + pi-piper-agent extension      │        │ + pi-piper-agent extension      │
│ /rc  ── connects out ────────┼──┐     │ /rc  ── connects out ────────┼──┐
└─────────────────────────────┘  │     └─────────────────────────────┘  │
                                  ▼                                     ▼
                     ┌──────────────────────────────────────────────────┐
                     │                Pi Piper Hub (this repo)              │
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

Pi Piper relays pi's own documented RPC protocol (see pi's `docs/rpc.md`)
almost verbatim between the phone and each session; there's no separate
protocol invented to keep in sync. See [`ARCHITECTURE.md`](ARCHITECTURE.md)
for the exact wire shapes.

## Components

- **This repo (Rust)**: the Hub. Binds `127.0.0.1:4390` by default;
  expose it to your tailnet with `tailscale serve` (see below). Also
  serves the mobile PWA. The repo is also a
  [pi package](https://pi.dev/docs/latest/packages) — its
  `package.json` manifest registers the `pi-piper-agent` extension — so
  `pi install git:github.com/ritvijsrivastava/pi-piper` installs `/rc`
  straight from this repository.
- **[`pi-piper-agent/`](pi-piper-agent/README.md)** (TypeScript pi extension):
  install this once, globally, so `/rc` is available in every project.
  See its README for setup and known limitations.

## Quickstart

Install the Hub binary — `cargo install pi-piper` if you have a Rust
toolchain, or the no-toolchain install script (see
[Installing the Hub binary](#installing-the-hub-binary) for all paths,
including `cargo binstall`):

```bash
cargo install pi-piper
```

Start the Hub:

```bash
pi-piper
```

This starts the Hub with no project pre-configured — it's a pure
switchboard, waiting for `pi-piper-agent` connections. On first run it also
generates an **agent token** at `~/.pi/agent/pi-piper/agent-token` (mode
`0600`); `pi-piper-agent` reads this automatically, you never type or copy
it anywhere.

Install `pi-piper-agent` once so `/rc` is available in every `pi`
session:

```bash
pi install git:github.com/ritvijsrivastava/pi-piper   # this repo, as a pi package
# or, from a local checkout of this repo:
pi install /absolute/path/to/pi-piper/pi-piper-agent
```

Then in any `pi` session:

```
/rc
```

Open `https://<host>/` on your phone (see [Deployment](#deployment) for
exposing it over Tailscale) — you'll see that session in the list.

### Installing the Hub binary

The binary is self-contained — the mobile PWA is embedded at compile
time — so installing is always a single file. In order of convenience:

- **cargo (compiles from source):**

  ```bash
  cargo install pi-piper
  ```

- **cargo-binstall (prebuilt release tarball, no compilation):**

  ```bash
  cargo binstall pi-piper
  ```

- **install script (no Rust toolchain at all):** downloads the right
  static musl binary from
  [GitHub Releases](https://github.com/ritvijsrivastava/pi-piper/releases),
  verifies its sha256, and installs to `~/.local/bin`:

  ```bash
  curl -fsSL https://raw.githubusercontent.com/ritvijsrivastava/pi-piper/main/scripts/install.sh | sh
  ```

  Set `PI_PIPER_INSTALL_VERSION` to pin a version and
  `PI_PIPER_INSTALL_DIR` to change the destination.

- **from a checkout (development):** `cargo build --release` →
  `./target/release/pi-piper`.

Prebuilt binaries are static (`x86_64-unknown-linux-musl` and
`aarch64-unknown-linux-musl`, produced by the release workflow on every
`vX.Y.Z` tag), so they run on any Linux distro with no dependencies.
Upgrading is the same command you installed with; `pi-piper --version`
tells you what you're running.

### Configuration

All flags also accept an environment variable (see `--help`), which is
what the systemd deployment uses so secrets never appear in `ps` output:

| Flag | Env | Purpose |
|---|---|---|
| `--bind` | `PI_PIPER_BIND` | Address to listen on. Defaults to `127.0.0.1:4390`. |
| `--agent-token` | `PI_PIPER_AGENT_TOKEN` | Pin the agent token instead of auto-generating one. Usually left unset. |
| `--agent-token-path` | `PI_PIPER_AGENT_TOKEN_PATH` | Where to persist a generated agent token. Defaults to `~/.pi/agent/pi-piper/agent-token`. |
| `--project-dir` | `PI_PIPER_PROJECT_DIR` | Optional: also spawn a headless `pi --mode rpc` for one project with no terminal open (v1-compatible fallback). Most setups don't need this. |
| `--session`, `--no-session`, `--pi-arg` | `PI_PIPER_SESSION`, `PI_PIPER_NO_SESSION` | Only relevant together with `--project-dir`. |

**Auth note:** `/ws`, `/ws/control`, and `/api/sessions` (the
phone-facing routes) have no shared secret at all — any request that
arrives carrying the `Tailscale-User-Login` identity header `tailscale
serve` stamps onto everything it proxies in is authorized, regardless
of which tailnet login it names. In other words: any device signed
into your tailnet that can reach this Hub through `tailscale serve` can
see and control your `pi` sessions. `/agent` (where `pi-piper-agent`
extensions register sessions) is different — it's reached directly over
loopback by a local process, never proxied through `tailscale serve`,
so there's no identity header to trust there; it keeps its own separate
shared secret, the agent token. See [`ARCHITECTURE.md`](ARCHITECTURE.md)
for the full model and why Pi Piper cannot tell a proxied tailnet request
apart from a local process by peer address alone — which is also why
none of this is a substitute for restricting the tailnet itself (ACLs,
who's on it) and never running `tailscale funnel` in front of Pi Piper.

### Mobile client

Open `https://<host>/` (reachable once you've set up `tailscale serve`,
see [Deployment](#deployment)) in a phone browser that's signed into the
same tailnet. There is nothing to configure or type in — no token, no
login screen; access is entirely gated by Tailscale. "Add to Home
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
  `pi-piper-agent`'s README for the nicer-UI backlog item).

**Known limitation:** dialogs raised by *other* project extensions
(via `ctx.ui.select/confirm/input`) are answered at the terminal, not
proxied to the phone — see `pi-piper-agent/README.md` "Known gaps" for why
and what else doesn't have full parity yet when a session is connected
via `/rc` rather than the optional headless fallback.

## Deployment

Reachable from your phone only over Tailscale.

### 1. Install the binary

Any path from
[Installing the Hub binary](#installing-the-hub-binary) works; the unit
below expects it at `/usr/local/bin/pi-piper`:

```bash
# from a checkout:
cargo build --release
sudo install -m 755 target/release/pi-piper /usr/local/bin/pi-piper
# or, from a release:
sudo install -m 755 ~/.local/bin/pi-piper /usr/local/bin/pi-piper
```

### 2. Configure

```bash
sudo install -d /etc/pi-piper
sudo install -m 600 deploy/pi-piper.env.example /etc/pi-piper/pi-piper.env
sudo $EDITOR /etc/pi-piper/pi-piper.env   # defaults are fine for most setups
```

### 3. Install the systemd unit

```bash
sudo cp deploy/pi-piper.service /etc/systemd/system/pi-piper.service
sudo $EDITOR /etc/systemd/system/pi-piper.service   # set User=
sudo systemctl daemon-reload
sudo systemctl enable --now pi-piper
sudo systemctl status pi-piper
```

The `User=` you pick is also the user whose `pi` sessions can `/rc` into
this Hub (it's the user whose `~/.pi/agent/pi-piper/agent-token` gets
generated and read).

### 4. Install `pi-piper-agent`

```bash
pi install git:github.com/ritvijsrivastava/pi-piper
```

(or from a local checkout: `pi install /absolute/path/to/pi-piper/pi-piper-agent`).
See [`pi-piper-agent/README.md`](pi-piper-agent/README.md) for details and
known limitations. Do this as the same user configured above.

### 5. Expose it on your tailnet with `tailscale serve`

Install [Tailscale](https://tailscale.com/download) on this machine and
sign in (`tailscale up`), then:

```bash
tailscale serve --bg 4390
```

This proxies your tailnet's HTTPS address (with a certificate Tailscale
manages automatically) to Pi Piper's loopback port. Useful commands:

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
3. Open `https://your-machine.your-tailnet.ts.net/` in your phone's
   browser. No token or login — being on the tailnet is the whole
   credential.
4. "Add to Home Screen" to install it as a standalone app icon.
5. In any `pi` terminal session (on the Hub's machine), run `/rc`. It
   appears in the phone's session list within a second or two.

Nothing here is reachable from the public internet — only devices signed
into your tailnet can resolve or reach that hostname at all, and any
device that can is authorized (see the auth note above).

## Development

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
node --test
./scripts/check-versions.sh
```

CI (`.github/workflows/ci.yml`) runs exactly these checks on every push
and pull request; `.github/workflows/release.yml` builds the musl
release tarballs and publishes the crate to crates.io on a `vX.Y.Z` tag
(requires a `CARGO_REGISTRY_TOKEN` repository secret).

Integration tests spawn a real `pi --mode rpc` process (for the
headless-link tests, `tests/websocket_bridge.rs`) and drive the full
axum router over a real WebSocket. `tests/agent_bridge.rs` covers the
`/agent` <-> `/ws` <-> `/ws/control` <-> `/api/sessions` machinery with a
plain WebSocket client standing in for `pi-piper-agent` (the wire protocol
is deliberately small and transport-only, see [`ARCHITECTURE.md`](ARCHITECTURE.md),
so this
doesn't need a real extension to exercise). Neither test suite calls the
configured LLM, so both run without network access or API cost beyond
whatever `pi` itself needs to start up.

For the extension side, see
[`pi-piper-agent/README.md`](pi-piper-agent/README.md#development).

## License

[MIT](LICENSE)
