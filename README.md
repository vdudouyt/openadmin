# OpenAdmin

A terminal UI for administering fleets of remote SSH machines: a known-hosts
manager, a real terminal multiplexer, and (stubbed) agentic chat.

Built with [ratatui](https://ratatui.rs). The visual language comes from
`design/` — a Claude Design system shared with the `cfdns` console.

## Build

```sh
cargo build --release
```

Needs `libsqlcipher-dev` at build time, and `ssh`, `ssh-keygen`, `sshfs` and
`fusermount` at runtime.

## Run

```sh
openadmin [--datadir <dir>] [--password <value>]
```

* `--datadir` — data directory, default `~/.openadmin`
* `--password` — unlock without the prompt. **Development only**: the password
  is visible in your shell history and in `/proc/<pid>/cmdline`.

On first run you are asked to create a database; after that, to unlock it.

## The three screens

`Alt+1` / `Alt+2` / `Alt+3`, or `F9` to cycle.

### 1 · Hosts

CRUD over the known hosts, with the mount and proxy state of each.

| Key | |
|---|---|
| `↑ ↓` `Home` `End` | move the cursor |
| `Insert` / `Space` | mark a host; `*` inverts, `Ctrl+A` selects all, `Esc` clears |
| `F2` / `F3` / `F8` | add · edit · delete |
| `F4` | mount / unmount over sshfs |
| `F5` | open a shell — several marked hosts open one grouped tab |
| `F6` | use this host as a SOCKS proxy for the others |
| `F7` | generate an ed25519 key and show its public half |
| `F1` `F10` | help · quit |

The mouse works throughout: hovering a row highlights it and explains it in the
status bar, a click moves the cursor, a double-click opens a shell, and the
`[gen]` cell, function-bar caps and screen tabs are all clickable.

**Mount points follow the host name** (`/net/<name>`) until you type in the
mount field yourself; empty it to hand control back.

### 2 · Shells

Open sessions as tabs. One host gives one full-width pane; several marked hosts
give a single tab named `Group: <first host>` that stacks one pane per host.

**A focused pane takes every key.** `F1`–`F10`, `Tab`, `Esc`, `Esc`+digit, and
every `Ctrl` and `Alt` chord go straight to the terminal — nothing is reserved.
mc reads `Esc`+digit as its own F-key emulation and `Alt` as its menu
shortcuts, so claiming any of them would quietly break it. GNU Screen keeps
`Ctrl+A`, vim keeps a zero-latency `Esc`.

This screen spends **one row** on itself. There is no function bar and no
status line — every key here belongs to the terminal, so neither would earn the
line of your shell it costs. The shell tabs share the header row with the screen
tabs, the brand stepping aside for them, and a tab holding a single pane draws
no title rule at all: its host name is already in its tab. A stacked group keeps
one title per pane, because there they tell the panes apart.

That means **the mouse is how you drive the app while a pane is focused**:

* the `1 Hosts` / `2 Shells` / `3 Chat` tabs on the right of the header switch
  screens — they shed their labels before they ever disappear, and shell tabs
  are never allowed to crowd them out, so a narrow terminal cannot strand you
* a shell tab selects itself on click, and its `×` closes it; `‹` and `›` mark
  tabs scrolled out of view
* to quit, switch to Hosts and use `F10` (or click Quit there)

With **no shell open** there is nothing to be transparent to, so the keyboard
comes back: the F-keys work, and `Esc` returns to Hosts. Closing your last
shell therefore never strands you.

### 3 · Chat

Renders an agent transcript and accepts input, but **no model is called yet**.

## Data

Everything lives under `~/.openadmin`:

```
openadmin.sqlite   SQLCipher-encrypted host database
config.toml        settings (see below)
keys/<host>        ed25519 private keys, 0600
```

`config.toml`:

| | default | |
|---|---|---|
| `term` | `xterm-256color` | `TERM` for spawned sessions |
| `scrollback` | `5000` | lines retained per session |
| `sshfs_options` | `[]` | extra options for every mount |
| `mount_prefix` | `/net` | where derived mount points live |
| `proxy_port` | `10000` | local SOCKS port for the proxy host |
| `model` | `claude-sonnet-4.5` | label shown on the Chat screen |

### Credential storage — read this

Host passwords are stored **in cleartext inside the encrypted database**, which
is the model OpenAdmin inherits from qhostman. The SQLCipher passphrase is
therefore the only thing protecting them: anyone who learns it gets every stored
password. The database and key files are created `0600` and the data directory
`0700`, but that is defence in depth, not the boundary.

Prefer keys (`F7`) over stored passwords where you can.

The schema is qhostman's, so an existing `~/.qhostman/qhostman.sqlite` opens
as-is; a `proxy` column is added on first open, and unlike qhostman, editing a
host no longer discards its key.

## `openadmin-sshpass`

A helper that supplies an SSH secret non-interactively:

```sh
SSHPASS=... openadmin-sshpass -- ssh user@host
```

It points `SSH_ASKPASS` at itself and sets `SSH_ASKPASS_REQUIRE=force`, so ssh
asks it for the password instead of prompting — no PTY screen-scraping, which
keeps the session byte-transparent for full-screen programs. Requires OpenSSH
8.4 or newer. The secret travels in the environment, never in argv.

## Tests

```sh
cargo test
```

Covers the database and its migration, the terminal key/mouse encoders, PTY
spawn and resize (asserting the child really observes `SIGWINCH`), tab and pane
management, and headless renders of every screen and dialog — including
synthetic mouse and hover events, and a 1×1 terminal.
