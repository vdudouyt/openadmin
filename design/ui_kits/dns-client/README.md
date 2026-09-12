# UI Kit · cfdns DNS Client (TUI)

A high-fidelity, interactive recreation of the **cfdns** terminal UI — a console
client for Cloudflare DNS. Open `index.html` to boot the full app.

This is the assembled, clickable expression of the design system: the records
grid, the F-key action bar, and every dialog, all drawn on a scaled character
grid exactly as the foundations prescribe.

## Run it
Open `index.html`. The 104-column screen scales to fit any viewport (letterboxed
on black). Drive it with the mouse (click rows, click F-keys) **or** the keyboard.

### Keys
- `↑ ↓` select · `Home/End` jump · `Enter` edit
- `F2` add · `F3` edit · `F4` toggle proxy · `F5` bulk add · `F6` filter ·
  `F8` delete · `F1` help · `F10` quit
- In dialogs: `Tab` move field · `←/→` change select/radio · `Enter` save · `Esc` cancel

## The signature flow — Bulk Add (`F5`)
Pick a type + proxy state, enter a **seed hostname containing a number**
(`s1000.mydomain.com`) and a list of IPs (one per line). Step 2 previews the
expansion before committing:
```
s1000.mydomain.com => 203.0.113.10
s1001.mydomain.com => 203.0.113.11
…
```
The number is incremented per IP, preserving zero-pad width.

## Component map
| File | What it provides |
|------|------------------|
| `tui.jsx` | Core grid primitives: `Row`, `BodyRow`, `TopBorder`/`SepBorder`/`BotBorder`, `Panel`, segment renderer (`buildSpans`), the scaling `Screen`, color tokens `C`, `COLS`/`ROWS`. |
| `data.js` | Sample zone, record-type metadata (`TYPE_META`), TTL formatting, type colors. |
| `Header.jsx` | Brand block + wordmark + zone/account band. |
| `RecordsTable.jsx` | The central scrollable grid — columns, zebra, reverse-video selection, proxy glyphs, scroll window. |
| `Chrome.jsx` | `StatusBar` (context + transient message + spinner) and `FunctionBar` (context-sensitive F-keys). |
| `Dialogs.jsx` | Shared field/select/radio/button builders, `AddEditDialog`, `DeleteDialog` (double-border modal), `HelpOverlay`. |
| `BulkWizard.jsx` | Two-step bulk-add wizard + the `expand()` hostname-numbering logic. |
| `app.jsx` | App state, global keyboard routing, and wiring. |
| `colors_and_type.css` | Copied from the design-system root — tokens + helper classes. |

## How it's built (and what's faked)
- Everything is rendered as **character rows** (`<div class="tui-row">`, `white-space:pre`)
  so box-drawing glyphs connect; color is applied with inline `<span>`. The whole
  screen is a fixed 104×~34 cell grid scaled with `transform`.
- The brand mark is a **solid CSS block**, not drawn art — a terminal can't render
  a logo at one cell per glyph, so the system uses an honest rectangle.
- State is in-memory (no real Cloudflare API). Validation is light (non-empty).
  These are cosmetic recreations, not production components.
