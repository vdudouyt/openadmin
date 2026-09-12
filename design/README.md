# cfdns — Cloudflare DNS Console · Design System

A design system for a **text-based / terminal user interface (TUI)** — the kind
built with **ncurses** or **ratatui** and run inside a terminal emulator. The
product is **cfdns**, a console client for managing Cloudflare DNS records:
list every record type, full CRUD, the proxied/DNS-only toggle, and a signature
**bulk-add** flow for sequentially-numbered hostnames.

This system documents how to design *anything* in that world — screens, dialogs,
tables, the function-key action bar — so it looks deliberate and professional
within the hard constraints of a character grid.

> **Sources.** This system was authored from a written product brief (no
> codebase or Figma was provided). There is therefore no upstream repo or design
> file to link. If you have the real `cfdns` source, drop it into the project and
> reconcile the UI kit against it.

---

## The medium & its constraints

Everything renders as **monospace characters on a fixed grid**. That single fact
drives every rule here:

- **No pixels, only cells.** Layout is measured in character cells (columns ×
  rows). Alignment is achieved by padding with spaces and drawing with
  box-drawing glyphs (`┌ ─ ┐ │ ├ ┼`), not CSS boxes.
- **One font, one size.** There is no type scale. Hierarchy comes from
  **color, weight, CASE, and reverse-video**, never from point size.
- **Color is the primary expressive tool.** A dim-white foreground on dark gray,
  with Cloudflare orange as the *only* accent, plus a small desaturated semantic
  palette (green/red/blue/yellow).
- **Input is keyboard-first.** Every action has an `F`-key. The mouse is a
  convenience, never a requirement. The bottom function bar is always visible.
- **Things you cannot draw:** drop shadows, gradients, rounded pixel corners,
  arbitrary image art, sub-cell positioning, free-floating overlap. If a design
  idea needs any of those, it is wrong for this medium. Embrace the limits.

---

## CONTENT FUNDAMENTALS

How copy is written in cfdns.

- **Voice: terse, operator-to-operator.** This is a power tool for people who
  live in the terminal. No marketing, no hand-holding, no exclamation points.
- **Casing.**
  - Screen / panel titles: **Title Case** — `Add DNS Record`, `Bulk Add`.
  - Column headers: **UPPERCASE**, lightly tracked — `TYPE  NAME  CONTENT  TTL  PROXY`.
  - Function-key labels: **single Title-Case word** — `Add`, `Edit`, `Delete`,
    `Bulk`, `Filter`, `Help`, `Quit`.
  - Body values: rendered **verbatim** (a hostname or IP is never re-cased).
- **Person.** Address the operator as **you** in hints (`Press F2 to add a
  record`). System state speaks in the **third person, present tense**
  (`12 records · 3 proxied`). Never "we".
- **Brevity over grammar.** Status lines drop articles: `Saved.` ·
  `No records match filter.` · `Loading zone…` (trailing ellipsis = in progress).
- **Confirmations are explicit and quantified.** `Delete 3 records? This cannot
  be undone.  [Y]es  [N]o` — always state the count and the consequence.
- **Errors are diagnostic, not apologetic.** `A record "www" already exists.` ·
  `Content is not a valid IPv4 address.` Tell the operator what is wrong and,
  where possible, which field.
- **No emoji.** None, anywhere. (See ICONOGRAPHY for the few Unicode marks that
  *are* allowed.)
- **Units & keys are explicit.** TTL shows `Auto` or `300s`; keys are written
  `F2`, `^C` (Ctrl+C), `↹` (Tab), `↵` (Enter), `Esc`.
- **Vibe:** calm, dense, trustworthy. The aesthetic of `lazygit`, `k9s`, `btop`,
  Midnight Commander — but unmistakably Cloudflare via the orange and the proxy ▲.

Example status line:
```
 zone: mydomain.com   12 records · 3 proxied      ▲ proxied  ○ dns-only      Saved.
```

---

## VISUAL FOUNDATIONS

### Color
- **Canvas:** dark gray, stepped — `--bg-base #1a1a1c` for the terminal, with
  barely-there steps for panels (`#212124`), zebra rows (`#232327`) and recessed
  wells/textareas (`#151517`). Contrast between surfaces is intentionally *low*;
  structure is carried by box-drawing lines, not by big fills.
- **Foreground:** a **dim** white — `--fg #c2c2c6` is the default, never pure
  `#fff`. Stepped down through `--fg-muted`, `--fg-faint`, `--fg-disabled` for
  secondary/tertiary/disabled text. `--fg-bright #e6e6e8` is reserved for the
  logo, focused-selection text, and key caps.
- **Accent:** Cloudflare orange `--orange #f6821f`, with `--orange-bright` for
  hover/highlight and `--orange-dim` for pressed/borders. It marks: the proxied
  cloud, the focused panel border, the active selection bar, function-key caps,
  and the logo. Used **sparingly** — if everything is orange, nothing is.
- **Semantic hues** are desaturated so they sit in the terminal world:
  `--green` (success), `--red` (destructive/error), `--blue` (record-type
  accent / info), `--yellow` (warning/pending), `--magenta` (rare, priorities).
- See `colors_and_type.css` for the full token set.

### Type
- **JetBrains Mono**, one size per app (`--fs 15px` default; `13px` compact,
  `17px` cozy). Hierarchy = weight (400/500/700) + color + CASE + reverse-video.
- Ligatures **off** (a data grid must never fuse `->` or `==`).
- **Letter-spacing is always `0`.** On a `white-space: pre` character grid any
  tracking accumulates across the row and walks every column out of alignment —
  at `0.06em` a ~108-character header drifts ~97px and breaks out of its frame.
  Uppercase column headers get their hierarchy from **color, weight and CASE**,
  never from tracking. (Outside the grid — the wordmark, a CSS-laid-out header
  band — tracking is fine.)

### Spacing
- The unit is the **character cell** (`--cell-h` line-height, `--cell-w`
  advance). Padding inside panels is **1 cell** horizontally; sections are
  separated by **1 blank row** or a `├────┤` rule. Never half-cells.

### Backgrounds & surfaces
- **Flat, always.** No images, no gradients, no texture, no blur. Depth is faked
  only by the low-contrast surface steps above and by border color. The closest
  thing to "elevation" is a dialog: same flat panel fill, drawn with a brighter
  (often orange) double or single border and an optional 1-cell drop of `░`
  shadow characters down its right/bottom edge if a retro feel is wanted
  (off by default — see Spacing card).

### Borders (the structure layer)
- **Single-line** `┌─┐│└┘├┤┬┴┼` in `--line #34343a` is the default frame.
- **The focused panel** switches its border to `--orange`. Only one panel is
  focused at a time — this is the primary "where am I" cue.
- **Double-line** `╔═╗║` (in `--orange-dim`) reserved for **modal** dialogs that
  block the app (delete confirm, errors).
- **Rounded** `╭╮╰╯` is an optional softer style for non-modal info popovers.
- **Heavy** `┏━┓` is avoided except for a single emphasized divider.
- Column separators inside a table are single `│` in `--line`.

### Animation
- A terminal **repaints**; it does not ease. There are **no CSS transitions** on
  structure. Permitted motion:
  - **Blinking cursor** (`█` toggling ~1.06s) in active inputs.
  - **Spinner** for async work, cycling Braille frames `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` (~80ms/frame).
  - **Instant** state changes (selection moves, panels open) — no fade, no slide.
- Reverse-video selection appears/disappears instantly. Respect
  `prefers-reduced-motion` by freezing spinner/cursor.

### Hover / focus / press
- **Hover (mouse):** row background lifts to `--bg-hover #2c2c31`. Subtle.
- **Selection (keyboard):** the current row is a full-width **reverse-video bar**
  — `--bg-sel` warm wash normally, `--orange` fill with `--orange-ink` text when
  its panel is focused. This is the dominant interaction signal.
- **Focused field:** label turns `--orange`, the field well border turns orange,
  a block cursor `█` blinks.
- **Press (button/key):** flash the key cap to a solid `--orange` fill for one
  frame; there is no scale/shadow to animate.
- **Marked / multi-selected rows (Insert):** `--yellow-mark #e8c45a` with a `●`
  in the gutter — the file-manager tagging convention. Marks are **orthogonal to
  the cursor**: the cursor says *where you are* (one row), marks say *what you
  tagged* (zero-to-many). Yellow separates from the orange cursor bar by **hue**,
  not just lightness, so a marked row stays legible while cursored. Never reuse
  orange for marks — it collapses the two signals.
- **Disabled:** text drops to `--fg-disabled`; key cap loses its accent.

### Corner radii, shadows, cards
- **Radius: 0** everywhere — corners are box-drawing glyphs (`┌` or the rounded
  `╭`). There is no such thing as a pixel radius here.
- **Shadows: none** (optional retro `░` character-shadow on modals only).
- **"Cards"** are **panels**: a titled box drawn with box characters, title
  inset into the top rule like `┌─ Records ─────────┐`, 1-cell internal padding,
  flat fill. That is the universal container.

### Layout rules (fixed regions)
The app is a fixed three-zone stack that fills the terminal:
1. **Header (top, fixed):** the brand block + wordmark on the left; zone selector /
   account on the right.
2. **Body (center, flexible):** fills *all* remaining rows. Primary content is
   the records table; dialogs open centered over it.
3. **Function bar (bottom, fixed):** `F1…F10` key caps + labels, full width.
   Optionally a 1-row status line sits just above it.

---

## ICONOGRAPHY

There are no image icons in a TUI — "icons" are **single Unicode glyphs** chosen
for unambiguous shape at one cell. The system is deliberately tiny:

- **Proxy state** (the signature mark): `▲` orange = **proxied**, `○` grey =
  **DNS only**. (Alternate weather set `☁ / ◌` if the font renders it cleanly.)
- **Status marks:** `✓` green (ok/saved), `●` (live/dirty), `…` (in progress),
  `!` yellow inside `[ ! ]` (warning), `×` red for failure.
- **Selection / focus:** `▸` orange marks the active row/field; `●` yellow marks a
  multi-selected row; `»` prompts an input; `▾ ▴` for collapsible sections;
  `← → ↑ ↓` for directions (all exactly one cell).

### Glyph width — the rule that breaks grids

On a `white-space: pre` row padded by **character count**, any glyph whose
advance isn't exactly one cell shifts everything after it and pushes the row's
border out of the frame. Measured at 15px JetBrains Mono (cell = **8.99px**):

- **Grid-safe (1 cell):** `● ○ ▲ ▸ ▾ ▴ ► ✓ … · • × » ← → ↑ ↓ █ ▓ ▒ ░` and all box-drawing.
- **NEVER in a grid row:** `↹` `⇧` (14.99) · `↵` (12.56) · `⠹` (10.49) · `✗` (9.95) ·
  `❯` (8.29) · `◆ ★ ⚡`.
- **In grid rows, spell keys out:** `Tab`, `Enter`, `Shift+Enter`, `Esc`, `^C`.
  The pretty key glyphs are fine in **CSS-laid-out chrome** — function-bar key
  caps, tabs, headers — where nothing aligns by character count.
- Always measure a new glyph before using it in a row.
- **Checkbox / radio:** `[x]` / `[ ]` and `(•)` / `( )` — literal ASCII, never a
  graphical control.
- **Spinner:** Braille cycle `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`.
- **Meters / bars:** block shades `█▓▒░` and partial blocks for gauges.
- **Logo:** the brand mark in `assets/cloudflare-ascii-logo.txt` — deliberately
  NOT a drawn cloud. A terminal can't render logo artwork at one cell per glyph,
  so the mark is an honest primitive: a plain solid orange block, 2 rows tall to
  match the wordmark beside it (rounded with quadrant blocks ▟▙▜▛). Single tone.

Rules: **no emoji ever**; prefer geometric Unicode (▲ ● ▸ ✓ ×) over pictograms;
every glyph must be legible at one cell, measure exactly one cell (see above),
and carry meaning by *color* as much as shape. If a glyph doesn't render in
common terminal fonts, fall back to ASCII (`->`, `[x]`, `*`).

---

## INDEX — what's in this system

Root files:
- **`README.md`** — this file: context, content & visual foundations, iconography.
- **`colors_and_type.css`** — all color + type tokens and semantic helper classes.
- **`styles.css`** — the single entry point; `@import`s the foundations above.
- **`thumbnail.html`** — the system's tile.
- **`SKILL.md`** — Agent-Skill manifest for reuse in Claude Code.
- **`assets/`** — `cloudflare-ascii-logo.txt` (brand block + wordmark, three sizes).
- **`preview/`** — design-system cards (type, color, spacing, box-drawing,
  components) shown in the Design System tab.

## Components

Importable React components, one per `components/<Name>/` folder and exposed on
the design system's window namespace:

- **`TuiPanel`** — the universal container: a titled box-drawing frame with inset
  title, 1-cell padding, and dim / orange-focused / double-modal borders.
- **`FunctionBar`** — the always-visible bottom F-key action strip, with active
  (inverted cap) and destructive (red label) states.
- **`StatusBar`** — the context row above the function bar: left context, a middle
  hover hint, and a right transient status with ok / err / warn tones.

Everything else in the system is a *pattern* documented by the cards and
recreated in the UI kits rather than a packaged component — in a character-grid
UI most "components" are row-drawing conventions, not reusable widgets.

UI kits:
- **`ui_kits/dns-client/`** — high-fidelity, interactive recreation of the cfdns
  TUI. `index.html` boots the full app (records table, F-key bar, add/edit/delete
  dialogs, and the bulk-add wizard). See its own `README.md` for the component
  map.
- **`ui_kits/openadmin/`** — OpenAdmin: a three-screen remote-host manager built
  on the same grid vocabulary (hosts + CRUD with Insert multi-select, grouped
  shell tabs, agentic chat). See its own `README.md`.

Start with `index.html` in the UI kit to see the system assembled; read
`colors_and_type.css` for tokens; consult the cards in `preview/` for the rules
behind each piece.
