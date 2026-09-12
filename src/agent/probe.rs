//! One tool per read-only question, with typed fields instead of an argv.
//!
//! This replaces a single `run_readonly` that took `command` plus
//! `args: [string]`. The command name was already an `enum`, but `args` was a
//! token stream, and a token stream is what invites a shell: weak local models
//! filled it with `|`, `2>&1` and `|| fallback`, spent a round trip being
//! refused, and left the refusal in the context to be imitated next turn.
//!
//! Here every option a command accepts is a named field with a type, so there
//! is nowhere a pipe fits. `readonly_logs` has `unit`, `lines` and `priority`;
//! it has no field that holds a fragment of shell. That is the difference
//! between a boundary the model is asked to respect and one it cannot express —
//! the same argument as `ConfirmedPlan`'s private constructor, one layer up.
//!
//! Four properties keep it honest:
//!
//! 1. **The argv is built here, never by the model.** A field renders to the
//!    exact option token this table names. Options that exist only to make
//!    output machine-readable — `--no-pager`, `--full`, `-T` — are in `always`
//!    and never offered, because a model that has to remember `--no-pager`
//!    sometimes forgets and gets a pager's escape sequences back.
//! 2. **`readonly::validate` still judges the result.** The whitelist is
//!    unchanged and remains the final gate, so this layer can only ever be
//!    narrower than the boundary. A probe that rendered something the whitelist
//!    refuses is a bug that `every_probe_renders_what_validate_accepts` fails
//!    on, rather than something an operator discovers.
//! 3. **The operator's `readonly_commands` still decides.** A probe whose
//!    program is not configured is not offered; a `what` variant whose program
//!    is not configured disappears from that enum. Narrowing the config narrows
//!    the tool list, and `every_permitted_command_is_reachable` stops a
//!    configured command from being unreachable.
//! 4. **Sloppy JSON is absorbed rather than refused.** `"50"` for an integer and
//!    a bare string for a one-element array are what small models send, and
//!    here they are unambiguous — there is no argv to split, so accepting them
//!    cannot change what runs. `hosts::lenient_port` already does this one layer
//!    over.

use crate::config::AgentConfig;
use anyhow::{Result, bail};

/// How a probe decides which program to run.
enum Cmd {
    /// Always this program, with these options applied to every call.
    Fixed {
        program: &'static str,
        always: &'static [&'static str],
    },
    /// A required closed-set field picks the program: `(field value, program,
    /// options)`. This is how several spellings of one question — `df` and `du`,
    /// `ip` and `ss` — become one tool with no free text in it.
    ByWhat {
        field: &'static str,
        about: &'static str,
        /// `None` when the field is required; otherwise the value used when the
        /// model leaves it out.
        default: Option<&'static str>,
        variants: &'static [(&'static str, &'static str, &'static [&'static str])],
    },
}

/// One typed field, and the argv it renders to.
enum Param {
    /// `true` emits `flag`; `false` and absent emit nothing.
    Flag {
        name: &'static str,
        flag: &'static str,
        about: &'static str,
    },
    /// Emits `opt` then the number.
    Int {
        name: &'static str,
        opt: &'static str,
        about: &'static str,
        min: i64,
        max: i64,
    },
    /// Emits `opt` then the string.
    Text {
        name: &'static str,
        opt: &'static str,
        about: &'static str,
    },
    /// Emits `opt` then the token paired with the chosen label. The label is
    /// what the model picks and the token is what the program wants, so
    /// `is_active` can read as a word while `is-active` goes on the command line.
    Choice {
        name: &'static str,
        opt: &'static str,
        about: &'static str,
        values: &'static [(&'static str, &'static str)],
        /// A choice that renders to a subcommand — `systemctl status`, `getent
        /// passwd` — is not optional: the program has nothing to do without it,
        /// and `every_probe_renders_what_validate_accepts` catches the omission.
        required: bool,
    },
    /// One bare operand, in this position.
    Word {
        name: &'static str,
        about: &'static str,
        required: bool,
    },
    /// Zero or more bare operands, in this position.
    Operands {
        name: &'static str,
        about: &'static str,
        required: bool,
    },
}

impl Param {
    fn name(&self) -> &'static str {
        match self {
            Param::Flag { name, .. }
            | Param::Int { name, .. }
            | Param::Text { name, .. }
            | Param::Choice { name, .. }
            | Param::Word { name, .. }
            | Param::Operands { name, .. } => name,
        }
    }

    fn about(&self) -> &'static str {
        match self {
            Param::Flag { about, .. }
            | Param::Int { about, .. }
            | Param::Text { about, .. }
            | Param::Choice { about, .. }
            | Param::Word { about, .. }
            | Param::Operands { about, .. } => about,
        }
    }

    fn required(&self) -> bool {
        matches!(
            self,
            Param::Word { required: true, .. }
                | Param::Operands { required: true, .. }
                | Param::Choice { required: true, .. }
        )
    }

    /// Operands need `--` in front of them, and options do not.
    fn is_operand(&self) -> bool {
        matches!(self, Param::Word { .. } | Param::Operands { .. })
    }

    fn schema(&self) -> serde_json::Value {
        match self {
            Param::Flag { about, .. } => {
                serde_json::json!({"type": "boolean", "description": about})
            }
            Param::Int {
                about, min, max, ..
            } => serde_json::json!({
                "type": "integer", "minimum": min, "maximum": max, "description": about
            }),
            Param::Text { about, .. } => {
                serde_json::json!({"type": "string", "description": about})
            }
            Param::Choice { about, values, .. } => serde_json::json!({
                "type": "string",
                "enum": values.iter().map(|(l, _)| *l).collect::<Vec<_>>(),
                "description": about
            }),
            Param::Word { about, .. } => {
                serde_json::json!({"type": "string", "description": about})
            }
            Param::Operands { about, .. } => serde_json::json!({
                "type": "array", "items": {"type": "string"}, "description": about
            }),
        }
    }
}

/// One read-only tool.
pub struct Probe {
    /// The tool name the model calls.
    pub name: &'static str,
    /// One or two lines for the schema: what question this answers.
    about: &'static str,
    cmd: Cmd,
    params: &'static [Param],
    /// Emit `--` before the first operand, so a file whose name begins with `-`
    /// is read as a file. Off for programs that do not take it — `find`, `ip`
    /// and `lsof` parse positionally.
    separator: bool,
}

/// Shorthands, so the table below reads as data rather than as constructors.
const fn flag(name: &'static str, f: &'static str, about: &'static str) -> Param {
    Param::Flag {
        name,
        flag: f,
        about,
    }
}
const fn int(
    name: &'static str,
    opt: &'static str,
    min: i64,
    max: i64,
    about: &'static str,
) -> Param {
    Param::Int {
        name,
        opt,
        about,
        min,
        max,
    }
}
const fn text(name: &'static str, opt: &'static str, about: &'static str) -> Param {
    Param::Text { name, opt, about }
}
const fn choice(
    name: &'static str,
    opt: &'static str,
    values: &'static [(&'static str, &'static str)],
    about: &'static str,
) -> Param {
    Param::Choice {
        name,
        opt,
        about,
        values,
        required: false,
    }
}

/// A choice the program cannot run without: it renders to the subcommand.
const fn subcommand(
    name: &'static str,
    values: &'static [(&'static str, &'static str)],
    about: &'static str,
) -> Param {
    Param::Choice {
        name,
        opt: "",
        about,
        values,
        required: true,
    }
}
const fn word(name: &'static str, required: bool, about: &'static str) -> Param {
    Param::Word {
        name,
        about,
        required,
    }
}
const fn operands(name: &'static str, required: bool, about: &'static str) -> Param {
    Param::Operands {
        name,
        about,
        required,
    }
}
const fn fixed(program: &'static str, always: &'static [&'static str]) -> Cmd {
    Cmd::Fixed { program, always }
}

const PRIORITIES: &[(&str, &str)] = &[
    ("emerg", "emerg"),
    ("alert", "alert"),
    ("crit", "crit"),
    ("err", "err"),
    ("warning", "warning"),
    ("notice", "notice"),
    ("info", "info"),
    ("debug", "debug"),
];

/// Every read-only tool, and the argv each of its fields renders to.
///
/// Editing this table is a trust decision in the same way `readonly::rule_for`
/// is, but a smaller one: nothing here can widen that table, because everything
/// it renders is checked against it.
static PROBES: &[Probe] = &[
    Probe {
        name: "readonly_sysinfo",
        about: "One fact about the machine itself: kernel, hostname, uptime, memory, \
                logged-in identity, date, CPU, PCI devices, loaded modules.",
        cmd: Cmd::ByWhat {
            field: "what",
            about: "Which fact to read.",
            default: None,
            variants: &[
                ("kernel", "uname", &["-a"]),
                ("hostname", "hostname", &[]),
                ("uptime", "uptime", &[]),
                ("memory", "free", &["-h"]),
                ("identity", "id", &[]),
                ("login_name", "whoami", &[]),
                ("date", "date", &[]),
                ("cpu_count", "nproc", &[]),
                ("cpu_model", "lscpu", &[]),
                ("pci_devices", "lspci", &[]),
                ("kernel_modules", "lsmod", &[]),
                ("memory_and_io_stats", "vmstat", &[]),
            ],
        },
        params: &[],
        separator: false,
    },
    Probe {
        name: "readonly_list_dir",
        about: "List a directory.",
        cmd: fixed("ls", &[]),
        params: &[
            flag("long", "-l", "Sizes, owners, modes and times."),
            flag("all", "-a", "Include dotfiles."),
            flag("human", "-h", "Sizes as K/M/G. Needs long."),
            flag("newest_first", "-t", "Sort by modification time."),
            flag("reverse", "-r", "Reverse the sort."),
            flag(
                "directory_itself",
                "-d",
                "The directory entry, not its contents.",
            ),
            operands(
                "paths",
                false,
                "Directories or files. Omit for the login directory.",
            ),
        ],
        separator: true,
    },
    Probe {
        name: "readonly_read_file",
        about: "Read whole files. Several paths in one call rather than one call each.",
        cmd: fixed("cat", &[]),
        params: &[operands("paths", true, "The files to read.")],
        separator: true,
    },
    Probe {
        name: "readonly_read_lines",
        about: "Read the beginning or the end of files, when the whole file is more than \
                the question needs.",
        cmd: Cmd::ByWhat {
            field: "from",
            about: "Which end of the file.",
            default: Some("end"),
            variants: &[("start", "head", &[]), ("end", "tail", &[])],
        },
        params: &[
            int("lines", "-n", 1, 10_000, "How many lines. Default 10."),
            operands("paths", true, "The files to read."),
        ],
        separator: true,
    },
    Probe {
        name: "readonly_search_files",
        about: "Search file contents for a pattern. This is how to find something in a \
                file — read the file whole only when you need the rest of it too.",
        cmd: fixed("grep", &[]),
        params: &[
            flag("ignore_case", "-i", "Case-insensitive."),
            flag(
                "line_numbers",
                "-n",
                "Prefix each match with its line number.",
            ),
            flag(
                "count_only",
                "-c",
                "Count matches instead of printing them.",
            ),
            flag(
                "files_only",
                "-l",
                "Name the matching files and nothing else.",
            ),
            flag("invert", "-v", "Lines that do NOT match."),
            flag(
                "regex",
                "-E",
                "Treat the pattern as an extended regular expression.",
            ),
            flag(
                "literal",
                "-F",
                "Treat the pattern as plain text, not a regex.",
            ),
            flag("recursive", "-r", "Search directories recursively."),
            int(
                "context",
                "-C",
                1,
                20,
                "Also print this many lines either side.",
            ),
            word("pattern", true, "What to search for."),
            operands("paths", true, "Files or directories to search."),
        ],
        separator: true,
    },
    Probe {
        name: "readonly_find_files",
        about: "Find files by name, type, size or age. Do not use it to browse a source \
                tree.",
        cmd: fixed("find", &[]),
        params: &[
            word("path", true, "Where to start."),
            text("name", "-name", "Shell-style name pattern, e.g. *.log."),
            choice(
                "type",
                "-type",
                &[
                    ("file", "f"),
                    ("directory", "d"),
                    ("symlink", "l"),
                    ("socket", "s"),
                    ("block_device", "b"),
                    ("character_device", "c"),
                    ("fifo", "p"),
                ],
                "Only entries of this kind.",
            ),
            int("max_depth", "-maxdepth", 1, 20, "How deep to descend."),
            int(
                "min_depth",
                "-mindepth",
                0,
                20,
                "Skip entries shallower than this.",
            ),
            text(
                "size",
                "-size",
                "find's size test, e.g. +100M for larger than 100 MB.",
            ),
            text(
                "modified_days",
                "-mtime",
                "find's mtime test: -1 is within a day, +7 is older than a week.",
            ),
            text("user", "-user", "Only files owned by this user."),
            flag("empty", "-empty", "Only empty files and directories."),
            flag("same_filesystem", "-xdev", "Do not cross mount points."),
        ],
        separator: false,
    },
    Probe {
        name: "readonly_inspect_path",
        about: "What a path is, rather than what is in it: metadata, file type, where a \
                symlink points, its resolved path, how many lines it has.",
        cmd: Cmd::ByWhat {
            field: "what",
            about: "Which property to read.",
            default: None,
            variants: &[
                ("metadata", "stat", &[]),
                ("file_type", "file", &[]),
                ("symlink_target", "readlink", &[]),
                ("resolved_path", "realpath", &[]),
                ("line_count", "wc", &["-l"]),
            ],
        },
        params: &[operands("paths", true, "The paths to inspect.")],
        separator: true,
    },
    Probe {
        name: "readonly_disk_space",
        about: "Free space per filesystem, or how much space paths are using.",
        cmd: Cmd::ByWhat {
            field: "what",
            about: "free: space per filesystem. used_by: the size of the paths given.",
            default: Some("free"),
            variants: &[("free", "df", &["-h"]), ("used_by", "du", &["-s", "-h"])],
        },
        params: &[operands(
            "paths",
            false,
            "For used_by, the paths to measure. For free, omit for every filesystem.",
        )],
        separator: true,
    },
    Probe {
        name: "readonly_storage",
        about: "Block devices, their filesystem identifiers, or what is mounted where.",
        cmd: Cmd::ByWhat {
            field: "what",
            about: "Which view of storage.",
            default: Some("devices"),
            variants: &[
                (
                    "devices",
                    "lsblk",
                    &["-o", "NAME,SIZE,TYPE,FSTYPE,MOUNTPOINTS"],
                ),
                ("filesystem_ids", "blkid", &[]),
                ("mounts", "findmnt", &[]),
            ],
        },
        params: &[],
        separator: false,
    },
    Probe {
        name: "readonly_processes",
        about: "Every running process with its command line, CPU and memory.",
        cmd: fixed("ps", &["aux"]),
        params: &[],
        separator: false,
    },
    Probe {
        name: "readonly_open_files",
        about: "Which processes hold a file, a directory or a network socket open.",
        cmd: fixed("lsof", &["-n", "-P"]),
        params: &[
            flag("network_only", "-i", "Only network connections."),
            text("user", "-u", "Only files held by this user."),
            int(
                "pid",
                "-p",
                1,
                4_194_304,
                "Only files held by this process.",
            ),
            word("path", false, "Only this file or directory."),
        ],
        separator: false,
    },
    Probe {
        name: "readonly_network",
        about: "Addresses, routes, links, neighbours, or which sockets are listening and \
                connected.",
        cmd: Cmd::ByWhat {
            field: "what",
            about: "Which view of the network.",
            default: None,
            variants: &[
                ("addresses", "ip", &["-br", "addr", "show"]),
                ("routes", "ip", &["route", "show"]),
                ("links", "ip", &["-br", "link", "show"]),
                ("neighbours", "ip", &["neigh", "show"]),
                ("listening_tcp", "ss", &["-tlnp"]),
                ("listening_udp", "ss", &["-ulnp"]),
                ("tcp_connections", "ss", &["-tanp"]),
                ("socket_summary", "ss", &["-s"]),
            ],
        },
        params: &[],
        separator: false,
    },
    Probe {
        name: "readonly_service",
        about: "The state of systemd units. `status` on several units in one call beats \
                one call each.",
        cmd: fixed("systemctl", &["--no-pager", "--full"]),
        params: &[
            subcommand(
                "action",
                &[
                    ("status", "status"),
                    ("show", "show"),
                    ("cat", "cat"),
                    ("is_active", "is-active"),
                    ("is_enabled", "is-enabled"),
                    ("is_failed", "is-failed"),
                    ("is_system_running", "is-system-running"),
                    ("list_units", "list-units"),
                    ("list_unit_files", "list-unit-files"),
                    ("list_timers", "list-timers"),
                    ("list_sockets", "list-sockets"),
                    ("list_dependencies", "list-dependencies"),
                    ("get_default", "get-default"),
                    ("show_environment", "show-environment"),
                ],
                "What to ask about the units. Reading only; there is no start or restart \
                 here — changing a service is a plan.",
            ),
            choice(
                "unit_type",
                "--type",
                &[
                    ("service", "service"),
                    ("socket", "socket"),
                    ("timer", "timer"),
                    ("target", "target"),
                    ("mount", "mount"),
                    ("path", "path"),
                ],
                "For the list_ actions: only units of this type.",
            ),
            choice(
                "state",
                "--state",
                &[
                    ("active", "active"),
                    ("inactive", "inactive"),
                    ("failed", "failed"),
                    ("enabled", "enabled"),
                    ("disabled", "disabled"),
                ],
                "For the list_ actions: only units in this state.",
            ),
            operands(
                "units",
                false,
                "Unit names, e.g. nginx or nginx.service. Omit for the list_ actions.",
            ),
        ],
        separator: false,
    },
    Probe {
        name: "readonly_logs",
        about: "The journal. Narrow it with unit, priority and since rather than reading \
                it all and looking.",
        cmd: fixed("journalctl", &["--no-pager"]),
        params: &[
            text("unit", "-u", "Only this unit's messages."),
            int(
                "lines",
                "-n",
                1,
                5_000,
                "How many of the most recent lines.",
            ),
            text(
                "since",
                "--since",
                "Start here: \"2026-09-12 14:00\", \"yesterday\", \"-2h\".",
            ),
            text("until", "--until", "Stop here, same spellings as since."),
            choice("priority", "-p", PRIORITIES, "This priority and worse."),
            int("boot", "-b", -20, 0, "0 is this boot, -1 the previous one."),
            text("matching", "-g", "Only messages matching this pattern."),
            text("identifier", "-t", "Only this syslog identifier."),
            flag("newest_first", "-r", "Most recent line first."),
            flag("kernel_only", "-k", "Only kernel messages."),
            flag(
                "explain",
                "-x",
                "Add catalog explanations where systemd has them.",
            ),
            flag("utc", "--utc", "Timestamps in UTC."),
        ],
        separator: false,
    },
    Probe {
        name: "readonly_kernel_log",
        about: "The kernel ring buffer, for hardware and driver messages that predate the \
                journal or are not in it.",
        cmd: fixed("dmesg", &["-T", "-P"]),
        params: &[
            choice("level", "-l", PRIORITIES, "Only this level."),
            flag("kernel_only", "-k", "Only kernel messages."),
            flag("userspace_only", "-u", "Only userspace messages."),
            flag("decode", "-x", "Decode facility and level into words."),
        ],
        separator: false,
    },
    Probe {
        name: "readonly_kernel_settings",
        about: "Kernel parameters. Reading only: setting one is a plan.",
        cmd: fixed("sysctl", &[]),
        params: &[
            flag("all", "-a", "Every parameter."),
            operands(
                "names",
                false,
                "Parameter names, e.g. net.ipv4.ip_forward. A name may not contain `=`; \
                 assigning one is a write.",
            ),
        ],
        separator: false,
    },
    Probe {
        name: "readonly_which_program",
        about: "Where a program is on PATH, and whether it is installed at all.",
        cmd: fixed("which", &[]),
        params: &[operands("programs", true, "Program names.")],
        separator: false,
    },
    Probe {
        name: "readonly_directory_lookup",
        about: "Look a user, group, host or service up the way the system resolves it — \
                including LDAP and NSS, which reading /etc/passwd misses.",
        cmd: fixed("getent", &[]),
        params: &[
            subcommand(
                "database",
                &[
                    ("passwd", "passwd"),
                    ("group", "group"),
                    ("hosts", "hosts"),
                    ("services", "services"),
                    ("protocols", "protocols"),
                    ("networks", "networks"),
                ],
                "Which database to query.",
            ),
            operands(
                "keys",
                false,
                "Names or ids to look up. Omit to list the database.",
            ),
        ],
        separator: false,
    },
];

/// The probe a tool name refers to.
pub fn find(name: &str) -> Option<&'static Probe> {
    PROBES.iter().find(|p| p.name == name)
}

/// Is this one of ours? Used to route a tool call without a table of names.
pub fn is_probe(name: &str) -> bool {
    find(name).is_some()
}

/// Every program a probe can run, whether or not the operator configured it.
///
/// Only the reachability tests need this, and they are the reason it exists: a
/// probe naming a program the whitelist does not know, or a configured command
/// no probe reaches, is a drift between the two layers.
#[cfg(test)]
fn programs(p: &Probe) -> Vec<&'static str> {
    match &p.cmd {
        Cmd::Fixed { program, .. } => vec![program],
        Cmd::ByWhat { variants, .. } => variants.iter().map(|(_, prog, _)| *prog).collect(),
    }
}

/// The `what`-style variants left after the operator's narrowing.
fn live_variants(
    p: &Probe,
    allowed: &[String],
) -> Vec<(&'static str, &'static str, &'static [&'static str])> {
    match &p.cmd {
        Cmd::Fixed { .. } => Vec::new(),
        Cmd::ByWhat { variants, .. } => variants
            .iter()
            .filter(|(_, prog, _)| allowed.iter().any(|a| a == prog))
            .copied()
            .collect(),
    }
}

/// Is this probe offered at all, given the configured commands?
fn offered(p: &Probe, allowed: &[String]) -> bool {
    match &p.cmd {
        Cmd::Fixed { program, .. } => allowed.iter().any(|a| a == program),
        Cmd::ByWhat { .. } => !live_variants(p, allowed).is_empty(),
    }
}

/// The JSON Schema for one probe's arguments.
///
/// `additionalProperties: false` is deliberate: with every field named and
/// typed, a key that is not here is a mistake, and saying so in the schema is
/// cheaper than a refusal. It also keeps a constrained decoder from inventing
/// one.
fn schema(p: &Probe, allowed: &[String]) -> serde_json::Value {
    let mut props = serde_json::Map::new();
    let mut required: Vec<&str> = vec!["host"];
    props.insert(
        "host".to_string(),
        serde_json::json!({"type": "string", "description": "Host name from list_hosts."}),
    );

    if let Cmd::ByWhat {
        field,
        about,
        default,
        ..
    } = &p.cmd
    {
        let labels: Vec<&str> = live_variants(p, allowed)
            .iter()
            .map(|(l, _, _)| *l)
            .collect();
        props.insert(
            (*field).to_string(),
            serde_json::json!({"type": "string", "enum": labels, "description": about}),
        );
        if default.is_none() {
            required.push(field);
        }
    }

    for prm in p.params {
        props.insert(prm.name().to_string(), prm.schema());
        if prm.required() {
            required.push(prm.name());
        }
    }

    serde_json::json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false,
    })
}

/// The tool definitions for every probe the configuration leaves standing.
pub fn definitions(cfg: &AgentConfig) -> Vec<super::proto::ToolDef> {
    PROBES
        .iter()
        .filter(|p| offered(p, &cfg.readonly_commands))
        .map(|p| {
            super::proto::ToolDef::function(
                p.name,
                p.about.to_string(),
                schema(p, &cfg.readonly_commands),
            )
        })
        .collect()
}

// ---- reading what the model sent ----------------------------------------
//
// Lenient in the two ways small models are sloppy, and in no others. `"50"` for
// an integer and `"/etc/hosts"` for a one-element array are unambiguous here
// precisely because there is no argv to split: one string is one operand, never
// two. The old `args: [string]` could not afford the same leniency, because
// splitting a string there guessed at where the arguments were.

fn as_int(v: &serde_json::Value, name: &str) -> Result<i64> {
    if let Some(i) = v.as_i64() {
        return Ok(i);
    }
    if let Some(s) = v.as_str()
        && let Ok(i) = s.trim().parse::<i64>()
    {
        return Ok(i);
    }
    bail!("{name} must be a whole number; got {v}")
}

fn as_bool(v: &serde_json::Value, name: &str) -> Result<bool> {
    match v {
        serde_json::Value::Bool(b) => Ok(*b),
        serde_json::Value::String(s) if s.eq_ignore_ascii_case("true") => Ok(true),
        serde_json::Value::String(s) if s.eq_ignore_ascii_case("false") => Ok(false),
        _ => bail!("{name} must be true or false; got {v}"),
    }
}

fn as_text(v: &serde_json::Value, name: &str) -> Result<String> {
    match v {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        _ => bail!("{name} must be a string; got {v}"),
    }
}

fn as_list(v: &serde_json::Value, name: &str) -> Result<Vec<String>> {
    match v {
        serde_json::Value::String(s) => Ok(vec![s.clone()]),
        serde_json::Value::Array(items) => items
            .iter()
            .map(|i| as_text(i, name))
            .collect::<Result<Vec<_>>>(),
        _ => bail!("{name} must be a list of strings; got {v}"),
    }
}

/// Build the program and argv for one call.
///
/// Every refusal here names the field, because the model's next move is to fix
/// that field and nothing else.
pub fn render(
    p: &Probe,
    args: &serde_json::Value,
    allowed: &[String],
) -> Result<(&'static str, Vec<String>)> {
    let obj = args
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("the arguments must be a JSON object"))?;

    // A key that is not a field is usually the model reaching for an option this
    // tool does not offer, so name what it does offer.
    let known: Vec<&str> = std::iter::once("host")
        .chain(match &p.cmd {
            Cmd::ByWhat { field, .. } => Some(*field),
            Cmd::Fixed { .. } => None,
        })
        .chain(p.params.iter().map(|x| x.name()))
        .collect();
    if let Some(k) = obj.keys().find(|k| !known.contains(&k.as_str())) {
        bail!(
            "{} has no field {k:?}. Its fields are: {}.",
            p.name,
            known.join(", ")
        );
    }

    let (program, mut out) = match &p.cmd {
        Cmd::Fixed { program, always } => (
            *program,
            always.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        ),
        Cmd::ByWhat { field, default, .. } => {
            let live = live_variants(p, allowed);
            let picked = match obj.get(*field) {
                Some(v) => as_text(v, field)?,
                None => match default {
                    Some(d) => (*d).to_string(),
                    None => bail!(
                        "{} needs {field}, one of: {}.",
                        p.name,
                        live.iter()
                            .map(|(l, _, _)| *l)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                },
            };
            match live.iter().find(|(l, _, _)| *l == picked) {
                Some((_, prog, always)) => (
                    *prog,
                    always.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                ),
                None => bail!(
                    "{field}={picked:?} is not one of: {}.",
                    live.iter()
                        .map(|(l, _, _)| *l)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        }
    };

    // The operator may have narrowed the list since the schema was built.
    if !allowed.iter().any(|a| a == program) {
        bail!("{program} is not among the commands this installation permits.");
    }

    let mut separated = false;
    for prm in p.params {
        let present = obj.get(prm.name());
        if prm.is_operand() && present.is_some() && p.separator && !separated {
            out.push("--".to_string());
            separated = true;
        }
        match (prm, present) {
            (_, None) if prm.required() => {
                bail!("{} needs {}: {}", p.name, prm.name(), prm.about())
            }
            (_, None) => continue,
            (Param::Flag { flag, name, .. }, Some(v)) => {
                if as_bool(v, name)? {
                    out.push((*flag).to_string());
                }
            }
            (
                Param::Int {
                    name,
                    opt,
                    min,
                    max,
                    ..
                },
                Some(v),
            ) => {
                let n = as_int(v, name)?;
                if n < *min || n > *max {
                    bail!("{name} must be between {min} and {max}; got {n}");
                }
                out.push((*opt).to_string());
                out.push(n.to_string());
            }
            (Param::Text { name, opt, .. }, Some(v)) => {
                let s = as_text(v, name)?;
                if s.trim().is_empty() {
                    continue;
                }
                if !opt.is_empty() {
                    out.push((*opt).to_string());
                }
                out.push(s);
            }
            (
                Param::Choice {
                    name, opt, values, ..
                },
                Some(v),
            ) => {
                let picked = as_text(v, name)?;
                let Some((_, token)) = values.iter().find(|(l, _)| *l == picked) else {
                    bail!(
                        "{name}={picked:?} is not one of: {}.",
                        values
                            .iter()
                            .map(|(l, _)| *l)
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                };
                if !opt.is_empty() {
                    out.push((*opt).to_string());
                }
                out.push((*token).to_string());
            }
            (Param::Word { name, .. }, Some(v)) => {
                let s = as_text(v, name)?;
                if s.trim().is_empty() {
                    bail!("{name} must not be empty");
                }
                out.push(s);
            }
            (Param::Operands { name, .. }, Some(v)) => {
                let list = as_list(v, name)?;
                if list.is_empty() && prm.required() {
                    bail!("{} needs at least one {name}", p.name);
                }
                out.extend(list);
            }
        }
    }

    Ok((program, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::readonly::{self, DEFAULT_COMMANDS};

    fn allowed() -> Vec<String> {
        DEFAULT_COMMANDS.iter().map(|s| s.to_string()).collect()
    }

    /// A value of the right type for one field, so a test can exercise a probe
    /// without knowing what its fields mean.
    fn sample(prm: &Param) -> serde_json::Value {
        match prm {
            Param::Flag { .. } => serde_json::json!(true),
            Param::Int { min, max, .. } => {
                // Inside the declared range, and not zero where zero is excluded.
                serde_json::json!(if *min > 0 {
                    *min
                } else {
                    (*min).max(0).min(*max)
                })
            }
            Param::Text { opt, .. } => match *opt {
                // Values the program itself parses, so the render is realistic.
                "-size" => serde_json::json!("+100M"),
                "-mtime" => serde_json::json!("-1"),
                _ => serde_json::json!("probe-test"),
            },
            Param::Choice { values, .. } => serde_json::json!(values[0].0),
            Param::Word { .. } => serde_json::json!("/etc"),
            Param::Operands { .. } => serde_json::json!(["/etc/hostname"]),
        }
    }

    /// The load-bearing test of the whole design: whatever a probe builds, the
    /// whitelist accepts. `probe` decides the shape and `readonly` owns the
    /// boundary, so this is what guarantees the new layer can only ever be
    /// narrower — a table entry naming an option the whitelist refuses fails
    /// here rather than in front of an operator.
    #[test]
    fn every_probe_renders_what_validate_accepts() {
        let all = allowed();
        for p in PROBES {
            // Every field on its own, then all of them together.
            let mut every = serde_json::Map::new();
            every.insert("host".into(), serde_json::json!("h"));
            if let Cmd::ByWhat {
                field, variants, ..
            } = &p.cmd
            {
                every.insert((*field).to_string(), serde_json::json!(variants[0].0));
            }
            for prm in p.params {
                let mut one = serde_json::Map::new();
                one.insert("host".into(), serde_json::json!("h"));
                if let Cmd::ByWhat {
                    field, variants, ..
                } = &p.cmd
                {
                    one.insert((*field).to_string(), serde_json::json!(variants[0].0));
                }
                one.insert(prm.name().to_string(), sample(prm));
                for prm2 in p.params.iter().filter(|x| x.required()) {
                    one.entry(prm2.name().to_string())
                        .or_insert_with(|| sample(prm2));
                }
                every.insert(prm.name().to_string(), sample(prm));
                check(p, &serde_json::Value::Object(one), &all);
            }
            check(p, &serde_json::Value::Object(every), &all);

            // And every variant of a `what`-style probe.
            if let Cmd::ByWhat { field, .. } = &p.cmd {
                for (label, _, _) in live_variants(p, &all) {
                    let mut v = serde_json::Map::new();
                    v.insert("host".into(), serde_json::json!("h"));
                    v.insert((*field).to_string(), serde_json::json!(label));
                    for prm in p.params.iter().filter(|x| x.required()) {
                        v.insert(prm.name().to_string(), sample(prm));
                    }
                    check(p, &serde_json::Value::Object(v), &all);
                }
            }
        }
    }

    fn check(p: &Probe, v: &serde_json::Value, all: &[String]) {
        let (program, args) =
            render(p, v, all).unwrap_or_else(|e| panic!("{} could not render {v}: {e}", p.name));
        readonly::validate(program, &args, all).unwrap_or_else(|e| {
            panic!(
                "{} rendered `{program} {}` which the whitelist refuses: {e}",
                p.name,
                args.join(" ")
            )
        });
    }

    /// A command the operator configured but no tool can reach is a capability
    /// silently lost — the mirror of the old `every_permitted_command_is_described`.
    #[test]
    fn every_permitted_command_is_reachable() {
        let reachable: Vec<&str> = PROBES.iter().flat_map(programs).collect();
        for c in DEFAULT_COMMANDS {
            assert!(
                reachable.contains(c),
                "{c} is permitted but no tool runs it"
            );
        }
    }

    /// Every probe must also be reachable *from* the whitelist: a probe naming a
    /// program with no rule could never run.
    #[test]
    fn no_probe_names_a_program_the_whitelist_does_not_know() {
        for p in PROBES {
            for prog in programs(p) {
                assert!(
                    DEFAULT_COMMANDS.contains(&prog),
                    "{} runs {prog}, which is not a permitted command",
                    p.name
                );
            }
        }
    }

    /// The property the whole change exists for: no field is a token stream, so
    /// one field value is always exactly one argv element. A value with spaces
    /// in it stays one argument rather than becoming two — which is what makes
    /// `|` in a field a filename rather than a pipe.
    #[test]
    fn a_field_value_is_always_one_argument() {
        let p = find("readonly_search_files").unwrap();
        let (_, args) = render(
            p,
            &serde_json::json!({
                "host": "h", "pattern": "foo bar | baz", "paths": ["/etc/my file"]
            }),
            &allowed(),
        )
        .unwrap();
        assert!(args.contains(&"foo bar | baz".to_string()), "{args:?}");
        assert!(args.contains(&"/etc/my file".to_string()), "{args:?}");
        // Nothing was split, so nothing became a second token.
        assert_eq!(
            args.iter().filter(|a| a.contains(' ')).count(),
            2,
            "{args:?}"
        );
    }

    /// Spot-checks of the argv, because the mapping from field to option is the
    /// one thing a reader of the table cannot verify by type-checking.
    #[test]
    fn fields_render_to_the_options_they_name() {
        let all = allowed();
        let r = |name: &str, v: serde_json::Value| {
            let (prog, args) = render(find(name).unwrap(), &v, &all).unwrap();
            format!("{prog} {}", args.join(" "))
        };
        assert_eq!(
            r(
                "readonly_logs",
                serde_json::json!({"host":"h","unit":"nginx","lines":50,"priority":"err"})
            ),
            "journalctl --no-pager -u nginx -n 50 -p err"
        );
        assert_eq!(
            r(
                "readonly_service",
                serde_json::json!({"host":"h","action":"is_active","units":["nginx","postgresql"]})
            ),
            "systemctl --no-pager --full is-active nginx postgresql"
        );
        assert_eq!(
            r(
                "readonly_network",
                serde_json::json!({"host":"h","what":"listening_tcp"})
            ),
            "ss -tlnp"
        );
        assert_eq!(
            r(
                "readonly_network",
                serde_json::json!({"host":"h","what":"addresses"})
            ),
            "ip -br addr show"
        );
        assert_eq!(
            r(
                "readonly_sysinfo",
                serde_json::json!({"host":"h","what":"memory"})
            ),
            "free -h"
        );
        // `--` goes in before the operands, so a file called `-n` is a file.
        assert_eq!(
            r(
                "readonly_read_lines",
                serde_json::json!({"host":"h","from":"end","lines":20,"paths":"/var/log/syslog"})
            ),
            "tail -n 20 -- /var/log/syslog"
        );
        // The default variant applies when the field is left out.
        assert_eq!(
            r("readonly_disk_space", serde_json::json!({"host":"h"})),
            "df -h"
        );
    }

    /// The two sloppinesses small models actually produce. Both are unambiguous
    /// here because no field is ever split, so accepting them cannot change what
    /// runs — unlike the old `args: [string]`, where a string had to be guessed
    /// apart.
    #[test]
    fn sloppy_but_unambiguous_json_is_absorbed() {
        let all = allowed();
        let (_, args) = render(
            find("readonly_read_lines").unwrap(),
            &serde_json::json!({"host":"h","from":"start","lines":"25","paths":"/etc/hosts"}),
            &all,
        )
        .unwrap();
        assert_eq!(args, vec!["-n", "25", "--", "/etc/hosts"]);

        let (_, args) = render(
            find("readonly_list_dir").unwrap(),
            &serde_json::json!({"host":"h","long":"true","paths":"/etc"}),
            &all,
        )
        .unwrap();
        assert_eq!(args, vec!["-l", "--", "/etc"]);
    }

    /// A refusal has to name the fix. These are the three a model can hit.
    #[test]
    fn refusals_name_the_field_and_what_it_accepts() {
        let all = allowed();
        let e = render(
            find("readonly_logs").unwrap(),
            &serde_json::json!({"host":"h","args":["-u","nginx"]}),
            &all,
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("no field \"args\""), "{e}");
        assert!(e.contains("unit"), "it lists the fields that exist: {e}");

        let e = render(
            find("readonly_service").unwrap(),
            &serde_json::json!({"host":"h","action":"restart","units":["nginx"]}),
            &all,
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("is not one of"), "{e}");
        assert!(e.contains("status"), "{e}");

        let e = render(
            find("readonly_logs").unwrap(),
            &serde_json::json!({"host":"h","lines":99999}),
            &all,
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("between 1 and 5000"), "{e}");

        let e = render(
            find("readonly_read_file").unwrap(),
            &serde_json::json!({"host":"h"}),
            &all,
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("needs paths"), "{e}");
    }

    /// Narrowing `readonly_commands` has to reach the model, or the boundary it
    /// sees is not the one it is held to.
    #[test]
    fn narrowing_the_config_removes_tools_and_variants() {
        let narrow = AgentConfig {
            readonly_commands: vec!["cat".into(), "ss".into()],
            ..AgentConfig::default()
        };
        let names: Vec<&str> = definitions(&narrow)
            .iter()
            .map(|d| d.function.name)
            .collect();
        assert!(names.contains(&"readonly_read_file"), "{names:?}");
        assert!(names.contains(&"readonly_network"), "{names:?}");
        assert!(!names.contains(&"readonly_logs"), "{names:?}");
        assert!(!names.contains(&"readonly_sysinfo"), "{names:?}");

        // `readonly_network` survives, but only its `ss` views.
        let net = definitions(&narrow)
            .into_iter()
            .find(|d| d.function.name == "readonly_network")
            .unwrap();
        let what = net.function.parameters["properties"]["what"]["enum"].clone();
        let labels: Vec<&str> = what
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(labels.contains(&"listening_tcp"), "{labels:?}");
        assert!(
            !labels.contains(&"addresses"),
            "ip is not configured: {labels:?}"
        );

        // And a program dropped from the config after the schema went out is
        // still refused at render time.
        let e = render(
            find("readonly_logs").unwrap(),
            &serde_json::json!({"host":"h"}),
            &narrow.readonly_commands,
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("not among the commands"), "{e}");
    }

    /// Every schema is closed: no free-form object, and a key that is not a
    /// field is named rather than ignored.
    #[test]
    fn every_schema_is_closed_and_typed() {
        let all = allowed();
        for p in PROBES {
            let s = schema(p, &all);
            assert_eq!(
                s["additionalProperties"],
                serde_json::json!(false),
                "{}",
                p.name
            );
            assert!(
                s["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r == "host")
            );
            for (name, spec) in s["properties"].as_object().unwrap() {
                let ty = spec["type"].as_str().unwrap();
                assert!(
                    ["boolean", "integer", "string", "array"].contains(&ty),
                    "{}.{name} is {ty}",
                    p.name
                );
                if ty == "array" {
                    assert_eq!(spec["items"]["type"], "string", "{}.{name}", p.name);
                }
                assert!(
                    !spec["description"].as_str().unwrap_or("").is_empty(),
                    "{}.{name} has no description",
                    p.name
                );
            }
        }
    }
}
