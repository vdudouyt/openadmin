//! Files the operator has staged for upload, in `<datadir>/artifacts/`.
//!
//! The model both reads this list and later names an entry in a plan, so a name
//! it supplies is untrusted input that ends up as a local path. Two ways out of
//! the directory have to be closed, and only the second is obvious:
//!
//! * `../../keys/web-01` — a relative escape in the name itself.
//! * an artifact that *is* a symlink to somewhere else — the name is innocent,
//!   the target is not.
//!
//! Canonicalizing and then checking containment closes both, because
//! `canonicalize` resolves `..` and follows symlinks before the check.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub name: String,
    pub size: u64,
}

pub fn dir(datadir: &Path) -> PathBuf {
    datadir.join("artifacts")
}

/// Create the directory if it is missing, owner-only like `keys/`.
#[allow(dead_code)] // used by the plan executor, next commit
pub fn ensure_dir(datadir: &Path) -> Result<PathBuf> {
    let d = dir(datadir);
    std::fs::create_dir_all(&d).with_context(|| format!("create {}", d.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
    }
    Ok(d)
}

/// Resolve an artifact name to a real file inside the artifacts directory.
///
/// Returns an error rather than a path for anything that escapes, so a caller
/// cannot forget to check.
pub fn resolve(datadir: &Path, name: &str) -> Result<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        bail!("no artifact named");
    }
    // An artifact is one flat file name, never a path.
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        bail!("artifact name must be a plain file name, not a path: {name:?}");
    }
    if name == "." || name == ".." {
        bail!("artifact name must be a plain file name: {name:?}");
    }

    let base = dir(datadir);
    let base = base
        .canonicalize()
        .with_context(|| format!("no artifacts directory at {}", base.display()))?;
    let candidate = base.join(name);
    // Resolves `..` and follows symlinks, so the containment check below sees
    // where the file really is rather than where it claims to be.
    let real = candidate
        .canonicalize()
        .with_context(|| format!("no artifact named {name:?}"))?;

    if !real.starts_with(&base) {
        bail!("artifact {name:?} resolves outside the artifacts directory");
    }
    if !real.is_file() {
        bail!("artifact {name:?} is not a regular file");
    }
    Ok(real)
}

/// Every usable artifact, sorted by name.
///
/// Entries that are not regular files, or that lead outside the directory, are
/// simply not listed — the model never learns they exist.
pub fn list(datadir: &Path) -> Result<Vec<Artifact>> {
    let d = dir(datadir);
    if !d.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&d).with_context(|| format!("read {}", d.display()))? {
        let Ok(entry) = entry else { continue };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        // Reuse the same gate the upload path uses, so the list can never
        // advertise something that would later be refused.
        let Ok(real) = resolve(datadir, &name) else {
            continue;
        };
        let size = real.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(Artifact { name, size });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("openadmin-artifacts-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn lists_regular_files_by_name() {
        let data = scratch("list");
        let a = ensure_dir(&data).unwrap();
        write(&a, "zeta.sh", "z");
        write(&a, "alpha.conf", "hello");
        std::fs::create_dir(a.join("a-directory")).unwrap();

        let found = list(&data).unwrap();
        assert_eq!(found.len(), 2, "directories are not artifacts: {found:?}");
        assert_eq!(found[0].name, "alpha.conf");
        assert_eq!(found[0].size, 5);
        assert_eq!(found[1].name, "zeta.sh");
    }

    #[test]
    fn a_missing_directory_lists_nothing_rather_than_failing() {
        let data = scratch("empty");
        assert!(list(&data).unwrap().is_empty());
    }

    #[test]
    fn a_name_is_resolved_inside_the_directory() {
        let data = scratch("resolve");
        let a = ensure_dir(&data).unwrap();
        write(&a, "hotfix.sh", "#!/bin/sh\n");
        let p = resolve(&data, "hotfix.sh").unwrap();
        assert!(p.starts_with(a.canonicalize().unwrap()));
        assert!(p.is_file());
        // Surrounding whitespace is not a different artifact.
        assert_eq!(resolve(&data, "  hotfix.sh  ").unwrap(), p);
    }

    /// The obvious escape.
    #[test]
    fn a_relative_path_cannot_escape() {
        let data = scratch("traverse");
        ensure_dir(&data).unwrap();
        std::fs::create_dir_all(data.join("keys")).unwrap();
        std::fs::write(data.join("keys/web-01"), "PRIVATE KEY").unwrap();

        for evil in [
            "../keys/web-01",
            "../../etc/passwd",
            "/etc/passwd",
            "..",
            ".",
            "",
            "sub/../../keys/web-01",
        ] {
            let e = resolve(&data, evil)
                .map(|p| format!("resolved to {}", p.display()))
                .unwrap_err()
                .to_string();
            assert!(
                e.contains("plain file name") || e.contains("no artifact"),
                "{evil:?} was not refused cleanly: {e}"
            );
        }
    }

    /// The non-obvious escape: the name is innocent, the target is not. This is
    /// why the check is on the canonicalized path, not the joined one.
    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_directory_is_refused() {
        let data = scratch("symlink");
        let a = ensure_dir(&data).unwrap();
        std::fs::create_dir_all(data.join("keys")).unwrap();
        std::fs::write(data.join("keys/web-01"), "PRIVATE KEY").unwrap();
        std::os::unix::fs::symlink(data.join("keys/web-01"), a.join("innocent.txt")).unwrap();

        let e = resolve(&data, "innocent.txt").unwrap_err().to_string();
        assert!(e.contains("outside the artifacts directory"), "{e}");

        // And it is not even advertised.
        let found = list(&data).unwrap();
        assert!(
            found.iter().all(|f| f.name != "innocent.txt"),
            "a symlink out of the directory must not be listed: {found:?}"
        );
    }

    /// A symlink that stays inside is fine — it is a normal way to stage a file.
    #[cfg(unix)]
    #[test]
    fn a_symlink_within_the_directory_is_fine() {
        let data = scratch("symlink-ok");
        let a = ensure_dir(&data).unwrap();
        write(&a, "real.conf", "body");
        std::os::unix::fs::symlink(a.join("real.conf"), a.join("alias.conf")).unwrap();
        assert!(resolve(&data, "alias.conf").is_ok());
    }

    #[test]
    fn a_directory_is_not_an_artifact() {
        let data = scratch("dir");
        let a = ensure_dir(&data).unwrap();
        std::fs::create_dir(a.join("stuff")).unwrap();
        let e = resolve(&data, "stuff").unwrap_err().to_string();
        assert!(e.contains("not a regular file"), "{e}");
    }
}
