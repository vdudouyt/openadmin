//! Building the command line that connects to a host.
//!
//! Follows qhostman's `sshto` (`/root/qhostman/sshto/main.cpp:80-110`): the
//! secret travels out-of-band in `SSHPASS` and `openadmin-sshpass` hands it to
//! ssh, so it never appears in the argv any other user could read.

use crate::config::Config;
use crate::db::model::HostRecord;
use std::path::{Path, PathBuf};

/// A command ready to be spawned on a PTY.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    /// Extra environment. Carries the secret, so never log it.
    pub env: Vec<(String, String)>,
}

/// Locate the `openadmin-sshpass` helper. It normally sits beside the running
/// binary (cargo's target dir, or an install prefix); fall back to `PATH`.
pub fn helper_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join("openadmin-sshpass");
        if sibling.is_file() {
            return sibling;
        }
    }
    PathBuf::from("openadmin-sshpass")
}

/// Path to a host's private key inside the data directory.
pub fn key_path(datadir: &Path, key_name: &str) -> PathBuf {
    datadir.join("keys").join(key_name)
}

/// The `ssh` command for an interactive shell on `rec`.
///
/// `proxy` is the host currently flagged as the SOCKS gateway, if any and if it
/// is not `rec` itself.
pub fn shell_command(
    rec: &HostRecord,
    datadir: &Path,
    cfg: &Config,
    proxy: Option<&HostRecord>,
) -> Launch {
    let mut args: Vec<String> = vec![
        "--".into(),
        "ssh".into(),
        rec.ssh_target(),
        "-p".into(),
        rec.port.to_string(),
        // Unknown hosts are added silently; a *changed* key still stops us.
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
    ];

    if !rec.key_name.is_empty() {
        args.push("-i".into());
        args.push(key_path(datadir, &rec.key_name).display().to_string());
    }

    if let Some(p) = proxy.filter(|p| p.id != rec.id) {
        // qhostman's proxy plugin shape: netcat in SOCKS5 mode against the
        // tunnel the gateway host opened (`proxy_plugin.cpp`).
        let _ = p;
        args.push("-o".into());
        args.push(format!(
            "ProxyCommand=nc -X 5 -x 127.0.0.1:{} %h %p",
            cfg.proxy_port
        ));
    }

    Launch {
        program: helper_path().display().to_string(),
        args,
        env: secret_env(rec),
    }
}

/// The `-D <port>` tunnel that makes a host act as the SOCKS gateway.
pub fn proxy_tunnel_command(rec: &HostRecord, datadir: &Path, cfg: &Config) -> Launch {
    let mut args: Vec<String> = vec![
        "--".into(),
        "ssh".into(),
        "-N".into(),
        "-D".into(),
        cfg.proxy_port.to_string(),
        rec.ssh_target(),
        "-p".into(),
        rec.port.to_string(),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
    ];
    if !rec.key_name.is_empty() {
        args.push("-i".into());
        args.push(key_path(datadir, &rec.key_name).display().to_string());
    }
    Launch {
        program: helper_path().display().to_string(),
        args,
        env: secret_env(rec),
    }
}

fn secret_env(rec: &HostRecord) -> Vec<(String, String)> {
    let secret = rec.secret();
    if secret.is_empty() {
        Vec::new()
    } else {
        vec![("SSHPASS".to_string(), secret.to_string())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> HostRecord {
        HostRecord {
            id: 1,
            name: "web-01".into(),
            proto: "ssh".into(),
            addr: "10.0.4.11".into(),
            port: 2222,
            login: "deploy".into(),
            pass: "hunter2".into(),
            ..Default::default()
        }
    }

    #[test]
    fn builds_a_password_shell_command() {
        let cfg = Config::default();
        let l = shell_command(&host(), Path::new("/root/.openadmin"), &cfg, None);
        assert_eq!(l.args[0], "--");
        assert_eq!(l.args[1], "ssh");
        assert_eq!(l.args[2], "deploy@10.0.4.11");
        assert!(l.args.windows(2).any(|w| w == ["-p", "2222"]));
        assert_eq!(l.env, vec![("SSHPASS".to_string(), "hunter2".to_string())]);
        assert!(
            !l.args.iter().any(|a| a.contains("hunter2")),
            "secret must not reach argv"
        );
    }

    #[test]
    fn a_configured_key_adds_dash_i_and_sends_the_passphrase() {
        let mut h = host();
        h.key_name = "web-01".into();
        h.key_value = "phrase".into();
        let l = shell_command(&h, Path::new("/data"), &Config::default(), None);
        assert!(l.args.windows(2).any(|w| w == ["-i", "/data/keys/web-01"]));
        assert_eq!(
            l.env[0].1, "phrase",
            "key passphrase wins over the password"
        );
    }

    #[test]
    fn proxy_host_injects_a_socks_proxycommand_but_never_for_itself() {
        let cfg = Config::default();
        let gw = HostRecord {
            id: 9,
            name: "bastion".into(),
            ..Default::default()
        };
        let l = shell_command(&host(), Path::new("/d"), &cfg, Some(&gw));
        assert!(
            l.args
                .iter()
                .any(|a| a == "ProxyCommand=nc -X 5 -x 127.0.0.1:10000 %h %p"),
            "expected a SOCKS ProxyCommand, got {:?}",
            l.args
        );

        // Connecting to the gateway itself must not route through itself.
        let l = shell_command(&gw, Path::new("/d"), &cfg, Some(&gw));
        assert!(!l.args.iter().any(|a| a.starts_with("ProxyCommand")));
    }

    #[test]
    fn a_host_without_a_secret_sets_no_env() {
        let mut h = host();
        h.pass.clear();
        assert!(
            shell_command(&h, Path::new("/d"), &Config::default(), None)
                .env
                .is_empty()
        );
    }

    #[test]
    fn proxy_tunnel_opens_a_dynamic_forward() {
        let l = proxy_tunnel_command(&host(), Path::new("/d"), &Config::default());
        assert!(l.args.windows(2).any(|w| w == ["-D", "10000"]));
        assert!(l.args.iter().any(|a| a == "-N"));
    }
}
