//! sshfs mounting (F9, `m`, `u` on the Hosts screen).
//!
//! qhostman nests `sshpass -> qhostman-sshfs-wrapper -> sshfs` purely to keep
//! SIGHUP away from sshfs's ssh when the session leader exits
//! (`/root/qhostman/sshfs-wrapper/main.cpp`). We need none of that: mounting is
//! a detached background process here, not a foreground PTY job, and sshfs can
//! take the password on stdin — so the secret never reaches the environment or
//! the argv either.

use crate::config::Config;
use crate::db::model::HostRecord;
use crate::ssh;
use anyhow::{Context, Result, bail};
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How a mount attempt ended, when it did not fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Mounted,
    /// The operator cancelled while sshfs was still connecting.
    Cancelled,
}

/// Build the sshfs argv for `rec`. Split out so it can be asserted on without
/// touching the filesystem.
pub fn sshfs_args(rec: &HostRecord, datadir: &Path, cfg: &Config) -> Vec<String> {
    let mut args = vec![rec.sshfs_remote(), rec.mount_point.clone()];
    args.push("-p".into());
    args.push(rec.port.to_string());
    args.push("-o".into());
    args.push("StrictHostKeyChecking=accept-new".into());
    if !rec.key_name.is_empty() {
        args.push("-o".into());
        args.push(format!(
            "IdentityFile={}",
            ssh::key_path(datadir, &rec.key_name).display()
        ));
    } else if !rec.pass.is_empty() {
        // Read the password from stdin instead of a tty.
        args.push("-o".into());
        args.push("password_stdin".into());
    }
    args.extend(cfg.sshfs_options.iter().cloned());
    args
}

/// Mount `rec` at its mount point. Blocks until sshfs has finished connecting,
/// which is fast and lets us report a real error instead of a silent failure.
/// Mount `rec`, giving up as soon as `cancel` is raised.
///
/// sshfs used to be run with `wait_with_output` on the UI thread, which froze
/// the screen for as long as it took to connect — and with no `ConnectTimeout`
/// an unreachable host takes as long as the kernel's TCP timeout, often a couple
/// of minutes, with nothing the operator could do but wait. So this polls
/// instead, and a raised flag kills the attempt.
pub fn mount(
    rec: &HostRecord,
    datadir: &Path,
    cfg: &Config,
    cancel: &AtomicBool,
) -> Result<Outcome> {
    if rec.mount_point.trim().is_empty() {
        bail!("{} has no mount point set.", rec.name);
    }
    std::fs::create_dir_all(&rec.mount_point)
        .with_context(|| format!("create mount point {}", rec.mount_point))?;

    let args = sshfs_args(rec, datadir, cfg);
    let uses_stdin = args.iter().any(|a| a == "password_stdin");
    let mut cmd = Command::new("sshfs");
    cmd.args(&args);
    let secret = uses_stdin.then(|| format!("{}\n", rec.pass));

    match run_cancellable(cmd, secret.as_deref(), cancel).context("run sshfs (is it installed?)")? {
        Finished::Cancelled => Ok(Outcome::Cancelled),
        Finished::Exited { success: true, .. } => Ok(Outcome::Mounted),
        Finished::Exited { stderr, .. } => bail!(
            "mounting {} failed: {}",
            rec.name,
            if stderr.is_empty() {
                "sshfs exited non-zero".to_string()
            } else {
                stderr
            }
        ),
    }
}

/// What `run_cancellable` saw.
#[derive(Debug)]
enum Finished {
    Exited { success: bool, stderr: String },
    Cancelled,
}

/// Run `cmd` to completion unless `cancel` is raised first.
///
/// Its own process group, so a cancel takes down everything it started: sshfs
/// runs `ssh` as a child, and killing sshfs alone leaves that `ssh` blocked on
/// the network until its own timeout. The daemon a *successful* sshfs leaves
/// behind is not caught by this, because FUSE daemonizes with `setsid()` and
/// leaves the group — so a cancel can never tear down a mount that finished.
///
/// stderr is read only after a failed exit. On success sshfs forks that daemon,
/// and reading a pipe the daemon might still hold is how a successful mount
/// would hang the reader.
fn run_cancellable(mut cmd: Command, stdin: Option<&str>, cancel: &AtomicBool) -> Result<Finished> {
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .process_group(0);
    let mut child = cmd.spawn()?;

    if let Some(data) = stdin
        && let Some(mut pipe) = child.stdin.take()
    {
        // Dropped at the end of this block, which is the EOF sshfs waits for.
        pipe.write_all(data.as_bytes())
            .context("send password to sshfs")?;
    }

    loop {
        if let Some(status) = child.try_wait()? {
            let stderr = if status.success() {
                String::new()
            } else {
                read_stderr(&mut child)
            };
            return Ok(Finished::Exited {
                success: status.success(),
                stderr,
            });
        }
        if cancel.load(Ordering::Acquire) {
            kill_group(child.id());
            let _ = child.wait();
            return Ok(Finished::Cancelled);
        }
        // Short enough that Cancel feels immediate, long enough not to spin.
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn read_stderr(child: &mut Child) -> String {
    let mut s = String::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut s);
    }
    s.trim().to_string()
}

/// SIGTERM so ssh can close its connection, then SIGKILL for anything that
/// ignored it. The same sequence as the agent's command timeout.
fn kill_group(pid: u32) {
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
    std::thread::sleep(Duration::from_millis(150));
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

pub fn unmount(rec: &HostRecord) -> Result<()> {
    let out = Command::new("fusermount")
        .args(["-uz", &rec.mount_point])
        .output()
        .context("run fusermount")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!(
            "unmounting {} failed: {}",
            rec.name,
            if err.is_empty() {
                "fusermount exited non-zero".to_string()
            } else {
                err
            }
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> HostRecord {
        HostRecord {
            name: "nas".into(),
            addr: "192.168.1.240".into(),
            mount_point: "/net/nas".into(),
            port: 2222,
            login: "media".into(),
            pass: "nasnas".into(),
            ..Default::default()
        }
    }

    #[test]
    fn password_hosts_read_the_secret_from_stdin_not_argv() {
        let args = sshfs_args(&host(), Path::new("/d"), &Config::default());
        assert_eq!(args[0], "media@192.168.1.240:/");
        assert_eq!(args[1], "/net/nas");
        assert!(args.windows(2).any(|w| w == ["-p", "2222"]));
        assert!(args.iter().any(|a| a == "password_stdin"));
        assert!(
            !args.iter().any(|a| a.contains("nasnas")),
            "secret must not reach argv"
        );
    }

    #[test]
    fn key_hosts_use_an_identity_file_and_no_stdin_password() {
        let mut h = host();
        h.key_name = "nas".into();
        let args = sshfs_args(&h, Path::new("/data"), &Config::default());
        assert!(args.iter().any(|a| a == "IdentityFile=/data/keys/nas"));
        assert!(!args.iter().any(|a| a == "password_stdin"));
    }

    #[test]
    fn an_addr_with_a_path_is_passed_through_untouched() {
        let mut h = host();
        h.addr = "files:/srv/data".into();
        assert_eq!(
            sshfs_args(&h, Path::new("/d"), &Config::default())[0],
            "media@files:/srv/data"
        );
    }

    #[test]
    fn configured_options_are_appended() {
        let cfg = Config {
            sshfs_options: vec!["-o".into(), "reconnect".into()],
            ..Config::default()
        };
        let args = sshfs_args(&host(), Path::new("/d"), &cfg);
        assert_eq!(
            &args[args.len() - 2..],
            &["-o".to_string(), "reconnect".to_string()]
        );
    }

    #[test]
    fn a_cancelled_run_stops_promptly_and_takes_its_children_with_it() {
        // A shell that starts a grandchild and waits: the shape of sshfs running
        // ssh. Killing only the direct child would leave the sleep behind.
        let pidfile =
            std::env::temp_dir().join(format!("openadmin-mnt-{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&pidfile);
        let mut cmd = Command::new("/bin/sh");
        cmd.args([
            "-c",
            &format!("sleep 30 & echo $! > {}; wait", pidfile.display()),
        ]);
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&cancel);
        let started = std::time::Instant::now();
        let worker = std::thread::spawn(move || run_cancellable(cmd, None, &flag).unwrap());

        // Let it start, then cancel.
        for _ in 0..100 {
            if pidfile.exists()
                && std::fs::read_to_string(&pidfile).is_ok_and(|s| !s.trim().is_empty())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        cancel.store(true, Ordering::Release);
        let finished = worker.join().unwrap();
        assert!(matches!(finished, Finished::Cancelled), "{finished:?}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "cancelled promptly"
        );

        let grandchild: i32 = std::fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        // kill(pid, 0) fails once the process is gone.
        let alive = unsafe { libc::kill(grandchild, 0) } == 0;
        assert!(!alive, "the grandchild went with the group");
        let _ = std::fs::remove_file(&pidfile);
    }

    #[test]
    fn a_failed_run_reports_its_stderr_and_an_uncancelled_one_is_left_alone() {
        let never = AtomicBool::new(false);
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "echo 'read: Connection reset by peer' >&2; exit 1"]);
        match run_cancellable(cmd, None, &never).unwrap() {
            Finished::Exited { success, stderr } => {
                assert!(!success);
                assert_eq!(stderr, "read: Connection reset by peer");
            }
            other => panic!("{other:?}"),
        }

        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "exit 0"]);
        assert!(matches!(
            run_cancellable(cmd, None, &never).unwrap(),
            Finished::Exited { success: true, .. }
        ));
    }

    #[test]
    fn stdin_is_delivered_and_closed() {
        // `read` only returns at a newline or EOF; the password path depends on
        // the pipe being closed after it.
        let never = AtomicBool::new(false);
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "read line; [ \"$line\" = hunter2 ] || exit 3"]);
        assert!(matches!(
            run_cancellable(cmd, Some("hunter2\n"), &never).unwrap(),
            Finished::Exited { success: true, .. }
        ));
    }

    #[test]
    fn a_host_without_a_mount_point_is_refused_before_touching_the_disk() {
        let mut h = host();
        h.mount_point = "  ".into();
        let err = mount(
            &h,
            Path::new("/d"),
            &Config::default(),
            &AtomicBool::new(false),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("no mount point"), "got {err}");
    }
}
