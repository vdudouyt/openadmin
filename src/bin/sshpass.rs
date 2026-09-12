//! `openadmin-sshpass` — supplies an SSH secret non-interactively.
//!
//! Two modes in one binary:
//!
//!   1. **Wrapper** — `openadmin-sshpass -- ssh -p 22 user@host …`
//!      Points `SSH_ASKPASS` at this same executable, forces ssh to use it even
//!      though a tty is present, then `exec`s the command in place.
//!
//!   2. **Askpass** — ssh calls this binary back with the prompt as argv[1].
//!      It prints the secret from `$SSHPASS` on stdout and exits.
//!
//! This deliberately departs from qhostman's `sshpass`
//! (`/root/qhostman/sshpass/main.cpp`), which opened its own PTY and
//! screen-scraped byte-by-byte for `" password:"`. That design existed because
//! `sshto` ran in the user's real terminal. OpenAdmin already owns a PTY per
//! session, so an inner PTY would stack a second terminal emulator between
//! programs like mc and the screen — exactly the fragility to avoid.
//! `SSH_ASKPASS_REQUIRE=force` needs OpenSSH >= 8.4.

use std::os::unix::process::CommandExt;
use std::process::Command;

/// The environment variable carrying the secret, matching qhostman's
/// `SSHPASS=` convention.
const SECRET_VAR: &str = "SSHPASS";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("--") => wrapper(&args[1..]),
        Some("--help") | Some("-h") | None => {
            eprintln!(
                "usage: openadmin-sshpass -- <command> [args...]\n\
                 \n\
                 Runs <command> with SSH_ASKPASS pointed at this binary, so ssh\n\
                 takes its password from the SSHPASS environment variable\n\
                 instead of prompting. Called back by ssh as the askpass helper\n\
                 when given a prompt argument."
            );
            std::process::exit(if args.is_empty() { 2 } else { 0 });
        }
        // Anything else is ssh invoking us as the askpass helper, passing the
        // prompt text as the sole argument.
        Some(prompt) => askpass(prompt),
    }
}

/// Wrapper mode: set up the askpass environment and exec the real command.
fn wrapper(cmd: &[String]) -> ! {
    let Some((program, rest)) = cmd.split_first() else {
        eprintln!("openadmin-sshpass: nothing to run after `--`");
        std::process::exit(2);
    };

    let self_path = std::env::current_exe().unwrap_or_else(|e| {
        eprintln!("openadmin-sshpass: cannot locate own executable: {e}");
        std::process::exit(2);
    });

    let mut command = Command::new(program);
    command
        .args(rest)
        .env("SSH_ASKPASS", &self_path)
        // Without `force`, ssh only consults askpass when there is no tty —
        // and we always run inside one.
        .env("SSH_ASKPASS_REQUIRE", "force");

    // `exec` replaces this process, so the child keeps our pty, pgid and fds.
    let err = command.exec();
    eprintln!("openadmin-sshpass: cannot run {program}: {err}");
    // 127 is the conventional "command not found" exit status.
    std::process::exit(127);
}

/// Askpass mode: answer the prompt on stdout.
fn askpass(prompt: &str) -> ! {
    // With StrictHostKeyChecking=accept-new an unknown host is added silently,
    // but a prompt can still reach us (a changed key, or an explicit
    // StrictHostKeyChecking=ask). Confirming is the same answer qhostman's
    // parser gave.
    if is_confirmation(prompt) {
        println!("yes");
        std::process::exit(0);
    }

    match std::env::var(SECRET_VAR) {
        Ok(secret) if !secret.is_empty() => {
            println!("{secret}");
            std::process::exit(0);
        }
        // No secret to offer. Exiting non-zero makes ssh treat the prompt as
        // unanswered and fail, rather than retrying with an empty password.
        _ => {
            eprintln!("openadmin-sshpass: {SECRET_VAR} is not set; cannot answer: {prompt}");
            std::process::exit(1);
        }
    }
}

/// Does this prompt want a yes/no confirmation rather than a secret?
fn is_confirmation(prompt: &str) -> bool {
    let p = prompt.to_ascii_lowercase();
    p.contains("continue connecting") || p.contains("(yes/no")
}

#[cfg(test)]
mod tests {
    use super::is_confirmation;

    #[test]
    fn recognizes_host_key_confirmation_prompts() {
        assert!(is_confirmation(
            "The authenticity of host '10.0.0.1' can't be established.\n\
             Are you sure you want to continue connecting (yes/no/[fingerprint])? "
        ));
        assert!(is_confirmation("Continue connecting (yes/no)? "));
    }

    #[test]
    fn password_and_passphrase_prompts_are_not_confirmations() {
        assert!(!is_confirmation("root@10.0.0.1's password: "));
        assert!(!is_confirmation(
            "Enter passphrase for key '/root/.openadmin/keys/web-01': "
        ));
    }
}
