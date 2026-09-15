---
name: Pi Piper
description: A status wall for your own live pi sessions, shrunk to the phone.
colors:
  bg: "#0a0d12"
  bg-raised: "#12161d"
  bg-inset: "#0d1117"
  border: "#1c212b"
  border-strong: "#58627a"
  text: "#e6e9ef"
  muted: "#8b95a5"
  accent: "#5b9df5"
  accent-hover: "#7db0f7"
  on-accent: "#04101f"
  status-streaming: "#34d399"
  status-connecting: "#f5b84c"
  status-idle: "#8b95a5"
  status-down: "#f2646b"
  on-status-streaming: "#03241a"
  on-status-connecting: "#241705"
  on-status-down: "#2b0508"
typography:
  2xs:
    fontSize: "0.72rem"
    lineHeight: 1.4
  xs:
    fontSize: "0.78rem"
    lineHeight: 1.45
  sm:
    fontSize: "0.85rem"
    lineHeight: 1.5
  base:
    fontFamily: "Inter Variable, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif"
    fontSize: "0.92rem"
    fontWeight: 500
    lineHeight: 1.45
    letterSpacing: "normal"
  md:
    fontSize: "0.95rem"
    lineHeight: 1.5
  lg:
    fontSize: "1rem"
    lineHeight: 1.45
  mono:
    fontFamily: "JetBrains Mono Variable, ui-monospace, Menlo, Consolas, monospace"
    fontSize: "{typography.sm.fontSize}"
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: "normal"
  title:
    fontFamily: "{typography.base.fontFamily}"
    fontSize: "{typography.md.fontSize}"
    fontWeight: 600
    lineHeight: 1.3
rounded:
  xs: "0.25rem"
  sm: "0.4rem"
  md: "0.6rem"
  lg: "0.9rem"
  pill: "999px"
spacing:
  sm: "0.4rem"
  md: "0.65rem"
  lg: "0.9rem"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.on-accent}"
    rounded: "{rounded.md}"
    padding: "0.62rem 1rem"
  button-primary-hover:
    backgroundColor: "{colors.accent-hover}"
    textColor: "{colors.on-accent}"
  button-danger:
    backgroundColor: "{colors.status-down}"
    textColor: "{colors.on-status-down}"
    rounded: "{rounded.md}"
    padding: "0.62rem 1rem"
  button-secondary:
    backgroundColor: "{colors.bg-raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
    padding: "0.62rem 1rem"
---

# Design System: Pi Piper

## Overview

**Creative North Star: "The Operator's Status Wall"**

Pi Piper reads like an SRE's dashboard shrunk onto a phone: dense, semantic, and legible in under a second, never a rounded-bubble chat-app costume. Every session is a row on a status wall, not a conversation thread in a list — color and shape together say what's happening before anyone reads a word, and one interactive accent is spent only on things you can actually press. The world is quiet by default (near-black ground, hairline dividers, a restrained neutral+accent strategy appropriate to an Operate surface) and earns its color exclusively through meaning: status green/amber/red/slate never appear as decoration, only as fact.

This is a self-hosted, single-operator tool glanced at mid-task — at a standup, on a train, from bed — so density and speed of comprehension outrank warmth or personality. Brand lives in small, precise details (the P mark, the monospace data voice, the four distinct status shapes) rather than in an expressive surface. Rejected explicitly during the direction round: a generic AI-chat-bubble skin (the category default for this kind of tool) and literal retro-instrument skeuomorphism (dials, meters, phosphor-glow costume) — the chosen world takes the *legibility discipline* of an instrument bench without the hardware cosplay.

**Key Characteristics:**
- Status is read by shape *and* color, never color alone (accessibility-driven, not decorative).
- One accent color, reserved strictly for links, primary actions, and focus — never for meaning.
- Dense monospace for identifiers, paths, timestamps, and code; humanist sans for everything conversational.
- Flat throughout — no fake material, no glass, no gradients standing in for hierarchy.
- Two-pane at desktop widths (session rail + chat), single column on the phone.

## Colors

Near-black ground with a single reserved accent; status color is a closed, meaningful vocabulary layered on top, never mixed with the accent's role.

### Primary
- **Signal Blue** (`#5b9df5`): the one interactive accent. Primary buttons, links, focus rings, the brand mark, the active search/composer border. Never used to indicate session state.

### Neutral
- **Void** (`#0a0d12`): page ground.
- **Raised Slate** (`#12161d`): header, rail, composer, cards — one step up from the ground.
- **Inset Slate** (`#0d1117`): recessed surfaces (inputs, code blocks) — one step down.
- **Hairline** (`#1c212b`): dividers between rows and panes; deliberately low-contrast, a seam not a border.
- **Component Edge** (`#58627a`): the visible boundary on inputs, buttons, and dashed states — bright enough to read as an edge at a glance.
- **Foreground** (`#e6e9ef`): primary text.
- **Muted Slate** (`#8b95a5`): secondary text, placeholders, timestamps, the idle status color.

### Named Rules
**The One Accent Rule.** Signal Blue is the only color allowed to mean "you can act here." If a new interactive element needs emphasis, it gets Signal Blue or it gets weight/size — never a second accent hue.

**The Status Is Not Decoration Rule.** Streaming green, connecting amber, idle slate, and down red never appear on non-status UI. A "success" toast or a "new" badge borrows weight and icon, not a status color, so status color retains one meaning everywhere it appears.

## Typography

**UI Font:** Inter Variable (with `-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif` fallback)
**Data/Mono Font:** JetBrains Mono Variable (with `ui-monospace, Menlo, Consolas, monospace` fallback)

**Character:** A workhorse grotesk for everything conversational and chrome, and a purpose-built coding monospace for everything that's actually an identifier — session names, paths, timestamps, tool arguments, code, and log output. This is a deliberate Operate-mode choice: legibility and density over a display voice with a point of view (see the floor note on the `overused-font` detector finding below). Both are self-hosted static variable-font files, no external request at runtime.

### Hierarchy
Six deliberate steps, shared by both families (family is a separate axis from size) — every literal font-size in the codebase sets from one of these, no one-offs:

- **2xs** (0.72rem): the tool-call step badge, the session-aggregate readout.
- **xs** (0.78rem): hints, session paths/subtitles, autocomplete descriptions, tool-body output.
- **sm** (0.85rem): session preview lines, thinking blocks, tool-call summaries, toasts.
- **base** (0.92rem, 500 weight): buttons, inputs, the search field, the chat pane header, autocomplete command names.
- **md** (0.95rem, 600 for titles): session titles, chat bubbles, the **Title** role generally.
- **lg** (1rem): the brand wordmark and every real text input (`message-input`, `session-search`) — 1rem is also the iOS auto-zoom threshold, so form fields never drop below it.

### Named Rules
**The Identifier Rule.** Anything that is a stable, copy-pasteable identifier — a path, a session id, a command name, a timestamp, code — sets in JetBrains Mono. Anything conversational sets in Inter. Never mix the two to signal "technical" where the content isn't actually data.

**The Six-Step Rule.** Every font-size in the app is one of the six scale steps above. A new component that seems to need a seventh size should reconsider its weight or spacing instead.

## Layout

Mobile-first single column; a real two-pane shell opens at `min-width: 880px` (`--rail-width: 22rem` fixed rail + fluid chat pane), and an intermediate tablet band (`620px`–`880px`) centers a single capped-width column (`max-width: 40rem`) rather than stretching a phone layout full-bleed. The global header spans the full width at every size; the session rail doubles as the always-on left pane on desktop and the sole screen on mobile, toggled via the router's `hidden` attribute. Composer and toast stack are docked and thumb-reachable at the bottom on mobile. Spacing runs on an approximate 0.15rem–0.9rem rhythm (tight within a row, a full hairline-bordered gap between rows); list rows separate with 1px hairlines rather than card gaps.

## Elevation & Depth

Flat by design — no drop shadows on surfaces at rest. Depth reads through layered neutral tone only: ground → raised → inset, three fixed steps, never a fourth. The two exceptions are floating overlays that must read above the flow: the autocomplete popover and toasts carry a soft ambient shadow (`0 0.4–0.5rem 1.2–1.5rem rgba(0,0,0,0.35–0.4)`) purely to separate them from the page behind, not to imply a lit material.

### Shadow Vocabulary
- **Overlay** (`box-shadow: 0 0.5rem 1.5rem rgba(0,0,0,0.4)`): autocomplete popover only.
- **Toast** (`box-shadow: 0 0.4rem 1.2rem rgba(0,0,0,0.35)`): toast cards only.

### Named Rules
**The Three-Step Rule.** Depth is ground/raised/inset and nothing else. A component that needs a fourth level is miscategorized, not under-elevated.

## Shapes

Soft-technical, not soft-consumer: small, consistent corner radii on every interactive control and card, never a large "friendly app" radius and never sharp/neobrutalist corners. Five steps, same discipline as type: `0.25rem` (**xs** — inline code, the brand mark), `0.4rem` (**sm** — small controls, focus-ring corner), `0.6rem` (**md** — inputs, cards, most buttons), `0.9rem` (**lg** — bubbles, the message input, the autocomplete popover), and `999px` (**pill** — the scrollbar thumb, the tool-step badge). List rows are flat, full-bleed, hairline-divided rectangles with no radius or card shadow — the row *is* the divider, not a card floating on the ground. The one recurring signature geometry is the status dot: four distinct drawn shapes (solid disc, hollow ring, dashed rotating ring, disc with a diagonal slash), never a plain colored circle standing in for all four states.

## Components

### Buttons
- **Shape:** `0.6rem` radius, `0.62rem 1rem` padding, inline-flex with icon + label gap of `0.4rem`.
- **Primary** (Send, Save & connect): filled Signal Blue, `#04101f` text, 600 weight.
- **Danger** (Abort): filled Status Down red, `#2b0508` text, 600 weight.
- **Secondary** (default `button`, settings gear, back): Raised Slate fill, Component Edge border, 500 weight.
- **Hover / Focus:** `filter: brightness(1.12)` on hover, `transform: scale(0.97)` on active/press; focus-visible gets a 2px Signal Blue outline with 2px offset on every interactive element, no exceptions.
- **Icon:** every button icon is a real stroke-based SVG (Lucide, ISC-licensed) from the shared sprite — never emoji or a Unicode glyph.

### Status Dot (signature component)
- **Streaming/connected:** solid filled disc, Status Streaming green; adds a fading pulse ring while actively streaming.
- **Idle:** hollow ring, 1.5px Status Idle slate border, transparent center.
- **Connecting:** hollow ring, 1.5px dashed Status Connecting amber border, rotating.
- **Disconnected:** solid filled disc, Status Down red, with a diagonal slash mask — the one state that must never be mistaken for "just quieter."

### Session Row (list item)
- **Shape:** full-width flat row, no radius, 1px hairline bottom border, `0.75rem 1rem` padding.
- **Content:** status dot, title (Title style), path + relative time (Data style, truncated), optional one-line preview (Label style, truncated).
- **Hover:** Raised Slate background.
- **Active** (open in the chat pane, desktop rail only): a tinted fill — `color-mix(in srgb, var(--accent) 14%, var(--bg-raised))` — never a colored border-left stripe; the floor explicitly bans that pattern as the category's lazy "selected row" default.

### Tool-Call Card
- **Shape:** `<details>`/`<summary>` disclosure, Inset Slate background, hairline border, `0.9rem` radius.
- **States:** wrench icon while running → check (green) or x (red) icon on completion; arguments shown while running, hidden once complete; a chained call within the same agent turn (the second tool call onward) carries a small pill step badge (`#2`, `#3`, ...).

### Inputs
- **Style:** Inset Slate background, Component Edge border, `0.6–0.9rem` radius depending on context (search/message inputs use the larger `lg` radius as a pill-leaning field; other inputs use `md`).
- **Focus:** border shifts to Signal Blue; no glow/shadow trick.

### Aggregate Readout (signature component)
- **Style:** Data-style (JetBrains Mono) inline stats row above the session list — `N streaming · M idle · K down` — each count colored by its own status color, muted slate for idle. Hidden entirely when there are zero known sessions (the empty state carries that message instead).

## Do's and Don'ts

### Do:
- **Do** use JetBrains Mono for every identifier (paths, session ids, timestamps, command names, code/log content) and Inter for everything conversational.
- **Do** encode every status with a distinct shape as well as its color — never ship a fifth status that's "just a new color" on the existing dot.
- **Do** keep Signal Blue exclusive to interactive/actionable meaning; introduce weight or size for emphasis instead of a second accent.
- **Do** self-host any font or icon asset added to this project — no runtime CDN dependency, matching the project's no-build-step, single-binary deployment model.
- **Do** draw every icon as a real stroke-based SVG from the shared sprite (`/icons/sprite.svg`), one consistent 2px stroke and round joins.

### Don't:
- **Don't** add a colored `border-left`/`border-right` above 1px to a list row, card, or callout to indicate selection or category — use a tinted fill (see Session Row) or a named status shape instead.
- **Don't** use gradients, glass/blur-as-decoration, or hard offset "neobrutalist" shadows anywhere in this world — depth is the three-step ground/raised/inset system only.
- **Don't** use emoji or bare Unicode glyphs as icons, including in JS-generated content (tool-call icons, status glyphs) — always the sprite.
- **Don't** treat Inter as a slop finding here without reading the mode: this is an Operate surface (task completion, not persuasion), where a workhorse system-adjacent face is the correct call per the product's own design guidance, not an unexamined default. `impeccable detect`'s `overused-font` warning on this file is a known, accepted exception — do not "fix" it by swapping to a display face.
- **Don't** stretch the mobile single-column layout full-bleed at desktop widths — desktop always gets the real two-pane rail-plus-chat shell (`min-width: 880px`) or, at intermediate widths, a capped centered column, never a wide phone screen.
