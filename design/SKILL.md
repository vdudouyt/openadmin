---
name: cfdns-design
description: Use this skill to generate well-branded interfaces and assets for cfdns and OpenAdmin — text-UI (ncurses/ratatui-style) console tools for Cloudflare DNS and remote host management — either for production or throwaway prototypes/mocks. Contains essential design guidelines, terminal color + type tokens, box-drawing rules, the ASCII brand mark, importable components, and two interactive TUI UI kits for prototyping.
user-invocable: true
---

Read the `README.md` file within this skill, and explore the other available files.

This is a **text-UI** design system: everything is monospace characters on a
fixed grid, dim-white on dark gray with one Cloudflare-orange accent, structured
with box-drawing glyphs and an F-key action bar. Honor the medium's limits — no
gradients, shadows, pixel radii, or drawn artwork; hierarchy comes from color,
weight, CASE and reverse-video, never point size. Two signals stay distinct:
the **orange cursor bar** (where you are) and **yellow `●` marks** (what you
multi-selected with `Insert`).

Key files:
- `README.md` — context, CONTENT FUNDAMENTALS, VISUAL FOUNDATIONS, ICONOGRAPHY,
  a Components index, and a file index.
- `styles.css` — the single stylesheet entry point (imports the foundations).
- `colors_and_type.css` — color + type tokens and `.tui-*` helper classes.
- `assets/cloudflare-ascii-logo.txt` — the honest brand mark (solid orange block).
- `preview/` — design-system cards (type, color, spacing, box-drawing, components).
- `components/` — importable React components: **TuiPanel** (titled box-drawing
  frame), **FunctionBar** (bottom F-key strip), **StatusBar** (context row).
- `ui_kits/dns-client/` — interactive cfdns TUI (DNS records, bulk-add wizard).
- `ui_kits/openadmin/` — interactive OpenAdmin TUI: host CRUD with `Insert`
  multi-select and auto `/net/<name>` mounts, grouped shell tabs, agentic chat.
  Its `tui.jsx` holds reusable grid primitives (`Row`, `BodyRow`, borders,
  `Screen`, field helpers).

If creating visual artifacts (slides, mocks, throwaway prototypes), copy assets
out and create static HTML files for the user to view; reuse `styles.css` and the
`tui.jsx` primitives so box-drawing stays aligned (rows are `white-space:pre`,
color via inline `<span>`; verify glyph widths are 1 cell — `▲ ○ ✓ █ ░ ▸ ●` are
safe, `◆ ★ ✗ ❯ ⚡` are not). If working on production code, copy assets and read
the rules here to become an expert in designing with this brand.

If the user invokes this skill without other guidance, ask them what they want to
build or design, ask a few questions, and act as an expert designer who outputs
HTML artifacts _or_ production code, depending on the need.
