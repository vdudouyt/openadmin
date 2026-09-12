//! Host record — mirrors qhostman's `DataBase::HostRecord`
//! (`/root/qhostman/gui/database.h`) field for field, plus the columns the
//! OpenAdmin design needs.

/// The protocols the Type field cycles through (`data.js:5`).
pub const HOST_TYPES: [&str; 2] = ["SSH", "FTP"];

/// Default port per protocol (`data.js:6`).
pub fn default_port(proto: &str) -> i64 {
    match proto.to_ascii_uppercase().as_str() {
        "FTP" => 21,
        _ => 22,
    }
}

/// Derive a mount point from the host nickname.
///
/// The mockup lowercases and hyphenates the whole name (`data.js:9`), which is
/// what we follow; qhostman instead stripped the first dotted component
/// (`edithostdialog.cpp:predictMountPoint`).
pub fn mount_for(prefix: &str, name: &str) -> String {
    let slug: String = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    format!("{}/{}", prefix.trim_end_matches('/'), slug)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostRecord {
    pub id: i64,
    pub name: String,
    pub proto: String,
    pub addr: String,
    pub mount_point: String,
    pub port: i64,
    pub login: String,
    pub pass: String,
    /// Filename under `<datadir>/keys/`. qhostman's `save()` dropped this on
    /// every edit; we persist it.
    pub key_name: String,
    /// Passphrase for the key above, fed to ssh the same way a password is.
    pub key_value: String,
    /// Route other connections through this host (`ProxyCommand`). Added by our
    /// additive migration; qhostman kept it as runtime-only plugin state.
    pub proxy: bool,
    /// Virtual field: `mount_point` is present in `/etc/mtab`. Never persisted.
    pub mounted: bool,
}

impl HostRecord {
    /// The ssh target host, cutting at the first `:` so an `addr` of
    /// `host:/remote/path` (the sshfs form) still yields a bare ssh host.
    /// Mirrors `stripAddr()` in `/root/qhostman/sshto/main.cpp`.
    pub fn ssh_host(&self) -> &str {
        match self.addr.find(':') {
            Some(i) => &self.addr[..i],
            None => &self.addr,
        }
    }

    /// `login@host`, or bare host when no login is set.
    pub fn ssh_target(&self) -> String {
        if self.login.is_empty() {
            self.ssh_host().to_string()
        } else {
            format!("{}@{}", self.login, self.ssh_host())
        }
    }

    /// The sshfs remote spec: qhostman appends `:/` when `addr` carries no path
    /// (`fuse-plugin/ssh_handler.cpp`).
    pub fn sshfs_remote(&self) -> String {
        let addr = if self.addr.contains(':') {
            self.addr.clone()
        } else {
            format!("{}:/", self.addr)
        };
        if self.login.is_empty() {
            addr
        } else {
            format!("{}@{}", self.login, addr)
        }
    }

    /// The secret ssh should authenticate with: a key passphrase when a key is
    /// configured, otherwise the password (`sshto/main.cpp:95-99`).
    pub fn secret(&self) -> &str {
        if self.key_name.is_empty() {
            &self.pass
        } else {
            &self.key_value
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_point_slugifies_the_nickname() {
        assert_eq!(mount_for("/net", "web-01"), "/net/web-01");
        assert_eq!(mount_for("/net", "Build Rig"), "/net/build-rig");
        assert_eq!(mount_for("/net/", "  NAS  "), "/net/nas");
    }

    #[test]
    fn addr_with_a_path_still_yields_a_bare_ssh_host() {
        let r = HostRecord {
            addr: "files.example.com:/srv/data".into(),
            login: "deploy".into(),
            ..Default::default()
        };
        assert_eq!(r.ssh_host(), "files.example.com");
        assert_eq!(r.ssh_target(), "deploy@files.example.com");
        assert_eq!(r.sshfs_remote(), "deploy@files.example.com:/srv/data");
    }

    #[test]
    fn sshfs_remote_appends_root_when_addr_has_no_path() {
        let r = HostRecord {
            addr: "10.0.0.1".into(),
            login: "root".into(),
            ..Default::default()
        };
        assert_eq!(r.sshfs_remote(), "root@10.0.0.1:/");
    }

    #[test]
    fn secret_prefers_the_key_passphrase() {
        let mut r = HostRecord {
            pass: "pw".into(),
            ..Default::default()
        };
        assert_eq!(r.secret(), "pw");
        r.key_name = "web-01".into();
        r.key_value = "phrase".into();
        assert_eq!(r.secret(), "phrase");
    }

    #[test]
    fn default_ports_follow_the_protocol() {
        assert_eq!(default_port("SSH"), 22);
        assert_eq!(default_port("ftp"), 21);
    }
}
