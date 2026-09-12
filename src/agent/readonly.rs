//! The read-only tool: one command, one host, no confirmation.
//!
//! Two independent defences, because either alone is insufficient:
//!
//! 1. **Quoting** (`ssh::quote_command`) stops an *argument* from becoming a
//!    second command. `ssh host cmd a b` does not pass argv to the remote
//!    program — sshd joins the tokens and hands the string to a login shell.
//! 2. **The allowlist below** stops the *program* being something that writes.
//!    A bare list of program names is not enough either: `systemctl status` only
//!    reads but `systemctl start` does not, and `find -exec` runs anything at
//!    all. So a program may carry argument rules as well.
//!
//! Anything not permitted here has to go through a confirmed plan.

use anyhow::{Result, bail};

/// Argument restrictions for a program whose read-only-ness depends on them.
struct Rule {
    /// When set, the first non-flag argument must be one of these.
    allow_first: Option<&'static [&'static str]>,
    /// Arguments that are never allowed, matched exactly.
    deny: &'static [&'static str],
    /// Argument prefixes that are never allowed.
    deny_prefix: &'static [&'static str],
    /// Reject any argument containing one of these substrings.
    deny_contains: &'static [&'static str],
}

const PLAIN: Rule = Rule {
    allow_first: None,
    deny: &[],
    deny_prefix: &[],
    deny_contains: &[],
};

/// Programs that read and never write, with the argument rules that keep them
/// that way. Editing this table is a trust decision — see `describe_rules`.
fn rule_for(program: &str) -> Option<Rule> {
    Some(match program {
        // Pure readers: nothing they can be asked to do mutates.
        "ls" | "cat" | "head" | "stat" | "file" | "readlink" | "realpath" | "du" | "df"
        | "lsblk" | "blkid" | "findmnt" | "uname" | "hostname" | "uptime" | "free" | "id"
        | "whoami" | "date" | "lscpu" | "lspci" | "lsmod" | "ps" | "lsof" | "getent" | "wc"
        | "grep" | "which" | "arch" | "nproc" | "vmstat" | "iostat" => PLAIN,

        // `tail -f` never returns; the timeout would catch it, but failing fast
        // with a clear message is better than a mystery stall.
        "tail" => Rule {
            deny: &["-f", "-F", "--follow", "--retry"],
            ..PLAIN
        },

        // `find` is a read tool with an execution engine bolted on.
        "find" => Rule {
            deny: &[
                "-delete", "-exec", "-execdir", "-ok", "-okdir", "-fls", "-fprint", "-fprint0",
                "-fprintf",
            ],
            ..PLAIN
        },

        // Reading the journal is fine; managing it is not.
        "journalctl" => Rule {
            deny: &["--rotate", "--flush", "--sync", "--relinquish-var"],
            deny_prefix: &["--vacuum"],
            ..PLAIN
        },

        // `dmesg -C` clears the ring buffer.
        "dmesg" => Rule {
            deny: &["-C", "--clear", "-c", "--read-clear"],
            ..PLAIN
        },

        // `ss -K` kills sockets.
        "ss" => Rule {
            deny: &["-K", "--kill"],
            ..PLAIN
        },

        // The subcommand is what decides: `status` reads, `start` does not.
        "systemctl" => Rule {
            allow_first: Some(&[
                "status",
                "show",
                "cat",
                "is-active",
                "is-enabled",
                "is-failed",
                "is-system-running",
                "list-units",
                "list-unit-files",
                "list-timers",
                "list-sockets",
                "list-dependencies",
                "get-default",
                "show-environment",
            ]),
            ..PLAIN
        },

        // `ip addr`/`route`/`link` list; `ip link set` reconfigures.
        "ip" => Rule {
            deny: &[
                "set", "add", "del", "delete", "change", "replace", "append", "flush",
            ],
            ..PLAIN
        },

        // `sysctl -w` and `sysctl key=value` both write.
        "sysctl" => Rule {
            deny: &["-w", "--write"],
            deny_contains: &["="],
            ..PLAIN
        },

        _ => return None,
    })
}

/// The commands permitted unless config.toml narrows them.
pub const DEFAULT_COMMANDS: &[&str] = &[
    "ls",
    "cat",
    "head",
    "tail",
    "stat",
    "file",
    "readlink",
    "realpath",
    "find",
    "du",
    "df",
    "lsblk",
    "blkid",
    "findmnt",
    "uname",
    "hostname",
    "uptime",
    "free",
    "id",
    "whoami",
    "date",
    "lscpu",
    "lspci",
    "lsmod",
    "ps",
    "ss",
    "lsof",
    "getent",
    "wc",
    "grep",
    "which",
    "nproc",
    "systemctl",
    "journalctl",
    "ip",
    "dmesg",
    "sysctl",
    "vmstat",
];

/// Validate a call before anything is spawned.
///
/// `allowed` is the configured list; a program must be in it *and* satisfy its
/// rules. A program with no rule entry is rejected outright — adding one is a
/// code change on purpose, so a stray config edit cannot widen the boundary.
pub fn validate(program: &str, args: &[String], allowed: &[String]) -> Result<()> {
    if program.trim().is_empty() {
        bail!("no command given");
    }
    // A path would sidestep the name check entirely.
    if program.contains('/') || program.contains("..") {
        bail!("command must be a bare name, not a path: {program:?}");
    }
    if !allowed.iter().any(|a| a == program) {
        bail!(
            "{program:?} is not a permitted read-only command. \
             Permitted: {}. Anything else has to go in a plan.",
            allowed.join(", ")
        );
    }
    let Some(rule) = rule_for(program) else {
        bail!(
            "{program:?} has no read-only rule defined, so it cannot run unattended. \
             Put it in a plan instead."
        );
    };

    for a in args {
        if rule.deny.contains(&a.as_str()) {
            bail!("{program:?} may not be used with {a:?} — that can modify the host.");
        }
        if let Some(p) = rule.deny_prefix.iter().find(|p| a.starts_with(**p)) {
            bail!("{program:?} may not be used with {p}… — that can modify the host.");
        }
        if let Some(c) = rule.deny_contains.iter().find(|c| a.contains(**c)) {
            bail!("{program:?} may not be used with an argument containing {c:?}.");
        }
    }

    if let Some(allow_first) = rule.allow_first {
        // Flags may precede the subcommand (`systemctl --no-pager status`).
        let first = args.iter().find(|a| !a.starts_with('-'));
        match first {
            Some(sub) if allow_first.contains(&sub.as_str()) => {}
            Some(sub) => bail!(
                "{program} {sub:?} is not read-only. Permitted: {}.",
                allow_first.join(", ")
            ),
            None => bail!("{program} needs one of: {}.", allow_first.join(", ")),
        }
    }

    Ok(())
}

/// One line for the system prompt, so the model knows the boundary up front
/// rather than discovering it by being refused.
pub fn describe(allowed: &[String]) -> String {
    format!(
        "Permitted read-only commands: {}. \
         systemctl is limited to status/show/cat/is-* and list-*; \
         find may not use -exec/-delete; journalctl may not vacuum; \
         ip may not set/add/del; sysctl may not write.",
        allowed.join(" ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed() -> Vec<String> {
        DEFAULT_COMMANDS.iter().map(|s| s.to_string()).collect()
    }

    fn ok(program: &str, args: &[&str]) {
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        validate(program, &a, &allowed())
            .unwrap_or_else(|e| panic!("{program} {args:?} should be allowed: {e}"));
    }

    fn rejected(program: &str, args: &[&str]) -> String {
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match validate(program, &a, &allowed()) {
            Ok(()) => panic!("{program} {args:?} should have been rejected"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn ordinary_diagnostics_are_allowed() {
        ok("ls", &["-la", "/etc"]);
        ok("cat", &["/etc/os-release"]);
        ok("df", &["-h"]);
        ok("blkid", &[]);
        ok("ps", &["aux"]);
        ok("journalctl", &["-u", "nginx", "-n", "50"]);
        ok("systemctl", &["status", "nginx"]);
        ok("systemctl", &["--no-pager", "status", "nginx"]);
        ok("ip", &["addr"]);
        ok("ip", &["-br", "link"]);
        ok("find", &["/var/log", "-name", "*.log"]);
    }

    #[test]
    fn a_path_is_not_a_command_name() {
        assert!(rejected("/bin/ls", &[]).contains("bare name"));
        assert!(rejected("../../bin/sh", &[]).contains("bare name"));
        assert!(rejected("", &[]).contains("no command"));
    }

    #[test]
    fn anything_off_the_list_is_refused() {
        assert!(rejected("rm", &["-rf", "/"]).contains("not a permitted"));
        assert!(rejected("bash", &["-c", "id"]).contains("not a permitted"));
        assert!(rejected("sh", &[]).contains("not a permitted"));
        assert!(rejected("curl", &["http://x"]).contains("not a permitted"));
        assert!(rejected("apt-get", &["install", "x"]).contains("not a permitted"));
    }

    /// The reason a name-only allowlist is not enough.
    #[test]
    fn mutating_subcommands_of_permitted_programs_are_refused() {
        assert!(rejected("systemctl", &["restart", "nginx"]).contains("not read-only"));
        assert!(rejected("systemctl", &["start", "nginx"]).contains("not read-only"));
        assert!(rejected("systemctl", &["--now", "disable", "x"]).contains("not read-only"));
        assert!(rejected("systemctl", &[]).contains("needs one of"));
    }

    /// The reason `find` is special: it has an execution engine.
    #[test]
    fn find_may_not_execute_or_delete() {
        assert!(rejected("find", &["/", "-delete"]).contains("modify"));
        assert!(rejected("find", &["/", "-exec", "rm", "{}", ";"]).contains("modify"));
        assert!(rejected("find", &["/", "-execdir", "sh", ";"]).contains("modify"));
        assert!(rejected("find", &["/", "-ok", "rm", ";"]).contains("modify"));
        assert!(rejected("find", &["/tmp", "-fprint", "/etc/passwd"]).contains("modify"));
    }

    #[test]
    fn other_mutating_flags_are_refused() {
        assert!(rejected("journalctl", &["--vacuum-time=1s"]).contains("modify"));
        assert!(rejected("journalctl", &["--rotate"]).contains("modify"));
        assert!(rejected("dmesg", &["-C"]).contains("modify"));
        assert!(rejected("dmesg", &["--clear"]).contains("modify"));
        assert!(rejected("ss", &["-K"]).contains("modify"));
        assert!(rejected("ip", &["link", "set", "eth0", "down"]).contains("modify"));
        assert!(rejected("ip", &["addr", "flush", "dev", "eth0"]).contains("modify"));
        assert!(rejected("sysctl", &["-w", "net.ipv4.ip_forward=1"]).contains("modify"));
        // Even without -w, `key=value` writes.
        assert!(
            rejected("sysctl", &["net.ipv4.ip_forward=1"]).contains("containing"),
            "sysctl key=value must be refused"
        );
    }

    /// `tail -f` would hang until the timeout; refuse it with a real reason.
    #[test]
    fn tail_may_not_follow() {
        assert!(rejected("tail", &["-f", "/var/log/syslog"]).contains("modify"));
        ok("tail", &["-n", "100", "/var/log/syslog"]);
    }

    /// Narrowing the config narrows the boundary; it cannot widen it past the
    /// rule table.
    #[test]
    fn the_config_list_can_only_narrow() {
        let narrow = vec!["ls".to_string(), "cat".to_string()];
        assert!(validate("ls", &[], &narrow).is_ok());
        assert!(validate("journalctl", &[], &narrow).is_err());

        // A program the user adds that has no rule is still refused.
        let widened = vec!["rm".to_string()];
        let err = validate("rm", &["-rf".into()], &widened)
            .unwrap_err()
            .to_string();
        assert!(err.contains("no read-only rule"), "{err}");
    }

    #[test]
    fn the_description_names_the_boundary() {
        let d = describe(&allowed());
        assert!(d.contains("journalctl"));
        assert!(d.contains("systemctl is limited"));
    }
}
