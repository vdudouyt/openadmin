//! `<datadir>/config.toml`. Modeled on `/root/cfdns/src/config.rs`, including
//! its owner-only permissions.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// `TERM` handed to spawned sessions.
    pub term: String,
    /// Scrollback lines retained per terminal session.
    pub scrollback: usize,
    /// Extra options appended to every `sshfs` invocation.
    pub sshfs_options: Vec<String>,
    /// Where auto-derived mount points live.
    pub mount_prefix: String,
    /// Local SOCKS port opened on the host flagged as proxy.
    pub proxy_port: u16,
    /// Shown in the Chat screen's panel label.
    pub model: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            term: "xterm-256color".to_string(),
            scrollback: 5000,
            sshfs_options: Vec::new(),
            mount_prefix: "/net".to_string(),
            proxy_port: 10000,
            model: "claude-sonnet-4.5".to_string(),
        }
    }
}

impl Config {
    pub fn path(datadir: &Path) -> PathBuf {
        datadir.join("config.toml")
    }

    /// Read the config, falling back to defaults when it is absent. A malformed
    /// file is an error the caller surfaces rather than silently overwriting.
    pub fn load(datadir: &Path) -> Result<Config> {
        let path = Config::path(datadir);
        if !path.exists() {
            return Ok(Config::default());
        }
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }

    pub fn save(&self, datadir: &Path) -> Result<()> {
        std::fs::create_dir_all(datadir).context("create data directory")?;
        let path = Config::path(datadir);
        std::fs::write(&path, toml::to_string_pretty(self)?)
            .with_context(|| format!("write {}", path.display()))?;
        restrict(&path, 0o600);
        restrict(datadir, 0o700);
        Ok(())
    }
}

fn restrict(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_defaults() {
        let dir = std::env::temp_dir().join(format!("openadmin-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = Config::load(&dir).unwrap();
        assert_eq!(cfg.scrollback, 5000);
        assert_eq!(cfg.mount_prefix, "/net");
    }

    #[test]
    fn roundtrips_through_disk() {
        let dir = std::env::temp_dir().join(format!("openadmin-cfg-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = Config {
            scrollback: 42,
            sshfs_options: vec!["-o".into(), "reconnect".into()],
            ..Config::default()
        };
        cfg.save(&dir).unwrap();
        let back = Config::load(&dir).unwrap();
        assert_eq!(back.scrollback, 42);
        assert_eq!(back.sshfs_options, vec!["-o", "reconnect"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A partial config must fill the rest from Default, so adding a field
    /// never breaks an existing file.
    #[test]
    fn partial_file_fills_defaults() {
        let cfg: Config = toml::from_str("scrollback = 10\n").unwrap();
        assert_eq!(cfg.scrollback, 10);
        assert_eq!(cfg.term, "xterm-256color");
        assert_eq!(cfg.mount_prefix, "/net");
    }
}
