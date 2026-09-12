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
//!
//! A name may be a path into a subdirectory — `nginx/site.conf` — because
//! operators stage files the way the files are organised, and a flat directory
//! makes them rename everything to use it. That is why the containment check has
//! to be the real boundary rather than a second belt behind a ban on `/`: the
//! ban is gone and `canonicalize` is all that stands between a name and the rest
//! of the data directory, which is what it was already doing for symlinks.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    /// Relative to the artifacts directory, `/`-separated: `nginx/site.conf`.
    pub name: String,
    pub size: u64,
}

/// What a scan found, and whether it stopped early.
///
/// The list is read into the model's context, so a deep tree has to be bounded —
/// but silently returning some of it would let the model conclude a file is not
/// staged when it is. `truncated` is the difference between a short list and a
/// wrong one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub items: Vec<Artifact>,
    pub truncated: bool,
}

/// How deep a scan goes, and how many files it will name.
///
/// An operator who stages a checkout rather than a file should get a bounded
/// list rather than a flooded context. Anything past these is still uploadable —
/// `resolve` does not consult this — it just is not advertised.
const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 200;

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

/// Resolve an artifact name to `(real file, canonical relative name)`.
///
/// Returns an error rather than a path for anything that escapes, so a caller
/// cannot forget to check. The one entry point, so there is no second, laxer one
/// for a caller to reach for by mistake.
///
/// The relative name is what the upload step needs: the path inside the
/// artifacts directory, rebuilt from the *canonicalized* path rather than from
/// what the model typed. So `./nginx//site.conf` and `nginx/site.conf` give the
/// same relative name, and by the time there is one, a name that pointed
/// somewhere else has already been refused.
pub fn staged(datadir: &Path, name: &str) -> Result<(PathBuf, String)> {
    let name = name.trim();
    if name.is_empty() {
        bail!("no artifact named");
    }
    if name.contains('\\') || name.contains('\0') {
        bail!("artifact name must be a relative path with no backslash: {name:?}");
    }
    // Component-wise rather than by substring, so `..` is refused wherever it
    // appears and `a..b.conf` — an ordinary file name — is not. A leading `./`
    // is allowed because it means nothing and refusing it would cost a round
    // trip; `canonicalize` below resolves it either way.
    for c in Path::new(name).components() {
        match c {
            std::path::Component::Normal(_) | std::path::Component::CurDir => {}
            _ => bail!(
                "artifact name must be a path relative to the artifacts directory, with no \
                 `..` and no leading `/`: {name:?}"
            ),
        }
    }

    let base = dir(datadir);
    let base = base
        .canonicalize()
        .with_context(|| format!("no artifacts directory at {}", base.display()))?;
    let candidate = base.join(name);
    // Resolves `..` and follows symlinks, so the containment check below sees
    // where the file really is rather than where it claims to be. With
    // subdirectories permitted this is the whole boundary, including for a
    // *directory* symlink that leads out — `link/web-01` resolves through it and
    // is caught here.
    let real = candidate
        .canonicalize()
        .with_context(|| format!("no artifact named {name:?}"))?;

    if !real.starts_with(&base) {
        bail!("artifact {name:?} resolves outside the artifacts directory");
    }
    if !real.is_file() {
        bail!("artifact {name:?} is not a regular file");
    }

    let rel = real
        .strip_prefix(&base)
        .expect("checked by starts_with")
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    Ok((real, rel))
}

/// Every usable artifact, recursively, sorted by name.
///
/// Entries that are not regular files, or that lead outside the directory, are
/// simply not listed — the model never learns they exist.
///
/// Recursive because operators stage files in the shape the files have:
/// `nginx/site.conf` next to `postgres/pg_hba.conf`. A flat scan named neither
/// and gave the model an empty list beside a directory full of work.
pub fn list(datadir: &Path) -> Result<Listing> {
    let d = dir(datadir);
    if !d.exists() {
        return Ok(Listing {
            items: Vec::new(),
            truncated: false,
        });
    }
    let mut items = Vec::new();
    // One over the cap, so a full list and a truncated one are distinguishable
    // without walking the rest of the tree.
    let stopped = walk(datadir, &d, &mut Vec::new(), 0, &mut items)?;
    items.sort_by(|a, b| a.name.cmp(&b.name));
    let truncated = stopped || items.len() > MAX_ENTRIES;
    items.truncate(MAX_ENTRIES);
    Ok(Listing { items, truncated })
}

/// One directory, then the ones inside it. `Ok(true)` means the walk stopped on
/// a limit rather than running out of files.
///
/// Entries are sorted before descending so that a truncated list is the same
/// list every time; `read_dir` order is whatever the filesystem says.
fn walk(
    datadir: &Path,
    here: &Path,
    prefix: &mut Vec<String>,
    depth: usize,
    out: &mut Vec<Artifact>,
) -> Result<bool> {
    if depth > MAX_DEPTH {
        return Ok(true);
    }
    let mut entries: Vec<(String, std::fs::DirEntry)> = Vec::new();
    for entry in std::fs::read_dir(here).with_context(|| format!("read {}", here.display()))? {
        let Ok(entry) = entry else { continue };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        entries.push((name, entry));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, entry) in entries {
        if out.len() > MAX_ENTRIES {
            return Ok(true);
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            // `file_type` does not follow symlinks, so this is a real directory.
            // A symlinked one is skipped deliberately: following it invites a
            // cycle, and one pointing back inside would list every file twice.
            // A file *through* such a link is still uploadable by name — only
            // the advertising stops here.
            //
            // And not into dot-directories: an operator who stages a checkout
            // has a `.git` in it, which sorts first and would fill the whole
            // list with objects nobody is going to upload.
            if name.starts_with('.') {
                continue;
            }
            prefix.push(name);
            let stopped = walk(datadir, &entry.path(), prefix, depth + 1, out)?;
            prefix.pop();
            if stopped {
                return Ok(true);
            }
            continue;
        }

        let mut parts = prefix.clone();
        parts.push(name);
        let rel = parts.join("/");
        // Reuse the same gate the upload path uses, so the list can never
        // advertise something that would later be refused.
        let Ok((real, _)) = staged(datadir, &rel) else {
            continue;
        };
        let size = real.metadata().map(|m| m.len()).unwrap_or(0);
        // Named as it was found, not as it canonicalizes. A symlink inside the
        // directory is a name the operator chose and it resolves, so it is
        // listed as itself rather than silently renamed to its target — which
        // would also list the target twice.
        out.push(Artifact { name: rel, size });
    }
    Ok(false)
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

    fn names(l: &Listing) -> Vec<&str> {
        l.items.iter().map(|a| a.name.as_str()).collect()
    }

    #[test]
    fn lists_regular_files_by_name() {
        let data = scratch("list");
        let a = ensure_dir(&data).unwrap();
        write(&a, "zeta.sh", "z");
        write(&a, "alpha.conf", "hello");
        std::fs::create_dir(a.join("a-directory")).unwrap();

        let found = list(&data).unwrap();
        assert_eq!(names(&found), vec!["alpha.conf", "zeta.sh"]);
        assert_eq!(found.items[0].size, 5);
        assert!(!found.truncated);
    }

    /// The reason this is recursive: operators stage files in the shape the
    /// files have, and a flat scan reported an empty directory beside a tree
    /// full of work.
    #[test]
    fn subdirectories_are_listed_by_their_path_and_can_be_resolved() {
        let data = scratch("deep");
        let a = ensure_dir(&data).unwrap();
        write(&a, "top.conf", "t");
        std::fs::create_dir_all(a.join("nginx/sites")).unwrap();
        std::fs::create_dir_all(a.join("postgres")).unwrap();
        write(&a.join("nginx"), "nginx.conf", "n");
        write(&a.join("nginx/sites"), "default", "d");
        write(&a.join("postgres"), "pg_hba.conf", "p");

        let found = list(&data).unwrap();
        assert_eq!(
            names(&found),
            vec![
                "nginx/nginx.conf",
                "nginx/sites/default",
                "postgres/pg_hba.conf",
                "top.conf",
            ]
        );
        // And every name it advertises is one an upload step can use — the whole
        // point, and the invariant that made the flat `/` ban removable.
        for a in &found.items {
            staged(&data, &a.name)
                .unwrap_or_else(|e| panic!("{} was listed but will not resolve: {e}", a.name));
        }
    }

    /// The upload step names the destination from this, so it has to be a clean
    /// relative path whatever the model wrote.
    #[test]
    fn a_resolved_name_comes_back_canonical() {
        let data = scratch("canon");
        let a = ensure_dir(&data).unwrap();
        std::fs::create_dir_all(a.join("nginx")).unwrap();
        write(&a.join("nginx"), "site.conf", "s");
        for spelling in ["nginx/site.conf", "./nginx/site.conf", "  nginx/site.conf "] {
            let (_, rel) = staged(&data, spelling).unwrap();
            assert_eq!(rel, "nginx/site.conf", "{spelling:?}");
        }
    }

    /// A deep or wide tree must bound what it puts in the model's context, and
    /// must say that it did — a short list read as a complete one is worse than
    /// no list.
    #[test]
    fn a_large_tree_is_bounded_and_says_so() {
        let data = scratch("many");
        let a = ensure_dir(&data).unwrap();
        for i in 0..MAX_ENTRIES + 20 {
            write(&a, &format!("file-{i:04}.conf"), "x");
        }
        let found = list(&data).unwrap();
        assert_eq!(found.items.len(), MAX_ENTRIES);
        assert!(found.truncated, "truncation is never silent");
        // Past the cap is still uploadable: the list advertises, it does not
        // decide.
        staged(&data, &format!("file-{:04}.conf", MAX_ENTRIES + 19)).unwrap();

        // Depth is bounded too, and nothing below the limit is claimed.
        let deep = scratch("deep-limit");
        let a = ensure_dir(&deep).unwrap();
        let mut p = a.clone();
        for i in 0..MAX_DEPTH + 3 {
            p = p.join(format!("d{i}"));
        }
        std::fs::create_dir_all(&p).unwrap();
        write(&p, "buried.conf", "b");
        let found = list(&deep).unwrap();
        assert!(found.truncated, "the depth limit is reported: {found:?}");
        assert!(names(&found).iter().all(|n| !n.contains("buried")));
    }

    /// A `.git` in a staged checkout sorts first and would otherwise fill the
    /// whole list with objects nobody is going to upload.
    #[test]
    fn dot_directories_are_not_walked_but_dotfiles_are_listed() {
        let data = scratch("dots");
        let a = ensure_dir(&data).unwrap();
        write(&a, ".env", "SECRET=1");
        std::fs::create_dir_all(a.join(".git/objects")).unwrap();
        write(&a.join(".git/objects"), "abcdef", "blob");

        let found = list(&data).unwrap();
        assert_eq!(names(&found), vec![".env"], "{found:?}");
        // Not walked is not the same as forbidden; the operator can still name it.
        assert!(staged(&data, ".git/objects/abcdef").is_ok());
    }

    #[test]
    fn a_missing_directory_lists_nothing_rather_than_failing() {
        let data = scratch("empty");
        assert!(list(&data).unwrap().items.is_empty());
    }

    #[test]
    fn a_name_is_resolved_inside_the_directory() {
        let data = scratch("resolve");
        let a = ensure_dir(&data).unwrap();
        write(&a, "hotfix.sh", "#!/bin/sh\n");
        let (p, _) = staged(&data, "hotfix.sh").unwrap();
        assert!(p.starts_with(a.canonicalize().unwrap()));
        assert!(p.is_file());
        // Surrounding whitespace is not a different artifact.
        assert_eq!(staged(&data, "  hotfix.sh  ").unwrap().0, p);
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
            let e = staged(&data, evil)
                .map(|(p, _)| format!("resolved to {}", p.display()))
                .unwrap_err()
                .to_string();
            // Refused by one of the three gates, each with a message that says
            // which: the name has a `..` or a leading `/`, there is no such
            // file, or it resolved to something that is not a regular file —
            // which is how `.` and `..` are turned away now that a name may
            // legitimately contain a `/`.
            assert!(
                e.contains("no leading")
                    || e.contains("no artifact")
                    || e.contains("not a regular file"),
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

        let e = staged(&data, "innocent.txt").unwrap_err().to_string();
        assert!(e.contains("outside the artifacts directory"), "{e}");

        // And it is not even advertised.
        let found = list(&data).unwrap();
        assert!(
            found.items.iter().all(|f| f.name != "innocent.txt"),
            "a symlink out of the directory must not be listed: {found:?}"
        );
    }

    /// Permitting `/` in a name exposes one escape a flat name could not reach: a
    /// *directory* symlink out of the artifacts directory, walked through by an
    /// innocent-looking path. `canonicalize` resolves the whole path, so the
    /// containment check sees it.
    #[cfg(unix)]
    #[test]
    fn a_path_through_a_directory_symlink_cannot_escape() {
        let data = scratch("dirlink");
        let a = ensure_dir(&data).unwrap();
        std::fs::create_dir_all(data.join("keys")).unwrap();
        std::fs::write(data.join("keys/web-01"), "PRIVATE KEY").unwrap();
        std::os::unix::fs::symlink(data.join("keys"), a.join("stash")).unwrap();

        let e = staged(&data, "stash/web-01").unwrap_err().to_string();
        assert!(e.contains("outside the artifacts directory"), "{e}");
        // And the walk does not descend through it, so it is never advertised.
        let found = list(&data).unwrap();
        assert!(names(&found).is_empty(), "{found:?}");
    }

    /// A symlink that stays inside is fine — it is a normal way to stage a file.
    #[cfg(unix)]
    #[test]
    fn a_symlink_within_the_directory_is_fine() {
        let data = scratch("symlink-ok");
        let a = ensure_dir(&data).unwrap();
        write(&a, "real.conf", "body");
        std::os::unix::fs::symlink(a.join("real.conf"), a.join("alias.conf")).unwrap();
        assert!(staged(&data, "alias.conf").is_ok());
    }

    #[test]
    fn a_directory_is_not_an_artifact() {
        let data = scratch("dir");
        let a = ensure_dir(&data).unwrap();
        std::fs::create_dir(a.join("stuff")).unwrap();
        let e = staged(&data, "stuff").unwrap_err().to_string();
        assert!(e.contains("not a regular file"), "{e}");
    }
}
