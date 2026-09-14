//! A directory under the data directory that the operator fills by hand.
//!
//! Two of them: `artifacts/`, whose files a plan uploads, and `manuals/`, whose
//! files the model reads. Both take a *name from the model* and turn it into a
//! local path, so the escape handling is the same and lives here once — the
//! alternative is two copies of a security boundary, which is how one of them
//! ends up wrong.
//!
//! A name is untrusted input. Three ways out of the directory have to be closed,
//! and only the first is obvious:
//!
//! * `../../keys/web-01` — a relative escape in the name itself.
//! * a file that *is* a symlink to somewhere else — the name is innocent, the
//!   target is not.
//! * a path *through* a directory symlink that leads out — `stash/web-01`, where
//!   `stash` points at `keys/`. This one only exists because a name may contain
//!   `/`.
//!
//! Canonicalizing and then checking containment closes all three, because
//! `canonicalize` resolves `..` and follows every symlink in the path before the
//! check happens.
//!
//! A name may be a path into a subdirectory — `nginx/site.conf` — because
//! operators keep files in the shape the files have, and a flat directory makes
//! them rename everything to use it. That is why the containment check has to be
//! the real boundary rather than a second belt behind a ban on `/`: there is no
//! such ban, and `canonicalize` is all that stands between a name and the rest of
//! the data directory. Which is what it was already doing for symlinks.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// How deep a scan goes, and how many files it will name.
///
/// An operator who stages a checkout rather than a file should get a bounded
/// list rather than a flooded context. Anything past these is still reachable by
/// name — `resolve` does not consult them — it just is not advertised.
pub const MAX_DEPTH: usize = 8;
pub const MAX_ENTRIES: usize = 200;

/// One such directory: where it is, and what to call the things in it.
///
/// `noun` is only for the refusals, and it earns its place there: "no manual
/// named" and "outside the artifacts directory" are what the model needs to read,
/// not the name of a Rust module.
pub struct Store {
    subdir: &'static str,
    noun: &'static str,
}

pub const ARTIFACTS: Store = Store {
    subdir: "artifacts",
    noun: "artifact",
};

pub const MANUALS: Store = Store {
    subdir: "manuals",
    noun: "manual",
};

impl Store {
    pub fn dir(&self, datadir: &Path) -> PathBuf {
        datadir.join(self.subdir)
    }

    /// Create the directory if it is missing, owner-only like `keys/`.
    ///
    /// Called at startup for both stores. A directory nobody creates is a
    /// feature nobody finds: the operator has to see `manuals/` to know they may
    /// write one.
    pub fn ensure(&self, datadir: &Path) -> Result<PathBuf> {
        let d = self.dir(datadir);
        std::fs::create_dir_all(&d).with_context(|| format!("create {}", d.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
        }
        Ok(d)
    }

    /// Resolve a name to `(real file, canonical relative name)`.
    ///
    /// Returns an error rather than a path for anything that escapes, so a caller
    /// cannot forget to check. The one entry point, so there is no second, laxer
    /// one for a caller to reach for by mistake.
    ///
    /// The relative name is rebuilt from the *canonicalized* path rather than
    /// from what the model typed, so `./nginx//site.conf` and `nginx/site.conf`
    /// give the same answer — and by the time there is one, a name that pointed
    /// somewhere else has already been refused.
    pub fn resolve(&self, datadir: &Path, name: &str) -> Result<(PathBuf, String)> {
        let noun = self.noun;
        let name = name.trim();
        if name.is_empty() {
            bail!("no {noun} named");
        }
        if name.contains('\\') || name.contains('\0') {
            bail!("{noun} name must be a relative path with no backslash: {name:?}");
        }
        // Component-wise rather than by substring, so `..` is refused wherever it
        // appears and `a..b.conf` — an ordinary file name — is not. A leading
        // `./` is allowed because it means nothing and refusing it would cost a
        // round trip; `canonicalize` below resolves it either way.
        for c in Path::new(name).components() {
            match c {
                std::path::Component::Normal(_) | std::path::Component::CurDir => {}
                _ => bail!(
                    "{noun} name must be a path relative to the {} directory, with no `..` \
                     and no leading `/`: {name:?}",
                    self.subdir
                ),
            }
        }

        let base = self.dir(datadir);
        let base = base
            .canonicalize()
            .with_context(|| format!("no {} directory at {}", self.subdir, base.display()))?;
        let candidate = base.join(name);
        // Resolves `..` and follows symlinks, so the containment check below sees
        // where the file really is rather than where it claims to be. With
        // subdirectories permitted this is the whole boundary, including for a
        // *directory* symlink that leads out.
        let real = candidate
            .canonicalize()
            .with_context(|| format!("no {noun} named {name:?}"))?;

        if !real.starts_with(&base) {
            bail!(
                "{noun} {name:?} resolves outside the {} directory",
                self.subdir
            );
        }
        if !real.is_file() {
            bail!("{noun} {name:?} is not a regular file");
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

    /// Resolve a name to a path to *write* into the store, containment-checked.
    ///
    /// The mirror of `resolve` for a file that does not exist yet — a download
    /// destination. `canonicalize` cannot judge a path that is not there, so
    /// the deepest ancestor that *does* exist is canonicalized instead and the
    /// containment checked on it: a directory symlink leading out is caught
    /// there, and the components below it are checked to be ordinary names by
    /// the same component loop `resolve` uses. A target that already exists as
    /// a directory is refused, since writing one would fail confusingly.
    pub fn dest(&self, datadir: &Path, name: &str) -> Result<PathBuf> {
        let noun = self.noun;
        let name = name.trim();
        if name.is_empty() {
            bail!("no {noun} named");
        }
        if name.contains('\\') || name.contains('\0') {
            bail!("{noun} name must be a relative path with no backslash: {name:?}");
        }
        for c in Path::new(name).components() {
            match c {
                std::path::Component::Normal(_) | std::path::Component::CurDir => {}
                _ => bail!(
                    "{noun} name must be a path relative to the {} directory, with no `..` \
                      and no leading `/`: {name:?}",
                    self.subdir
                ),
            }
        }

        let base = self.dir(datadir);
        let base = base
            .canonicalize()
            .with_context(|| format!("no {} directory at {}", self.subdir, base.display()))?;
        let candidate = base.join(name);

        // Up from the target until something exists; the store directory
        // itself always does, so this terminates. Whatever exists is followed
        // to where it really is — this is where a symlink leading out is
        // caught, at the first component that is one.
        let mut probe = candidate.as_path();
        let real = loop {
            match probe.canonicalize() {
                Ok(real) => break real,
                Err(_) => match probe.parent() {
                    Some(p) => probe = p,
                    None => bail!("{noun} {name:?} cannot be made a path"),
                },
            }
        };
        if !real.starts_with(&base) {
            bail!(
                "{noun} {name:?} resolves outside the {} directory",
                self.subdir
            );
        }
        if candidate.is_dir() {
            bail!("{noun} {name:?} is a directory, not a file to write");
        }
        Ok(candidate)
    }

    /// Every resolvable file, recursively, sorted, bounded. `true` means the walk
    /// stopped on a limit rather than running out of files.
    ///
    /// Recursive because operators keep files in the shape the files have:
    /// `nginx/site.conf` next to `postgres/pg_hba.conf`. A flat scan named
    /// neither and gave the model an empty list beside a directory full of work.
    ///
    /// One name over the cap is collected before stopping, so a full list and a
    /// truncated one are distinguishable without walking the rest of the tree.
    pub fn walk(&self, datadir: &Path) -> Result<(Vec<String>, bool)> {
        let d = self.dir(datadir);
        if !d.exists() {
            return Ok((Vec::new(), false));
        }
        let mut names = Vec::new();
        let stopped = self.descend(datadir, &d, &mut Vec::new(), 0, &mut names)?;
        names.sort();
        let truncated = stopped || names.len() > MAX_ENTRIES;
        names.truncate(MAX_ENTRIES);
        Ok((names, truncated))
    }

    /// One directory, then the ones inside it.
    ///
    /// Entries are sorted before descending so that a truncated list is the same
    /// list every time; `read_dir` order is whatever the filesystem says.
    fn descend(
        &self,
        datadir: &Path,
        here: &Path,
        prefix: &mut Vec<String>,
        depth: usize,
        out: &mut Vec<String>,
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
                // `file_type` does not follow symlinks, so this is a real
                // directory. A symlinked one is skipped deliberately: following
                // it invites a cycle, and one pointing back inside would list
                // every file twice. A file *through* such a link is still
                // reachable by name — only the advertising stops here.
                //
                // And not into dot-directories: an operator who stages a
                // checkout has a `.git` in it, which sorts first and would fill
                // the whole list with objects nobody is going to use.
                if name.starts_with('.') {
                    continue;
                }
                prefix.push(name);
                let stopped = self.descend(datadir, &entry.path(), prefix, depth + 1, out)?;
                prefix.pop();
                if stopped {
                    return Ok(true);
                }
                continue;
            }

            let mut parts = prefix.clone();
            parts.push(name);
            let rel = parts.join("/");
            // The same gate the readers use, so a listing can never advertise
            // something a later call would refuse.
            if self.resolve(datadir, &rel).is_err() {
                continue;
            }
            // Named as it was found, not as it canonicalizes. A symlink inside
            // the directory is a name the operator chose and it resolves, so it
            // is listed as itself rather than silently renamed to its target —
            // which would also list the target twice.
            out.push(rel);
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openadmin-store-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The escapes are tested in depth through `artifacts`, which has exercised
    /// this code since before it moved here. What matters here is that the
    /// *second* store gets exactly the same treatment — the whole reason for
    /// having one copy — and that a refusal still reads as being about the thing
    /// the model named.
    #[test]
    fn every_store_closes_the_same_escapes_in_its_own_words() {
        let data = scratch("escapes");
        for store in [&ARTIFACTS, &MANUALS] {
            let d = store.ensure(&data).unwrap();
            std::fs::write(d.join("ok.txt"), "fine").unwrap();
            std::fs::create_dir_all(data.join("keys")).unwrap();
            std::fs::write(data.join("keys/web-01"), "PRIVATE KEY").unwrap();
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(data.join("keys/web-01"), d.join("innocent")).unwrap();
                std::os::unix::fs::symlink(data.join("keys"), d.join("stash")).unwrap();
            }

            assert!(store.resolve(&data, "ok.txt").is_ok());
            for evil in [
                "../keys/web-01",
                "sub/../../keys/web-01",
                "/etc/passwd",
                "..",
                "",
                #[cfg(unix)]
                "innocent",
                #[cfg(unix)]
                "stash/web-01",
            ] {
                let e = store
                    .resolve(&data, evil)
                    .map(|(p, _)| format!("resolved to {}", p.display()))
                    .unwrap_err()
                    .to_string();
                assert!(
                    e.contains(store.noun),
                    "{:?} refused without saying what it was about: {e}",
                    evil
                );
                // One of the gates, each of which says which it was.
                assert!(
                    e.contains("name must be")
                        || e.ends_with("named")
                        || e.contains("outside the")
                        || e.contains("not a regular file"),
                    "{evil:?} was not refused cleanly: {e}"
                );
            }

            // Neither symlink is advertised either.
            let (names, _) = store.walk(&data).unwrap();
            assert_eq!(names, vec!["ok.txt".to_string()], "{}", store.subdir);
        }
    }

    /// A dest is a file that does not exist yet, so `canonicalize` cannot
    /// judge the whole path: the deepest existing ancestor is judged instead,
    /// which is what catches a symlink leading out even when the target
    /// itself is not there to resolve.
    #[test]
    fn a_dest_is_containment_checked_before_it_exists() {
        let data = scratch("dest");
        for store in [&ARTIFACTS, &MANUALS] {
            let d = store.ensure(&data).unwrap();
            std::fs::create_dir_all(d.join("logs")).unwrap();
            std::fs::create_dir_all(data.join("keys")).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(data.join("keys"), d.join("stash")).unwrap();

            // A new file in a new subdirectory: nothing exists yet, and it is
            // inside.
            let p = store.dest(&data, "logs/new/x.log").unwrap();
            assert!(p.starts_with(&d), "{}", p.display());
            assert!(p.ends_with("logs/new/x.log"), "{}", p.display());

            // The escapes.
            for evil in [
                "../keys/web-01",
                "/abs/x.log",
                "..",
                "",
            ] {
                let e = store.dest(&data, evil).unwrap_err().to_string();
                assert!(
                    e.contains(store.noun),
                    "{evil:?} refused without saying what it was about: {e}"
                );
            }
            #[cfg(unix)]
            {
                // Through a directory symlink that leads out — the ancestor
                // exists, canonicalizes outside, and the dest is refused even
                // though `stash/new/x.log` itself does not exist.
                let e = store.dest(&data, "stash/new/x.log")
                    .unwrap_err()
                    .to_string();
                assert!(e.contains("outside the"), "{e}");
            }

            // And an existing directory is not a file to write.
            let e = store.dest(&data, "logs").unwrap_err().to_string();
            assert!(e.contains("directory"), "{e}");
        }
    }

    /// The two stores are different directories, so a name in one is not a name
    /// in the other.
    #[test]
    fn the_stores_do_not_see_each_others_files() {
        let data = scratch("separate");
        let a = ARTIFACTS.ensure(&data).unwrap();
        MANUALS.ensure(&data).unwrap();
        std::fs::write(a.join("hotfix.sh"), "#!/bin/sh\n").unwrap();

        assert!(ARTIFACTS.resolve(&data, "hotfix.sh").is_ok());
        let e = MANUALS.resolve(&data, "hotfix.sh").unwrap_err().to_string();
        assert!(e.contains("no manual named"), "{e}");
        assert!(MANUALS.walk(&data).unwrap().0.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn ensure_is_owner_only_and_idempotent() {
        use std::os::unix::fs::PermissionsExt;
        let data = scratch("ensure");
        let d = MANUALS.ensure(&data).unwrap();
        assert!(d.is_dir());
        let mode = std::fs::metadata(&d).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "the data directory's own files are 0600/0700");
        // Called on every startup, so it must not mind an existing directory.
        std::fs::write(d.join("keep.md"), "still here\n").unwrap();
        MANUALS.ensure(&data).unwrap();
        assert!(d.join("keep.md").exists());
    }
}
