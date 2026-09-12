//! The read-only tool: one command, one host, no confirmation.
//!
//! Two independent defences, because either alone is insufficient:
//!
//! 1. **Quoting** (`ssh::quote_command`) stops an *argument* from becoming a
//!    second command. `ssh host cmd a b` does not pass argv to the remote
//!    program — sshd joins the tokens and hands the string to a login shell.
//! 2. **The whitelist below** stops the *program*, or one of its options, from
//!    being something that writes.
//!
//! Both lists are whitelists. A blacklist is the wrong shape for a security
//! boundary: it is only ever as good as the last time someone read the man
//! page, and an option added by a future release is permitted by default. Here
//! the failure mode of an unrecognised option is a refusal, not an execution.
//!
//! Anything not permitted has to go through a confirmed plan.

use anyhow::{Result, bail};

/// What arguments a permitted program may take.
enum Args {
    /// The program has no option that writes anything, so its options need no
    /// policing. Operands are paths and patterns, already shell-quoted.
    Free,
    /// Only these options are permitted; anything else starting with `-` is
    /// refused.
    Only {
        allow: &'static [&'static str],
        /// Options that consume the following token. Without this, a value
        /// that looks like an option — `find -mtime -1`, `find -size +100M` —
        /// is checked as if it were one, and refused.
        value_options: &'static [&'static str],
        /// When set, the first operand must be one of these.
        subcommands: Option<&'static [&'static str]>,
        /// When set and a second operand is present, it must be one of these.
        /// `ip link` lists but `ip link set` reconfigures, and the verb is the
        /// second operand, not the first.
        verbs: Option<&'static [&'static str]>,
        /// Refuse an operand containing any of these.
        operand_must_not_contain: &'static [&'static str],
        /// Whether single-dash options may be clustered (`-la` == `-l -a`).
        /// Off by default, so an unrecognised option can never be split into
        /// permitted letters.
        clustered: bool,
    },
}

const fn only(
    allow: &'static [&'static str],
    value_options: &'static [&'static str],
    clustered: bool,
) -> Args {
    Args::Only {
        allow,
        value_options,
        subcommands: None,
        verbs: None,
        operand_must_not_contain: &[],
        clustered,
    }
}

/// The permitted programs and the arguments each may take.
///
/// Editing this table is a trust decision: everything here runs unattended.
fn rule_for(program: &str) -> Option<Args> {
    Some(match program {
        // No option of these writes anything, so operands are the only input
        // and they are quoted before they leave.
        "ls" | "cat" | "head" | "stat" | "file" | "readlink" | "realpath" | "du" | "df"
        | "uname" | "hostname" | "uptime" | "free" | "id" | "whoami" | "date" | "lscpu"
        | "lspci" | "lsmod" | "ps" | "lsof" | "getent" | "wc" | "grep" | "which" | "arch"
        | "nproc" | "vmstat" | "findmnt" | "lsblk" => Args::Free,

        // `tail -f` never returns.
        "tail" => only(
            &["-n", "--lines", "-c", "--bytes", "-q", "-v", "-z"],
            &["-n", "--lines", "-c", "--bytes"],
            true,
        ),

        // find is a read tool with an execution engine attached, so its
        // expression primaries are whitelisted: -exec, -delete, -fprintf and
        // friends are simply not on the list.
        "find" => only(
            &[
                "-name",
                "-iname",
                "-path",
                "-ipath",
                "-regex",
                "-iregex",
                "-type",
                "-size",
                "-empty",
                "-maxdepth",
                "-mindepth",
                "-mtime",
                "-atime",
                "-ctime",
                "-mmin",
                "-amin",
                "-cmin",
                "-newer",
                "-anewer",
                "-cnewer",
                "-user",
                "-group",
                "-uid",
                "-gid",
                "-nouser",
                "-nogroup",
                "-perm",
                "-links",
                "-inum",
                "-samefile",
                "-readable",
                "-writable",
                "-executable",
                "-print",
                "-print0",
                "-ls",
                "-xdev",
                "-mount",
                "-prune",
                "-follow",
                "-not",
                "-a",
                "-and",
                "-o",
                "-or",
                "-true",
                "-false",
                "-depth",
                "-L",
                "-H",
                "-P",
            ],
            &[
                "-name",
                "-iname",
                "-path",
                "-ipath",
                "-regex",
                "-iregex",
                "-type",
                "-size",
                "-maxdepth",
                "-mindepth",
                "-mtime",
                "-atime",
                "-ctime",
                "-mmin",
                "-amin",
                "-cmin",
                "-newer",
                "-anewer",
                "-cnewer",
                "-user",
                "-group",
                "-uid",
                "-gid",
                "-perm",
                "-links",
                "-inum",
                "-samefile",
            ],
            false,
        ),

        // Reading the journal, never managing it: no --vacuum-*, no --rotate,
        // and no -f, which would never return.
        "journalctl" => only(
            &[
                "-u",
                "--unit",
                "--user-unit",
                "-n",
                "--lines",
                "--since",
                "-S",
                "--until",
                "-U",
                "-p",
                "--priority",
                "-b",
                "--boot",
                "-k",
                "--dmesg",
                "-x",
                "--catalog",
                "-r",
                "--reverse",
                "-o",
                "--output",
                "-g",
                "--grep",
                "--no-pager",
                "--no-hostname",
                "--no-full",
                "-a",
                "--all",
                "-t",
                "--identifier",
                "-m",
                "--merge",
                "--system",
                "--utc",
                "-q",
                "--quiet",
                "--case-sensitive",
            ],
            &[
                "-u",
                "--unit",
                "--user-unit",
                "-n",
                "--lines",
                "--since",
                "-S",
                "--until",
                "-U",
                "-p",
                "--priority",
                "-b",
                "--boot",
                "-o",
                "--output",
                "-g",
                "--grep",
                "-t",
                "--identifier",
            ],
            false,
        ),

        // No -C/--clear, and no -w/--follow.
        "dmesg" => only(
            &[
                "-T",
                "--ctime",
                "-H",
                "--human",
                "-k",
                "--kernel",
                "-u",
                "--userspace",
                "-x",
                "--decode",
                "-l",
                "--level",
                "-f",
                "--facility",
                "-t",
                "--notime",
                "-e",
                "--reltime",
                "-P",
                "--nopager",
                "-J",
                "--json",
            ],
            &["-l", "--level", "-f", "--facility"],
            true,
        ),

        // No -K/--kill.
        "ss" => only(
            &[
                "-t",
                "-u",
                "-x",
                "-w",
                "-l",
                "-n",
                "-p",
                "-a",
                "-e",
                "-m",
                "-i",
                "-s",
                "-4",
                "-6",
                "-H",
                "-O",
                "-r",
                "--tcp",
                "--udp",
                "--listening",
                "--all",
                "--numeric",
                "--processes",
                "--info",
                "--summary",
                "--no-header",
            ],
            &[],
            true,
        ),

        // The subcommand decides: `status` reads, `restart` does not.
        "systemctl" => Args::Only {
            allow: &[
                "--no-pager",
                "--no-legend",
                "--plain",
                "--full",
                "-l",
                "--all",
                "-a",
                "-t",
                "--type",
                "--state",
                "-o",
                "--output",
                "-n",
                "--lines",
                "-q",
                "--quiet",
                "--user",
                "--system",
                "--recursive",
                "-r",
                "--reverse",
            ],
            value_options: &["-t", "--type", "--state", "-o", "--output", "-n", "--lines"],
            subcommands: Some(&[
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
            verbs: None,
            operand_must_not_contain: &[],
            clustered: false,
        },

        // `ip addr`/`route`/`link` list; `ip link set` reconfigures — so the
        // object is whitelisted too, and no writing verb is on the list.
        "ip" => Args::Only {
            allow: &[
                "-s",
                "-d",
                "-o",
                "-br",
                "-4",
                "-6",
                "-j",
                "-p",
                "-c",
                "--json",
                "--brief",
                "--details",
                "--oneline",
                "--stats",
            ],
            value_options: &[],
            subcommands: Some(&[
                "addr", "address", "a", "link", "l", "route", "r", "neigh", "n", "rule", "maddr",
                "mroute", "netns", "tunnel", "tuntap",
            ]),
            // `ip link` lists; `ip link set` reconfigures. Only the reading
            // verbs are named, so every writing one is refused.
            verbs: Some(&["show", "list", "lst", "get", "s", "l", "sh"]),
            operand_must_not_contain: &[],
            clustered: false,
        },

        // Reading values only: no -w, and no `key=value`, which writes even
        // without it.
        "sysctl" => Args::Only {
            allow: &["-a", "-A", "-n", "-e", "-N", "--all", "--names", "--values"],
            value_options: &[],
            subcommands: None,
            verbs: None,
            operand_must_not_contain: &["="],
            clustered: true,
        },

        // `blkid -g` rewrites the cache.
        "blkid" => only(
            &[
                "-o",
                "--output",
                "-s",
                "--match-tag",
                "-t",
                "--match-token",
                "-L",
                "--label",
                "-U",
                "--uuid",
                "-p",
                "--probe",
                "-i",
                "--info",
                "-k",
                "--list-filesystems",
            ],
            &[
                "-o",
                "--output",
                "-s",
                "--match-tag",
                "-t",
                "--match-token",
                "-L",
                "--label",
                "-U",
                "--uuid",
            ],
            false,
        ),

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

/// Tokens that mean something only to a shell.
///
/// There is no shell here. `ssh` hands sshd one string, but every token of it
/// has been single-quoted (`ssh::quote_command`), so `|` arrives at the program
/// as a literal argument. That used to be the end of the matter — harmless,
/// because nothing could execute. It stops being harmless once the model
/// believes it is typing at a prompt: `lsblk | head -n 100` becomes
/// `'lsblk' '|' 'head' '-n' '100'`, and lsblk answers with a complaint about a
/// device called `|`. A confusing error from the far end is worse than a
/// refusal, because nothing in it says what this tool would not do.
///
/// Matched as whole tokens and never as substrings. `grep -E 'a|b'` is an
/// ordinary pattern and `find / -name '*>*'` an ordinary one; what is being
/// recognised is not a dangerous character but a model that thinks it has a
/// shell.
const SHELL_TOKENS: &[&str] = &[
    "|", "||", "&&", "&", ";", ";;", "<", "<<", "<<<", "2>&1", "1>&2",
];

fn shell_token(arg: &str) -> bool {
    SHELL_TOKENS.contains(&arg)
        || arg.starts_with('>')
        || arg.starts_with("2>")
        || arg.starts_with("1>")
        || arg.starts_with("$(")
        || arg.starts_with('`')
}

/// What to do instead, named per operator.
///
/// "There is no shell" says what the model may not do and leaves it to work out
/// what to do instead, which is the round trip this module exists to avoid. Each
/// operator has a substitution, and the reason it is unnecessary is usually that
/// the tool already does the thing the operator was reaching for.
fn no_shell_fix(tok: &str) -> &'static str {
    match tok {
        "|" | "||" => {
            "To bound output, use the command's own option — `head -n`, `tail -n`, \
             `journalctl -n`; it is capped for you in any case. To search a file, \
             `grep -n <pattern> <file>` is the whole command."
        }
        "2>&1" | "1>&2" => "stderr is captured and returned to you already, labelled.",
        ";" | ";;" | "&&" | "&" => {
            "To answer two questions in one call, give one command several operands — \
             `cat /etc/os-release /etc/hostname`, `systemctl status nginx postgresql`. \
             Otherwise make two calls."
        }
        t if t.starts_with('>') || t.starts_with("2>") || t.starts_with("1>") => {
            "Nothing here may write to a file; the output is returned to you."
        }
        t if t.starts_with("$(") || t.starts_with('`') => {
            "Nothing is substituted or expanded. Read the value in one call and use it \
             in the next."
        }
        _ => "Pass the program its own options instead.",
    }
}

/// Is `arg` an option, or an operand?
fn is_option(arg: &str) -> bool {
    arg.chars().count() > 1 && arg.starts_with('-') && arg != "--"
}

/// Check one option against the whitelist.
fn option_allowed(arg: &str, allow: &[&str], clustered: bool) -> bool {
    if allow.contains(&arg) {
        return true;
    }
    // `--lines=50` is the `--lines` option.
    if arg.starts_with("--")
        && let Some((head, _)) = arg.split_once('=')
        && allow.contains(&head)
    {
        return true;
    }
    if !clustered || arg.starts_with("--") {
        return false;
    }
    // Char-wise, so a multi-byte operand cannot panic a byte slice.
    let body: Vec<char> = arg.chars().skip(1).collect();
    if body.is_empty() {
        return false;
    }
    // `-n50`: one option letter with its value attached.
    if body.len() > 1
        && !body[1..].iter().all(|c| c.is_ascii_alphabetic())
        && allow.contains(&format!("-{}", body[0]).as_str())
    {
        return true;
    }
    // `-la` == `-l -a`; every letter must be permitted on its own.
    body.iter()
        .all(|c| allow.contains(&format!("-{c}").as_str()))
}

/// Validate a call before anything is spawned.
///
/// `allowed` is the configured list; a program must be in it *and* satisfy its
/// rule. A program with no rule is refused outright, so narrowing the config
/// narrows the boundary while widening it cannot.
pub fn validate(program: &str, args: &[String], allowed: &[String]) -> Result<()> {
    if program.trim().is_empty() {
        bail!("no command given");
    }
    // A whole command line where a program name belongs. No model can reach this
    // any more — `probe` builds the argv from typed fields and the program comes
    // out of its own table — but it is checked before the allow-list because of
    // what the old message did when a model *could*: `lsblk | head -n 100` was
    // compared against the allow-list as if it were a program name and refused
    // as "not a permitted command" by a message that then listed `lsblk` among
    // the permitted ones. No rule can be extracted from a refusal that
    // contradicts itself, so the model guessed again, which is how one mistake
    // became a session of them. Kept as a guard on the boundary rather than a
    // message to a model: a caller that builds an argv wrongly is told plainly.
    if let Some(first) = program.split_whitespace().next()
        && first != program.trim()
    {
        let rest: Vec<&str> = program.split_whitespace().skip(1).collect();
        if rest.iter().any(|t| shell_token(t)) {
            bail!(
                "{program:?} is a command line, not a program, and there is no shell \
                 here. Only {first:?} could run."
            );
        }
        bail!(
            "{program:?} is a command line, not a program: {first:?} is the program and \
             {rest:?} are its arguments."
        );
    }

    // A path would sidestep the name check entirely.
    if program.contains('/') || program.contains("..") {
        bail!("command must be a bare name, not a path: {program:?}");
    }

    if !allowed.iter().any(|a| a == program) {
        bail!(
            "{program:?} is not a permitted read-only command. Permitted: {}. \
             Anything else has to go in a plan.",
            allowed.join(", ")
        );
    }
    let Some(rule) = rule_for(program) else {
        bail!(
            "{program:?} has no read-only rule defined, so it cannot run unattended. \
             Put it in a plan instead."
        );
    };

    // Before the rule, because this reaches every program — including the ones
    // whose options are unpoliced, which is where it used to happen: `lsblk`
    // takes any option, so `["|", "head"]` passed the whitelist and went out to
    // be quoted into `'lsblk' '|' 'head'`, which lsblk answered with a complaint
    // about a device named `|`. A confusing error from the far end is worse than
    // a refusal, because nothing in it says what this tool would not do.
    //
    // `probe` cannot produce one of these now, since no field is a token stream.
    // This stays as the check that keeps that true: a probe table entry that
    // rendered shell syntax would be caught here, and by the test that renders
    // every probe.
    //
    // Stops at `--`, which already means "operands from here" everywhere else in
    // this module, so a path that really is called `|` is still a path.
    if let Some(tok) = args
        .iter()
        .take_while(|a| *a != "--")
        .find(|a| shell_token(a))
    {
        bail!(
            "{tok:?} is shell syntax, and {program} is run without a shell — it would \
             reach {program} as a literal argument. Nothing here is piped, redirected or \
             chained. {}",
            no_shell_fix(tok)
        );
    }

    let Args::Only {
        allow,
        value_options,
        subcommands,
        verbs,
        operand_must_not_contain,
        clustered,
    } = rule
    else {
        return Ok(());
    };

    // Everything after a bare `--` is an operand, never an option.
    let mut operands_only = false;
    let mut expecting_value = false;
    let mut operands: Vec<&String> = Vec::new();
    for a in args {
        if a == "--" {
            operands_only = true;
            continue;
        }
        // The token after `-mtime` is its value, whatever it looks like.
        if expecting_value {
            expecting_value = false;
            continue;
        }
        if !operands_only && is_option(a) {
            if !option_allowed(a, allow, clustered) {
                bail!(
                    "{program} may not be used with {a:?}.{} Permitted options: {}.",
                    bundling_hint(a, allow, clustered),
                    allow.join(" ")
                );
            }
            // `--lines=50` carries its own value; `--lines 50` eats the next.
            expecting_value = value_options.contains(&a.as_str());
        } else {
            operands.push(a);
        }
    }

    for a in &operands {
        if let Some(c) = operand_must_not_contain.iter().find(|c| a.contains(**c)) {
            bail!("{program:?} may not be used with an argument containing {c:?}.");
        }
    }

    if let Some(subs) = subcommands {
        match operands.first() {
            Some(sub) if subs.contains(&sub.as_str()) => {}
            Some(sub) => bail!(
                "{program} {sub:?} is not read-only. Permitted: {}.",
                subs.join(", ")
            ),
            None => bail!("{program} needs one of: {}.", subs.join(", ")),
        }
    }

    if let Some(verbs) = verbs
        && let Some(verb) = operands.get(1)
        && !verbs.contains(&verb.as_str())
    {
        bail!(
            "{program} {} {verb:?} is not read-only. Permitted: {}.",
            operands[0],
            verbs.join(", ")
        );
    }

    Ok(())
}

/// Why `journalctl -rn 50` was refused when `-r` and `-n` are both permitted.
///
/// Bundling is off for most commands, so an unrecognised option can never be
/// split into permitted letters. That makes a bundle of *permitted* letters
/// collateral damage, and without this the model has to work out for itself
/// why a refusal names options it thought it had used. Naming the rewrite
/// turns a round trip of reasoning into a substitution.
fn bundling_hint(token: &str, allow: &[&str], clustered: bool) -> String {
    if clustered || !token.starts_with('-') || token.starts_with("--") || token.len() <= 2 {
        return String::new();
    }
    let split: Vec<String> = token[1..].chars().map(|c| format!("-{c}")).collect();
    if split.iter().all(|o| allow.contains(&o.as_str())) {
        return format!(
            " Options here are not bundled — pass {} as separate arguments.",
            split.join(" ")
        );
    }
    String::new()
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
        ok("journalctl", &["--lines=50", "--no-pager"]);
        ok("systemctl", &["status", "nginx"]);
        ok("systemctl", &["--no-pager", "status", "nginx"]);
        ok("ip", &["addr"]);
        ok("ip", &["-br", "link"]);
        // An option's value may itself look like an option.
        ok("find", &["/var/log", "-name", "*.log", "-mtime", "-1"]);
        ok("find", &["/var", "-size", "+100M", "-maxdepth", "3"]);
        ok("journalctl", &["-p", "3", "-b", "-1"]);
        ok("tail", &["-n", "100", "/var/log/syslog"]);
        ok("ss", &["-tlnp"]);
        ok("dmesg", &["-T"]);
        ok("sysctl", &["-a"]);
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
        assert!(rejected("curl", &["http://x"]).contains("not a permitted"));
        assert!(rejected("apt-get", &["install", "x"]).contains("not a permitted"));
    }

    /// The point of whitelisting options: an option nobody has heard of is
    /// refused rather than permitted by omission.
    #[test]
    fn an_unknown_option_is_refused_not_assumed_harmless() {
        assert!(rejected("journalctl", &["--some-future-flag"]).contains("may not be used"));
        assert!(rejected("find", &["/", "-newprimary"]).contains("may not be used"));
        assert!(rejected("systemctl", &["--force", "status", "x"]).contains("may not be used"));
        assert!(rejected("tail", &["--weird"]).contains("may not be used"));
    }

    /// The reason a name-only allowlist is not enough.
    #[test]
    fn mutating_subcommands_of_permitted_programs_are_refused() {
        assert!(rejected("systemctl", &["restart", "nginx"]).contains("not read-only"));
        assert!(rejected("systemctl", &["start", "nginx"]).contains("not read-only"));
        assert!(rejected("systemctl", &[]).contains("needs one of"));
        assert!(rejected("ip", &["tcp_metrics"]).contains("not read-only"));
    }

    /// The reason `find` is special: it has an execution engine.
    #[test]
    fn find_may_not_execute_or_delete() {
        for bad in [
            "-delete", "-exec", "-execdir", "-ok", "-okdir", "-fls", "-fprint", "-fprint0",
            "-fprintf",
        ] {
            let e = rejected("find", &["/", bad]);
            assert!(e.contains("may not be used"), "{bad}: {e}");
        }
    }

    #[test]
    fn other_mutating_options_are_refused() {
        assert!(rejected("journalctl", &["--vacuum-time=1s"]).contains("may not be used"));
        assert!(rejected("journalctl", &["--rotate"]).contains("may not be used"));
        assert!(rejected("journalctl", &["-f"]).contains("may not be used"));
        assert!(rejected("dmesg", &["-C"]).contains("may not be used"));
        assert!(rejected("dmesg", &["--clear"]).contains("may not be used"));
        assert!(rejected("ss", &["-K"]).contains("may not be used"));
        // `ip link` lists, but the verb is the second operand.
        for verb in ["set", "add", "del", "delete", "change", "replace", "flush"] {
            let e = rejected("ip", &["link", verb]);
            assert!(e.contains("not read-only"), "ip link {verb}: {e}");
        }
        ok("ip", &["link", "show"]);
        ok("ip", &["route", "get", "8.8.8.8"]);
        assert!(rejected("sysctl", &["-w", "net.ipv4.ip_forward=1"]).contains("may not be used"));
        // Even without -w, `key=value` writes.
        assert!(rejected("sysctl", &["net.ipv4.ip_forward=1"]).contains("containing"));
        assert!(rejected("blkid", &["-g"]).contains("may not be used"));
    }

    /// `tail -f` would hang until the timeout; refuse it with a real reason.
    #[test]
    fn tail_may_not_follow() {
        assert!(rejected("tail", &["-f", "/var/log/syslog"]).contains("may not be used"));
        assert!(rejected("tail", &["-F"]).contains("may not be used"));
    }

    /// Clustering is per-program, so `-exec` can never be read as permitted
    /// letters — and where it is on, `-la` and `-n50` still work.
    #[test]
    fn option_clusters_are_only_split_where_declared() {
        // `ss` declares clustering.
        assert!(option_allowed("-tlnp", &["-t", "-l", "-n", "-p"], true));
        assert!(!option_allowed("-tlnK", &["-t", "-l", "-n", "-p"], true));
        // An attached value.
        assert!(option_allowed("-n50", &["-n"], true));
        // `find` does not, so nothing is split.
        assert!(!option_allowed("-ex", &["-e", "-x"], false));
        // Long options are never split even when clustering is on.
        assert!(!option_allowed("--exec", &["-e", "-x", "-c"], true));
    }

    #[test]
    fn everything_after_a_double_dash_is_an_operand() {
        // A file that looks like an option is still just a file.
        ok("tail", &["-n", "5", "--", "-weird-filename"]);
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

    /// A refusal should cost a substitution, not a fresh round of reasoning.
    #[test]
    fn a_refused_bundle_names_the_rewrite() {
        let err = validate("journalctl", &["-rn".into(), "50".into()], &allowed())
            .unwrap_err()
            .to_string();
        assert!(err.contains("not bundled"), "{err}");
        assert!(err.contains("-r -n"), "it spells out the fix: {err}");

        // A bundle containing something genuinely refused gets no such hint:
        // splitting it would not help, and suggesting it would mislead.
        let err = validate("journalctl", &["-rf".into()], &allowed())
            .unwrap_err()
            .to_string();
        assert!(!err.contains("not bundled"), "{err}");

        // Where the program's own parser is unambiguous, bundles are allowed
        // and there is nothing to explain.
        assert!(validate("tail", &["-qv".into()], &allowed()).is_ok());
    }

    /// The second layer's guard against a caller that builds an argv wrongly.
    /// No model can reach it — `probe` renders the program from its own table —
    /// but this is the shape that produced the refusal loop this whole change
    /// came from, so the boundary still names it for what it is rather than
    /// reporting a command line as an unknown program.
    #[test]
    fn a_command_line_where_a_program_belongs_is_named_as_one() {
        let err = validate("lsblk | head -n 100", &[], &allowed())
            .unwrap_err()
            .to_string();
        assert!(err.contains("no shell"), "{err}");
        assert!(
            err.contains("\"lsblk\""),
            "it names the program it found: {err}"
        );
        // And never the self-contradiction: the program was not the fault, so
        // the permitted list has no business in the message.
        assert!(!err.contains("is not a permitted"), "{err}");
        assert!(!err.contains("journalctl"), "no command list: {err}");

        // The other paste from the same session.
        let err = validate("lsblk --list=all 2>&1 || lsblk", &[], &allowed())
            .unwrap_err()
            .to_string();
        assert!(err.contains("no shell"), "{err}");

        // A line with no shell syntax in it is still a line, and is taken apart
        // rather than reported as a missing program.
        let err = validate("cat /etc/os-release", &[], &allowed())
            .unwrap_err()
            .to_string();
        assert!(err.contains("\"cat\" is the program"), "{err}");
        assert!(err.contains("/etc/os-release"), "it names the rest: {err}");
        // Not reported as a path, which is what the `/` check would have said.
        assert!(!err.contains("bare name"), "{err}");
    }

    /// Shell syntax among the arguments used to be *accepted*: `lsblk` takes any
    /// option, so `["|", "head"]` passed the whitelist, was quoted, and came
    /// back as a complaint about a device named `|`. Nothing a probe renders can
    /// contain one now; this is what keeps that a fact rather than an intention.
    #[test]
    fn shell_syntax_among_the_arguments_is_refused_with_the_substitution() {
        let err = validate("lsblk", &["|".into(), "head".into()], &allowed())
            .unwrap_err()
            .to_string();
        assert!(err.contains("shell syntax"), "{err}");
        assert!(err.contains("literal argument"), "{err}");
        assert!(err.contains("head -n"), "it names the substitution: {err}");
        assert!(!err.contains("is not a permitted"), "{err}");

        for (tok, fix) in [
            ("2>&1", "stderr is captured"),
            (";", "several operands"),
            ("&&", "several operands"),
            (">/tmp/x", "may write to a file"),
            ("$(which", "substituted"),
        ] {
            let err = validate("cat", &[tok.to_string()], &allowed())
                .unwrap_err()
                .to_string();
            assert!(err.contains(fix), "{tok}: {err}");
        }

        // Whole tokens only. An alternation is an ordinary pattern, and a
        // refusal here would break a call that works today.
        ok("grep", &["-E", "nginx|apache", "/etc/hosts"]);
        ok("find", &["/etc", "-name", "*>*"]);
        // And `--` still means "operands from here", so one of these characters
        // can be searched for as itself.
        ok("grep", &["--", "|", "/etc/hosts"]);
    }
}
