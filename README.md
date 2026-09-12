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

**No F-key is reserved here.** Every key — `F1`–`F10`, `Tab`, `Ctrl+A`, and the
mouse — goes straight to the terminal, so `mc`, GNU Screen and vim keep their
full keyboard. OpenAdmin is reached with an mc-style `Esc` prefix instead:

| Chord | |
|---|---|
| `Esc` `1` | help |
| `Esc` `2` | focus the next pane in a group |
| `Esc` `3` | next tab |
| `Esc` `4` | close the tab |
| `Esc` `5` | new shell (jumps to Hosts) |
| `Esc` `9` | cycle screen |
| `Esc` `0` | quit OpenAdmin |

Typed quickly, `Esc`+digit reaches the app as `Alt`+digit; both spellings work,
so the chord never depends on how fast you type. A lone `Esc` is held for
`escape_time_ms` and then delivered to the terminal, so vim behaves normally —
lower that value if it feels sluggish.

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
| `escape_time_ms` | `250` | how long a lone `Esc` is held before reaching the terminal |
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
