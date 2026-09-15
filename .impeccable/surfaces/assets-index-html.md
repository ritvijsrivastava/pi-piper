---
version: 1
slug: "assets-index-html"
primary_target: "assets/index.html"
related_targets: ["assets/style.css","assets/app.js"]
---

## Scope & mode

Full PWA redesign: session list, chat view, composer, settings panel. Mode: **Operate** (task completion — monitor and steer live `pi` sessions from a phone; expression may never obscure state or the task).

## Audience, job, constraints

Developers who self-host their own Pi Piper Hub on their own private Tailscale network, glancing at a phone mid-task to check on or steer an already-running `pi` session. Must stay fast, legible at a glance, low-noise, trustworthy for a security-sensitive remote-access tool. No framework/build step — static HTML/CSS/JS embedded via `rust-embed`; self-hosted static font files are allowed (still no bundler). Preserve all existing functionality, RPC/WebSocket wiring, and accessibility baseline (`aria-live` regions, semantic HTML).

## Direction contract

**THESIS:** Pi Piper's session list and chat view become an SRE/NOC status wall shrunk to the phone — dense, semantic, legible in under a second — refusing the generic rounded-bubble AI-chat-app default this category always ships.

**OWN-WORLD:** near-black ground `#0a0d12`, raised surface `#12161d`, hairline border `#232a35`, primary text `#e6e9ef`, muted text `#8b95a5`. Status owns meaning and nothing else: green `#34d399` streaming, amber `#f5b84c` connecting/warning, slate `#8b95a5` idle, red `#f2646b` disconnected — kept strictly separate from one cool interactive accent, blue `#5b9df5`, reserved for links, primary actions, and focus rings. Inter for UI chrome; JetBrains Mono for session names, paths, timestamps, code, and logs. Both self-hosted as static woff2 files, no CDN, no build step.

**STORY:** a developer opens the app and reads full system state in under a second — an aggregate "N streaming · M idle · K down" readout, then per-session status by color and shape (never color alone) — taps into one session, and the chat view reads like a calm systems log: tight monospace metadata, generous markdown prose, quiet collapsible tool-call cards with a visible step index when chained.

**FIRST VIEWPORT:** session list as a dense status grid. Each row: status dot/ring (semantic color + distinct shape per state) + session name (Inter, medium) + cwd path (mono, muted) + relative last-activity + one-line preview. Aggregate readout pinned above the list; search as a quiet hairline field, not a floating pill. Desktop: a fixed ~360px list rail beside the chat pane — never today's single column stretched full width. Composer stays docked and thumb-reachable at the bottom on mobile.

**FORM:** IMPECCABLE'S PICK — NOC / SRE Video Wall. Seed key `774d2335` (`--scope direction --mode operate`).

**FINISH:** unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.

## Unresolved decisions

- Exact motion grammar for streaming/state-change transitions (kept purposeful, not decorative).
- Whether the aggregate readout persists inside the chat view header or only on the session list.
