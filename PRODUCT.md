# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Developers who run the `pi` coding-agent interactively in a terminal
and want to keep steering a live session from their phone. Confirmed:
each operator runs their own Piper Hub on their own private Tailscale
network (multi-operator, not a single shared instance) — the user
explicitly expects others to adopt it on their own tailnets, not just
themselves. Each operator's Hub still serves only that operator's own
sessions; there is no cross-tenant sharing of sessions between
operators within a Hub. The phone is a companion to an already-running
desktop terminal, not a replacement for it — the same live session is
driven from both at once, fully synced.

## Product Purpose

Piper lets a `pi` session started normally in a desktop terminal opt
into remote control (`/rc`) without restarting it, then be viewed and
driven from a mobile browser (installable PWA) over a private
Tailscale network — prompts, steering, aborts, model switches, slash
commands with autocomplete — while the terminal keeps working
normally and in sync. Success is a developer who steps away from their
desk mid-task and can keep working from their phone with no loss of
fidelity versus the terminal, then walk back to the same live state.

## Positioning

Unlike a hosted "AI chat app," Piper adds no new backend product and
invents no new protocol: it relays `pi`'s own documented RPC protocol
almost verbatim between phone and terminal, and it never leaves the
operator's private Tailscale network (no public hosting, no app store).
The phone and the terminal are two live clients of the *same* running
session, not two separate conversations — whatever is typed in one
appears in the other in real time.

## Operating Context

- One long-running Rust binary (the Hub) per operator, typically
  deployed via systemd, bound to loopback and exposed on that
  operator's own tailnet with `tailscale serve` (never `tailscale
  funnel`/public internet).
- `piper-agent`, a TypeScript `pi` extension installed globally, gives
  any interactive `pi` session the `/rc` command to register with that
  operator's Hub.
- The phone client is a installable PWA (session list + chat view),
  opened over `https://<tailnet-host>/`, with per-browser localStorage
  token storage.
- Multiple simultaneous sessions, possibly across different projects,
  are listed Claude-mobile-style and switched between.
- Concurrent input from terminal + phone on the same session is
  "last one wins" — no locking/turn-taking model.

## Capabilities and Constraints

- Confirmed functionality: session registry (`/agent`), phone-facing
  `/ws` (session) and `/ws/control` (list) WebSockets, `/api/sessions`
  HTTP snapshot, slash-command dispatch and autocomplete, markdown +
  code-block rendering, collapsible tool-call and thinking-block cards,
  streaming/queue indicators, extension UI dialogs (`select`/
  `confirm`/`input`/`editor`/`notify`/`setStatus`/`setWidget`).
- Not implemented: Web Push notifications (background-finished-session
  alerts) — stretch, not required for the current milestone.
- Known limitation: `ctx.ui` dialogs raised by *other* project
  extensions (not `piper-agent` itself) are answered at the terminal
  only, not proxied to the phone, when connected via `/rc`.
- No phone-side secret by design: `/ws`, `/ws/control`, and
  `/api/sessions` authorize any request carrying the
  `Tailscale-User-Login` identity header `tailscale serve` stamps onto
  everything it proxies in — any device signed into the tailnet is
  authorized, nothing to type or configure. A separate, auto-generated
  agent token still gates `/agent` (where `piper-agent` registers
  sessions), because that endpoint is reached directly over loopback by
  a local process and never proxied through `tailscale serve`, so
  there's no identity header to trust there.
- No framework/build step for the frontend by design (plain HTML/CSS/
  JS, embedded into the compiled binary via `rust-embed`); the Rust
  backend uses `axum`/`tokio`.
- Multi-user access control within one Hub is explicitly a non-goal:
  each Hub instance is single-operator, single-tailnet. (Multiple
  independent operators each running their own Hub on their own
  tailnet, as confirmed above, is the supported multi-adopter model —
  distinct from multiple people sharing one Hub/tailnet.)

## Brand Commitments

**Assumed, not confirmed — flag if wrong:** "Piper" is treated as the
settled product name and the existing `assets/icons/` mark as the
current identity, since the user moved directly to UI work without
correcting either. No other visual/brand references were made binding.

## Evidence on Hand

No marketing copy, testimonials, case studies, or press exist or are
to be fabricated — this is a developer tool documented by its own
README.md and SPEC.md, not a persuasion surface. `SPEC.md` §13 records
exactly what has been validated end-to-end versus covered only by
automated tests; treat anything not listed there as unverified in
production use.

## Product Principles

1. The terminal stays the source of truth and never loses fidelity —
   the phone is a synced second client, not a replacement.
2. Relay `pi`'s existing RPC protocol verbatim wherever possible;
   invent new wire shapes only where genuinely nothing exists yet
   (agent registration, control channel).
3. Never widen the network exposure implied by "Tailscale-only, no
   public hosting" for convenience.
4. Defense-in-depth on auth (two distinct tokens) without pretending
   either token is the primary security boundary — the tailnet is.
5. No framework/build step for the frontend unless a future decision
   explicitly changes that constraint.

## Accessibility & Inclusion

**Assumed, not confirmed — flag if wrong:** standard web-accessibility
baseline (semantic HTML, `aria-live` regions already present for the
transcript/toasts, visible focus states, sufficient contrast in the
dark theme). No screen-reader-first or other elevated requirement was
established.
