//! SSH key generation for a host (F7 / the `[gen]` cell in the KEY column).
//!
//! Keys live in `<datadir>/keys/<host>`, the layout qhostman's tools expect
//! (`sshto/main.cpp:128`, `fuse-plugin/ssh_handler.cpp:21`).

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn keys_dir(datadir: &Path) -> PathBuf {
    datadir.join("keys")
}

/// Sanitize a host nickname into a filename. Host names are free text, so
/// anything that could escape the keys directory is replaced.
pub fn key_file_name(host: &str) -> String {
    let cleaned: String = host
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('.').to_string();
    if cleaned.is_empty() {
        "host".to_string()
    } else {
        cleaned
    }
}

/// Generate an ed25519 keypair for `host`, returning `(key_name, public_key)`.
/// An existing key is reused rather than silently replaced.
pub fn generate(datadir: &Path, host: &str) -> Result<(String, String)> {
    let dir = keys_dir(datadir);
    std::fs::create_dir_all(&dir).context("create keys directory")?;
    restrict(&dir, 0o700);

    let name = key_file_name(host);
    let path = dir.join(&name);
    let pub_path = dir.join(format!("{name}.pub"));

    if !path.exists() {
        let out = Command::new("ssh-keygen")
            .args([
                "-t",
                "ed25519",
                "-N",
                "",
                "-C",
                &format!("openadmin@{host}"),
                "-f",
            ])
            .arg(&path)
            .output()
            .context("run ssh-keygen (is openssh-client installed?)")?;
        if !out.status.success() {
            bail!(
                "ssh-keygen failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        restrict(&path, 0o600);
    }

    let public = std::fs::read_to_string(&pub_path)
        .with_context(|| format!("read {}", pub_path.display()))?;
    Ok((name, public.trim().to_string()))
}

/// Read back an already-generated public key.
pub fn public_key(datadir: &Path, key_name: &str) -> Result<String> {
    let path = keys_dir(datadir).join(format!("{key_name}.pub"));
    Ok(std::fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?
        .trim()
        .to_string())
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
    fn nicknames_become_safe_filenames() {
        assert_eq!(key_file_name("web-01"), "web-01");
        assert_eq!(key_file_name("db.main"), "db.main");
        // Traversal is neutralized: separators become underscores and a name
        // that is nothing but dots is replaced outright.
        let escaped = key_file_name("../../etc/passwd");
        assert!(!escaped.contains('/'), "got {escaped}");
        assert_ne!(escaped, "..");
        assert_eq!(escaped, "_.._etc_passwd");
        assert_eq!(key_file_name(".."), "host");
        assert_eq!(key_file_name("/"), "_");
        assert_eq!(key_file_name("my host"), "my_host");
        assert_eq!(key_file_name("  "), "host");
        assert_eq!(key_file_name("..."), "host");
    }

    #[test]
    fn generates_reads_back_and_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("openadmin-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let (name, pubkey) = generate(&dir, "web-01").unwrap();
        assert_eq!(name, "web-01");
        assert!(pubkey.starts_with("ssh-ed25519 "), "got {pubkey}");
        assert!(pubkey.contains("openadmin@web-01"));
        assert_eq!(public_key(&dir, &name).unwrap(), pubkey);

        // Re-generating must not replace the key.
        let (_, again) = generate(&dir, "web-01").unwrap();
        assert_eq!(
            again, pubkey,
            "an existing key is reused, never overwritten"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(keys_dir(&dir).join("web-01"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "private keys must be owner-only");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
