//! Files the operator has staged for upload, in `<datadir>/artifacts/`.
//!
//! The path handling — every way a name could point outside the directory, and
//! the bounded recursive walk — is `store::ARTIFACTS`, shared with `manuals`.
//! What is here is only what makes an artifact an artifact: its size, which is
//! what an operator wants to see before approving an upload of it.

use crate::agent::store::ARTIFACTS;
use anyhow::Result;
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

/// Create the directory if it is missing, owner-only like `keys/`.
pub fn ensure_dir(datadir: &Path) -> Result<PathBuf> {
    ARTIFACTS.ensure(datadir)
}

/// Resolve an artifact name to `(real file, canonical relative name)`.
///
/// The second half is what the upload step needs, so the destination on the far
/// side is built from a path this module vouches for rather than from what the
/// model typed.
pub fn staged(datadir: &Path, name: &str) -> Result<(PathBuf, String)> {
    ARTIFACTS.resolve(datadir, name)
}

/// Every usable artifact, recursively, sorted by name, with its size.
pub fn list(datadir: &Path) -> Result<Listing> {
    let (names, truncated) = ARTIFACTS.walk(datadir)?;
    let items = names
        .into_iter()
        .map(|name| {
            // Already resolved by the walk; this is only the size.
            let size = staged(datadir, &name)
                .and_then(|(real, _)| Ok(real.metadata()?.len()))
                .unwrap_or(0);
            Artifact { name, size }
        })
        .collect();
    Ok(Listing { items, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::store;

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
        for i in 0..store::MAX_ENTRIES + 20 {
            write(&a, &format!("file-{i:04}.conf"), "x");
        }
        let found = list(&data).unwrap();
        assert_eq!(found.items.len(), store::MAX_ENTRIES);
        assert!(found.truncated, "truncation is never silent");
        // Past the cap is still uploadable: the list advertises, it does not
        // decide.
        staged(&data, &format!("file-{:04}.conf", store::MAX_ENTRIES + 19)).unwrap();

        // Depth is bounded too, and nothing below the limit is claimed.
        let deep = scratch("deep-limit");
        let a = ensure_dir(&deep).unwrap();
        let mut p = a.clone();
        for i in 0..store::MAX_DEPTH + 3 {
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
