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

`Alt+1` / `Alt+2` / `Alt+3` jump to a screen; `Alt+←` / `Alt+→` walk
between them, wrapping; `F9` cycles forward from Shells and Chat. On Hosts, `F9`
is Mount.

**Errors open a dialog** on top of whatever is showing, and stay until you
dismiss them — `Esc`, `Enter`, or the Dismiss button. Nothing behind the dialog
reacts to a key or a click while it is up, and dismissing it returns you to what
you were doing, a half-filled form included. A second error while one is showing
is added underneath it rather than replacing it.

### 1 · Hosts

CRUD over the known hosts, with the mount and proxy state of each.

| Key | |
|---|---|
| `↑ ↓` `Home` `End` | move the cursor |
| `Insert` / `Space` | mark a host; `*` inverts, `Ctrl+A` selects all, `Esc` clears |
| `Enter` | open a shell — several marked hosts open one grouped tab |
| `F2` / `a` | add |
| `F4` / `e` | edit |
| `F9` · `m` · `u` | mount / unmount over sshfs · mount · unmount |
| `F6` | use this host as a SOCKS proxy for the others |
| `F8` | delete |
| `F12` / `i` | actions — bulk import |
| `F1` `F10` | help · quit |

Every F-key has a letter twin for keyboards where the F-row is awkward, in either
case; the function bar shows only the F-keys, and `F1` lists the letters. `F9`
toggles — if any target is unmounted it mounts, otherwise it unmounts, and its
label in the function bar says Mount or Unmount for whichever it will do.
`m` and `u` say which way outright, which is what you want on a selection where
some hosts are mounted and some are not. Either way, a host already in the state
asked for is left alone.

**Mounting runs in the background, with a way out.** A dialog shows which host
sshfs is connecting to, with a bar of one segment per host — solid once mounted,
a sweeping block on the one in flight, since sshfs reports nothing while it
connects. `Esc`, `Enter`, `Ctrl+C` or the orange Cancel button stop it: the host
in flight is killed along with the `ssh` it started, and nothing after it is
attempted. Hosts already mounted stay mounted, and the table is re-read from the
mount table afterwards rather than trusted. This matters most on a host that is
not answering, which would otherwise hold the screen for the kernel's TCP
timeout. Unmounting is immediate and needs no dialog.

**SSH keys are made in the edit dialog**, not on this screen: the table's KEY
column only shows whether a host has one (`●`) or not (`○`). `F7` in the dialog —
or the button in its key row — generates an ed25519 key for the host and shows
its public half, or, if the host already has one, shows that. A key generated in
the form is saved with the host when you save the form; closing the key dialog
returns you to the form with your edits still in it.

The mouse works throughout: hovering a row highlights it and explains it in the
status bar, a click moves the cursor, a double-click opens a shell, and the
function-bar caps and screen tabs are all clickable.

**Mount points follow the host name** (`/net/<name>`) until you type in the
mount field yourself; empty it to hand control back.

**Bulk import** (`F12`, then `i`) takes a host list in qhostman's format and
adds every host in it at once. Each host is four lines, with a blank line
between hosts:

```
mydomain-s5000
1.2.3.4
root
123123

mydomain-s5001
2.3.4.5
root
123123
```

Paste it into the box, then `Tab` to review what it will do before anything is
written. Each host becomes an SSH host on port 22 with the usual mount point,
and the review masks the passwords. A name that is already in the table is
skipped rather than changed, so pasting the same list again after adding to it
adds only the new hosts. A block that is not four lines is reported by the line
it starts on, and it stops the whole import: a list that is out of shape in one
place may be out of step from there on, so nothing is written until it is
fixed. The import is one transaction. The hosts it adds come back marked, so
whatever you do next — mount them, open shells on them, or delete them if it
was the wrong list — applies to exactly those.

`F12` rather than `F11`, which GNOME Terminal, Konsole and Windows Terminal
keep for fullscreen; `i` is there for drop-down terminals such as Guake and
Yakuake, which keep `F12` in the same way.

### 2 · Shells

Open sessions as tabs. One host gives one full-width pane; several marked hosts
give a single tab named `Group: <first host>` that stacks one pane per host.

**A focused pane takes every key but one.** `F1`–`F10`, `Tab`, `Esc`,
`Esc`+digit, `Alt`+letter, `Alt`+digit and every `Ctrl` chord go straight to the
terminal. mc reads `Esc`+digit as its own F-key emulation and `Alt` as its menu
shortcuts, so claiming any of them would quietly break it. GNU Screen keeps
`Ctrl+A`, vim keeps a zero-latency `Esc`.

The exception is **`Alt+←` / `Alt+→`**, which walk between screens — the one
keyboard way out of a live terminal. mc, vim and GNU Screen bind neither by
default; if you have bound `\e[1;3D`/`\e[1;3C` to word movement in your shell,
that is what you give up.

This screen spends **one row** on itself. There is no function bar and no
status line — every key here belongs to the terminal, so neither would earn the
line of your shell it costs. The shell tabs share the header row with the screen
tabs — the brand stepping aside whenever there is a shell to name, and taking
the space back when the last one closes — and a tab holding a single pane draws
no title rule at all: its host name is already in its tab. A stacked group keeps
one title per pane, because there they tell the panes apart.

That means **the mouse is how you drive the app while a pane is focused**:

* the `1 Hosts` / `2 Shells` / `3 Chat` tabs on the right of the header switch
  screens — they shed their labels before they ever disappear, and shell tabs
  are never allowed to crowd them out, so a narrow terminal cannot strand you
* a shell tab selects itself on click, and its `×` closes it; `‹` and `›` mark
  tabs scrolled out of view
* to quit, switch to Hosts (`Alt+←`) and use `F10`, or click Quit there

**The wheel scrolls a pane back** through what it has printed — up to
`scrollback` lines each, 5000 by default — with a scrollbar in the pane's
right-hand column showing where you are. Output arriving meanwhile does not move
what you are reading; typing or pasting returns you to the live prompt.
Full-screen programs are left alone: mc, htop and anything else that asked for
the mouse get the wheel themselves, and vim, less and their like draw on the
alternate screen, which has no history to scroll. The scrollbar has a column of
its own, so a pane tells its program it is one column narrower than it looks.

**Closing the last shell takes you back** to the screen you were on before
Shells — Hosts, for the usual Enter on a host and `exit` — whether you closed it
with `F4`, clicked its `×`, or it ended by itself. Only that last close moves
you: a shell ending while you are on another screen changes nothing.

With **no shell open** there is nothing to be transparent to, so the keyboard
comes back: the F-keys work, and `Esc` returns to Hosts. An empty Shells screen
you reached on purpose, with `Alt+2`, stays put.

### 3 · Chat

An agent that helps you diagnose and fix the fleet. It speaks the **OpenAI Chat
Completions** API, which is also what vLLM, Ollama, llama.cpp, OpenRouter and
Azure speak — so you can point it at a model on your own network rather than
send transcripts about your infrastructure to a third party.

**Configuration is by hand** — there is no setup dialog yet. Run OpenAdmin
once so it writes `~/.openadmin/config.toml`, then fill in the `[agent]`
section:

```toml
[agent]
api_key = "sk-..."                          # or set OPENAI_API_KEY instead
base_url = "https://api.openai.com/v1"      # or http://localhost:11434/v1, ...
model = "gpt-5"                             # empty by default; nothing is guessed
reasoning_effort = ""                       # "none" turns thinking off on Ollama
```

**Turning thinking off.** `reasoning_effort` is sent with each request when it is
not empty, and omitted entirely when it is — so leaving it alone keeps whatever
your backend did before. Ollama's OpenAI-compatible endpoint accepts `none`,
`low`, `medium`, `high` and `max`, and **`none` disables thinking**; OpenAI's
reasoning models take `minimal` through `high` and have no `none`. The value goes
through verbatim, because which words a backend accepts is the backend's business.

Worth setting on a local model. Thinking is generated before the answer is, so it
is wall-clock time you spend watching a spinner — and some Ollama models return
their thinking in a `reasoning` field and leave `content` empty, which arrives
here as a turn that says nothing at all. Note that `think: false`, Ollama's own
parameter, is **not** accepted on the `/v1/chat/completions` path; this is why the
setting is spelled the OpenAI way.

`OPENAI_API_KEY` overrides the file and is the better choice for anything
shared or backed up, since the file holds a live credential in plaintext beside
an encrypted database. Every setting is written to the file at startup —
including ones added since it was created — so the knobs are discoverable
without reading the source.

**The model may look, but it may not touch.** Reading is unattended so the agent
can find out what is actually true before it suggests anything, and it reads
through one tool per question rather than one tool that takes a command line:

| | |
|---|---|
| `readonly_*` | nineteen read-only questions — logs, services, network, disks, files, processes |
| `list_hosts` | the known SSH machines, by name — never an address, login or password |
| `list_artifacts` | files staged under `~/.openadmin/artifacts/`, subdirectories included; a plan uploads one to `/tmp/openadmin/` |
| `list_manuals` · `fetch_manual` | what the operator has written about *this* fleet |
| `propose_plan` | proposes changes; **executes nothing** |
| `create_host` · `edit_host` | write the host database, and only on request |

**There is no shell, and no field that could hold one.** `readonly_logs` takes
`unit`, `lines`, `priority`, `since`; `readonly_service` takes an `action` from a
closed list and a list of `units`. Every option a command accepts is a named,
typed field, and the argv is assembled here from the fields — so `|`, `2>&1` and
`|| fallback` are not refused, they are unrepresentable. Each schema is closed
(`additionalProperties: false`), so a field that does not exist is named as such
rather than ignored.

This matters most on small local models. A single tool taking `command` and
`args: [string]` puts a token stream in front of a model whose prior says a
command line is a thing you type a pipe into; it writes one, spends a round trip
being refused, and leaves the refusal in its context to imitate next turn. A
field called `lines` with type `integer` has no such failure mode, and on any
backend that compiles tool schemas into a decoding grammar — llama.cpp with
`--jinja`, vLLM's guided decoding, Ollama's structured output — the malformed
call cannot be emitted at all.

The whitelist is still the boundary, and still a whitelist of commands *and* of
their options: `src/agent/probe.rs` decides the shape of a call, and
`readonly::validate` judges the argv that comes out of it, exactly as it did
when the model wrote that argv itself. So the tool layer can only ever be
narrower than the boundary, and a test fails if a probe renders something the
whitelist would refuse. The command list is in `config.toml`; narrowing it
removes tools and removes options from the `what` enums, and widening it past
the built-in rules is not possible.

What comes back is the output capped at `output_cap_bytes` with the middle
elided, stderr labelled, and the exit status — so nothing needs to arrange for
any of that. A command still running after `command_timeout_secs` is killed.

### Manuals

The agent knows how to administer machines in general and nothing about *your*
fleet — that the standby is promoted with a particular script, that a config is
rolled in a particular order, who gets woken at 3am. Write that down once instead
of typing it into the chat every session:

```
~/.openadmin/manuals/
  db-failover.md        # Promoting the standby
  nginx-deploy.md       # Rolling a config change
  linux/tuning.md       # Sysctls we set, and why
```

One file per subject, scanned recursively, and **a manual's first line is its
description** — a markdown `# Heading` or a front-matter `title:` both work. The
filenames and descriptions are in the agent's instructions from the start, so it
never has to go looking to find out that your guidance exists; `fetch_manual`
reads one in full when it is relevant, and that costs no SSH connection because
the file is local. `list_manuals` is for one you add mid-conversation.

A manual is **you speaking**, and the agent is told so: where one covers the task,
its way wins over whatever the agent would otherwise have done, and it reads the
relevant one before proposing a plan rather than after you reject one. That is
the opposite of how a command's output is treated, which is data from a machine
being diagnosed.

Long manuals are truncated at `output_cap_bytes` — keeping the **beginning** and
saying how much was dropped, never eliding the middle the way command output is,
because a procedure that lost steps 4 to 7 still reads as complete. Binaries are
neither listed nor fetched.

**Staged files keep their shape.** `list_artifacts` scans
`~/.openadmin/artifacts/` recursively, so stage files the way they are organised
— `nginx/site.conf` beside `postgres/pg_hba.conf` — rather than flattening
everything into one directory to make them visible. A name is a path relative to
that directory, it keeps that path under `/tmp/openadmin/` on the far side (so
two files called `site.conf` stay two files), and a scriptlet in the same plan
can name it by the path it already knows — `/tmp/openadmin/nginx/site.conf`.
One fixed directory rather than one per plan, because the model writes that path
into a script *before* the plan has run, and a path with a plan number in it is
one it cannot know yet; the cost is that files outlive the plan that sent them,
so an upload the operator unchecked can leave a later script reading an older
file of the same name. A very large tree is listed
up to a limit and says when it stopped; anything under the directory can still be
uploaded by name whether it was listed or not. Nothing escapes: a name with `..`
in it is refused, and so is a path that leads out through a symlink, because the
check is on the canonicalized path.

Changing anything goes through a **plan** — scripts and artifact uploads, with
the hosts for each — which you review in a dialog with a checkbox per step and
per host. The script is shown in full, never elided: a plan you cannot read end
to end is one you cannot judge. Confirm runs exactly what is still checked.

**Nothing halts on failure.** Every checked step runs on every checked host,
and the report — each pair with its exit status — goes back to the model, which
is asked to propose a follow-up plan scoped to just the hosts that failed.
Halting early would leave the fleet in a state nobody asked for and nobody can
see.

The agent *cannot* execute anything itself, and this is structural rather than a
rule it is asked to follow. What the model builds is a `Plan`; what the executor
takes is a `ConfirmedPlan`, whose constructor is private to the dialog's module.
A model ignoring every word of its system prompt still has no function it can
reach that accepts what it can make.

`Ctrl+C` cancels a turn in flight. `PgUp`/`PgDn` scroll the transcript.

## Data

Everything lives under `~/.openadmin`:

```
openadmin.sqlite   SQLCipher-encrypted host database
config.toml        settings (see below)
keys/<host>        ed25519 private keys, 0600
artifacts/         files a plan may upload
manuals/           what you have written for the agent
```

`artifacts/` and `manuals/` are created empty on every start, so they are there
to put something in.

`config.toml`:

| | default | |
|---|---|---|
| `term` | `xterm-256color` | `TERM` for spawned sessions |
| `scrollback` | `5000` | lines retained per pane — how far back the wheel scrolls |
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

Prefer keys (`F7` in the edit dialog) over stored passwords where you can.

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
