# UI Kit · OpenAdmin (TUI)

A three-screen terminal application for managing remote hosts — built on the
cfdns design system's character-grid vocabulary, palette, and F-key conventions.

Open `index.html`. The 120-column screen scales to fit the viewport.

## Screens

**1 · Hosts** (`Alt+1`) — the known-hosts table with full CRUD.
Columns: `NAME · TYPE · ADDR · PORT · MOUNT POINT · LOGIN · PASSWORD · KEY · MNT · PRX`.
- **Multi-select with `Insert`**, file-manager style. Marked rows render in
  `--yellow-mark` with a `●` in the gutter, so they stay legible *through* the
  orange cursor bar. `*` inverts, `Ctrl+A` selects all, `Esc` clears.
- Actions apply to the **marked set** when there is one, otherwise to the row
  under the cursor.
- **Mount point auto-derives** from the host name as `/net/<name>` and keeps
  following it until the user edits the mount field; clearing the field restores
  auto mode. The hint line under the field states which mode it is in.
- **KEY** column: `✓` when a key is installed, `[gen]` (underlined, clickable)
  when not — opens the public-key dialog for pasting into the remote host's
  `authorized_keys`.
- **MNT / PRX** show mount and proxy state as `●`/`○`.

**2 · Shells** (`Alt+2`) — open shells as tabs, named after their host.
Opening a shell with **several hosts marked** creates one tab named
`Group: <first host>` containing a **vertically stacked pane per host**, each
with its own titled frame, scrollback and prompt. Click a pane to focus it;
`Tab` cycles tabs; the `×` on a tab appears on hover.

**3 · Chat** (`Alt+3`) — agentic chat, opencode-inspired. User turns are marked
with an orange `»`, assistant prose is plain, and **tool calls render as boxed
subtrees** (`┌ ssh · web-01 · … ✓ ok`) with their captured output indented under
a `│ └` rule. Input sits in a framed composer with a blinking block cursor.

## Keys
`Alt+1/2/3` or `F9` switch screens · `F1` help · `F10` quit
Hosts: `↑↓` move · `Insert` mark · `F2` add · `F3` edit · `F4` mount/unmount ·
`F5` shell · `F6` proxy · `F7` generate key · `F8` delete

## Mouse
Modern terminals report mouse events, and this kit leans on that:
- **Hover** highlights the row and prints a contextual line in the status bar
  (`deploy@10.0.4.11:22 · mounted at /net/web-01 · double-click for a shell`).
- **Click** moves the cursor; **double-click** opens a shell.
- Screen tabs, shell tabs, tab close buttons, `[gen]`, dialog buttons and every
  F-key in the bottom bar are clickable with their own hover states.

## Component map
| File | What it provides |
|------|------------------|
| `tui.jsx` | Grid primitives shared with cfdns, widened to 120 cols: `Row`, `BodyRow`, borders, `Screen`, field/select helpers, `Spinner`, color tokens `C`. |
| `data.js` | Sample hosts, `mountFor()` derivation, shell scrollback, chat transcript, public key. |
| `AppChrome.jsx` | Header with brand block + screen tabs, status bar, function-key bar. |
| `HostsScreen.jsx` | The hosts table — columns, marks, cursor, hover, state glyphs. |
| `HostDialogs.jsx` | Add/Edit host, multi-target delete confirm, SSH public key output, help. |
| `ShellsScreen.jsx` | Tab bar, single and stacked shell panes. |
| `ChatScreen.jsx` | Transcript renderer (prose wrap + tool-call boxes) and composer. |
| `app.jsx` | State, keyboard routing, screen/modal wiring. |

## What's faked
No real SSH/FTP, mounting, or model calls. Shell scrollback is canned, the chat
replies on a timer, and the public key is a fixed sample. Validation is light.
These are cosmetic recreations for design work, not production components.
