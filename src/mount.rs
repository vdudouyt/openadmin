//! sshfs mounting (F4).
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
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

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
pub fn mount(rec: &HostRecord, datadir: &Path, cfg: &Config) -> Result<()> {
    if rec.mount_point.trim().is_empty() {
        bail!("{} has no mount point set.", rec.name);
    }
    std::fs::create_dir_all(&rec.mount_point)
        .with_context(|| format!("create mount point {}", rec.mount_point))?;

    let args = sshfs_args(rec, datadir, cfg);
    let uses_stdin = args.iter().any(|a| a == "password_stdin");

    let mut child = Command::new("sshfs")
        .args(&args)
        .stdin(if uses_stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("run sshfs (is it installed?)")?;

    if uses_stdin && let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{}", rec.pass).context("send password to sshfs")?;
    }

    let out = child.wait_with_output().context("wait for sshfs")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!(
            "mounting {} failed: {}",
            rec.name,
            if err.is_empty() {
                "sshfs exited non-zero".to_string()
            } else {
                err
            }
        );
    }
    Ok(())
}

/// Unmount, matching qhostman's `fusermount -uz` (lazy, so a busy mount still
/// detaches).
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
    fn a_host_without_a_mount_point_is_refused_before_touching_the_disk() {
        let mut h = host();
        h.mount_point = "  ".into();
        let err = mount(&h, Path::new("/d"), &Config::default())
            .unwrap_err()
            .to_string();
        assert!(err.contains("no mount point"), "got {err}");
    }
}
