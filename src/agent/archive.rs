//! What is inside an archive in the artifacts directory, without unpacking it.
//!
//! An archive is opaque to a model that can only list and upload artifacts. To
//! write the script that unpacks `app-1.2.3.tar.gz` and runs its installer it had
//! to guess the top directory's name, whether `install.sh` exists, whether it is
//! executable. A wrong guess is a failed plan on real hosts — the most expensive
//! round trip there is, because the operator reviewed and approved it first. One
//! read-only call turns the guesses into facts.
//!
//! `.tar.gz`, `.tgz` and `.zip`. Nothing is extracted and nothing is written.
//!
//! The dependencies are pure Rust and decompress nothing they need not: zip is
//! built with no compression features at all, because listing reads only its
//! central directory, which is plain metadata. The tests list a deflate-compressed
//! fixture to keep that true — see `src/agent/testdata/`.

use crate::agent::store::ARTIFACTS;
use anyhow::{Context, Result, bail};
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

/// Stop reading after this many entries.
pub const MAX_ENTRIES: usize = 1000;

/// Stop reading a tar.gz after this long. A tar has no index, so listing one
/// means decompressing all of it to walk the headers, and the agent's turn is
/// waiting on the answer. Checked between entries: one enormous member still has
/// to be read past before the check runs again.
pub const BUDGET: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    /// The target, when it can be read without decompressing anything.
    Symlink(Option<String>),
    Hardlink(String),
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// As stored, with control characters escaped. Never sanitized away: a name
    /// the model sees has to be a name that exists in the archive, or the plan
    /// that extracts it names a file that is not there.
    pub path: String,
    pub kind: Kind,
    pub size: u64,
    /// Permission bits, when the archive records them.
    pub mode: Option<u32>,
}

impl Entry {
    /// Would extracting this write outside the directory it is extracted into?
    /// An absolute path, or any `..` component — split on `\` too, which Windows
    /// extractors treat as a separator.
    pub fn escapes(&self) -> bool {
        self.path.starts_with('/')
            || self.path.starts_with('\\')
            || self.path.split(['/', '\\']).any(|c| c == "..")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub format: &'static str,
    pub entries: Vec<Entry>,
    /// Every entry in the archive, when that is known without reading them all —
    /// a zip's central directory says; a tar that was stopped early cannot.
    pub total: Option<usize>,
    /// Why reading stopped before the end, if it did.
    pub stopped: Option<Stopped>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    Entries(usize),
    Time(Duration),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    TarGz,
    Zip,
}

/// List the archive `name` in the artifacts directory.
pub fn list(datadir: &Path, name: &str, max_entries: usize, budget: Duration) -> Result<Listing> {
    // The same containment check an upload goes through: no name reaches a
    // file outside the artifacts directory.
    let (real, rel) = ARTIFACTS.resolve(datadir, name)?;
    let format = detect(&real, &rel)?;
    let file = std::fs::File::open(&real).with_context(|| format!("open {rel}"))?;
    match format {
        Format::Zip => list_zip(file, max_entries),
        Format::TarGz => list_tar_gz(file, max_entries, budget),
    }
}

/// Which format, from the name, confirmed by the first bytes.
///
/// The extension is what an operator meant and the magic is what the file is.
/// Both have to agree, and a disagreement is said plainly: a `.zip` that is
/// really gzip is a mistake worth knowing about before a plan runs `unzip` on it.
fn detect(real: &Path, rel: &str) -> Result<Format> {
    let lower = rel.to_ascii_lowercase();
    let wanted = if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        Format::TarGz
    } else if lower.ends_with(".zip") {
        Format::Zip
    } else {
        bail!("{rel} is not an archive this can read: .tar.gz, .tgz and .zip are supported");
    };

    let mut magic = [0u8; 4];
    let n = std::fs::File::open(real)
        .and_then(|mut f| f.read(&mut magic))
        .with_context(|| format!("read {rel}"))?;
    let magic = &magic[..n];
    let is_gzip = magic.starts_with(&[0x1f, 0x8b]);
    let is_zip = magic.starts_with(b"PK\x03\x04") || magic.starts_with(b"PK\x05\x06");
    let actually = if is_gzip {
        "gzip data"
    } else if is_zip {
        "a zip archive"
    } else {
        "neither gzip nor zip"
    };
    match wanted {
        Format::TarGz if !is_gzip => bail!("{rel} is named like a tar.gz but is {actually}"),
        Format::Zip if !is_zip => bail!("{rel} is named like a zip but is {actually}"),
        _ => Ok(wanted),
    }
}

fn list_zip(file: std::fs::File, max_entries: usize) -> Result<Listing> {
    let mut zip = zip::ZipArchive::new(file).context("read the zip's central directory")?;
    let total = zip.len();
    let mut entries = Vec::with_capacity(total.min(max_entries));
    for i in 0..total.min(max_entries) {
        // Raw: metadata only. No compression method is needed to read it, which
        // is what lets zip be built with none.
        let mut f = zip
            .by_index_raw(i)
            .with_context(|| format!("read zip entry {i}"))?;
        // Only the mode bits the archive actually stores. zip's `unix_mode` makes
        // some up for an entry written on DOS or Windows — 775 for a directory,
        // 664 for a file — and presented as real those would answer "is the
        // installer executable?" with a guess.
        let stored = zip::HasZipMetadata::get_metadata(&f).external_attributes >> 16;
        let mode = (stored != 0).then_some(stored & 0o7777);
        let kind = if f.is_dir() {
            Kind::Dir
        } else if f.is_symlink() {
            Kind::Symlink(zip_link_target(&mut f))
        } else {
            Kind::File
        };
        entries.push(Entry {
            path: escape_controls(f.name()),
            size: if kind == Kind::File { f.size() } else { 0 },
            kind,
            mode,
        });
    }
    Ok(Listing {
        format: "zip",
        entries,
        total: Some(total),
        stopped: (total > max_entries).then_some(Stopped::Entries(max_entries)),
    })
}

/// A zip symlink's target, which the format keeps as the entry's content.
///
/// Read from the raw bytes and inflated with flate2 when deflated — CPython and
/// most producers compress symlink entries like any other — so zip itself still
/// needs no compression feature. Bounded, since a "symlink" is only a claim the
/// archive makes about itself.
fn zip_link_target<R: Read>(f: &mut zip::read::ZipFile<'_, R>) -> Option<String> {
    const MAX: u64 = 4096;
    if f.size() > MAX {
        return None;
    }
    let mut s = String::new();
    // zip's constants rather than its enum variants: `Deflated` exists only with
    // the feature this build leaves out, while `DEFLATE` names the method either
    // way.
    let method = f.compression();
    let read = if method == zip::CompressionMethod::STORE {
        f.take(MAX).read_to_string(&mut s)
    } else if method == zip::CompressionMethod::DEFLATE {
        flate2::read::DeflateDecoder::new(f.take(MAX))
            .take(MAX)
            .read_to_string(&mut s)
    } else {
        return None;
    };
    read.ok().map(|_| escape_controls(&s))
}

fn list_tar_gz(file: std::fs::File, max_entries: usize, budget: Duration) -> Result<Listing> {
    let started = Instant::now();
    // Multi, not plain: `GzDecoder` stops at the end of the first gzip member,
    // and a tarball that was concatenated, or written by a parallel compressor,
    // has several. The single-member decoder listed such an archive as complete
    // while dropping everything after the first member — escaping entries
    // included.
    let gz = flate2::read::MultiGzDecoder::new(std::io::BufReader::new(file));
    let mut tar = tar::Archive::new(gz);
    let mut entries = Vec::new();
    let mut stopped = None;
    for item in tar.entries().context("read the tar stream")? {
        if entries.len() >= max_entries {
            stopped = Some(Stopped::Entries(max_entries));
            break;
        }
        if started.elapsed() >= budget {
            stopped = Some(Stopped::Time(budget));
            break;
        }
        let e = item.context("read a tar header")?;
        let h = e.header();
        let path = escape_controls(&String::from_utf8_lossy(&e.path_bytes()));
        let link = || {
            e.link_name_bytes()
                .map(|b| escape_controls(&String::from_utf8_lossy(&b)))
        };
        let kind = match h.entry_type() {
            tar::EntryType::Regular | tar::EntryType::Continuous => Kind::File,
            tar::EntryType::Directory => Kind::Dir,
            tar::EntryType::Symlink => Kind::Symlink(link()),
            tar::EntryType::Link => Kind::Hardlink(link().unwrap_or_default()),
            // PAX and GNU long-name records are folded into the entry they
            // describe by the iterator and never reach here.
            _ => Kind::Other,
        };
        entries.push(Entry {
            size: if kind == Kind::File { e.size() } else { 0 },
            mode: h.mode().ok().map(|m| m & 0o7777),
            path,
            kind,
        });
    }
    Ok(Listing {
        format: "tar.gz",
        total: stopped.is_none().then_some(entries.len()),
        entries,
        stopped,
    })
}

/// Control characters made visible rather than removed.
///
/// An entry's name is text someone else chose. Removing its control characters
/// would keep them off the operator's terminal but hand the model a name that
/// does not exist in the archive; escaping them does both jobs.
fn escape_controls(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if (c as u32) < 0x20 || c == '\u{7f}' || ('\u{80}'..='\u{9f}').contains(&c) {
            out.push_str(&format!("\\x{:02x}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

fn human(n: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64 / 1024.0;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    format!("{v:.1} {}", UNITS[u])
}

fn perms(mode: Option<u32>) -> String {
    let Some(m) = mode else {
        // Not recorded — which is not the same as "no permissions".
        return "?????????".to_string();
    };
    let bit = |mask: u32, c: char| if m & mask != 0 { c } else { '-' };
    [
        bit(0o400, 'r'),
        bit(0o200, 'w'),
        bit(0o100, 'x'),
        bit(0o040, 'r'),
        bit(0o020, 'w'),
        bit(0o010, 'x'),
        bit(0o004, 'r'),
        bit(0o002, 'w'),
        bit(0o001, 'x'),
    ]
    .iter()
    .collect()
}

/// The listing as the model reads it: a summary, any entries that escape, then
/// one line per entry, bounded to `cap_bytes`.
///
/// One compact line per entry rather than JSON, which costs several times the
/// tokens at hundreds of entries. Entries that escape the extraction directory
/// are named in the summary as well as marked in place, so a cap that cuts the
/// tree cannot hide them — they are the lines most likely to change a plan.
pub fn render(name: &str, l: &Listing, cap_bytes: usize) -> String {
    let unpacked: u64 = l.entries.iter().map(|e| e.size).sum();
    let count = match l.total {
        Some(t) => format!("{t} entr{}", if t == 1 { "y" } else { "ies" }),
        None => format!("at least {} entries", l.entries.len()),
    };
    let mut head = format!(
        "{name} — {}, {count}, {} unpacked",
        l.format,
        human(unpacked)
    );
    if l.stopped.is_some() || l.total.is_some_and(|t| t > l.entries.len()) {
        head.push_str(" (in the entries listed)");
    }
    head.push('\n');

    match l.stopped {
        Some(Stopped::Entries(n)) => head.push_str(&format!(
            "Listing stopped after {n} entries; the archive has more than are shown.\n"
        )),
        Some(Stopped::Time(d)) => head.push_str(&format!(
            "Listing stopped after {}s of reading; the archive has more than are shown.\n",
            d.as_secs()
        )),
        None => {}
    }

    let escaping: Vec<&Entry> = l.entries.iter().filter(|e| e.escapes()).collect();
    if !escaping.is_empty() {
        head.push_str(&format!(
            "! {} entr{} would be written outside the directory this is extracted into \
             (an absolute path or a `..`). Extract it somewhere disposable, or not at all:\n",
            escaping.len(),
            if escaping.len() == 1 { "y" } else { "ies" }
        ));
        for e in escaping.iter().take(20) {
            head.push_str(&format!("!   {}\n", e.path));
        }
        if escaping.len() > 20 {
            head.push_str(&format!("!   … and {} more\n", escaping.len() - 20));
        }
    }

    // Room kept back for the "… N more not shown" line, so the cap is a cap: a
    // first cut checked each entry against it and then appended that line past
    // it, landing 58 bytes over on a 50,000-entry archive.
    const TAIL: usize = 96;
    let budget = cap_bytes.saturating_sub(TAIL);
    let mut out = head;
    let mut shown = 0;
    for e in &l.entries {
        let (t, size, suffix) = match &e.kind {
            Kind::File => ('f', human(e.size), String::new()),
            Kind::Dir => ('d', String::new(), String::new()),
            Kind::Symlink(Some(to)) => ('l', String::new(), format!(" -> {to}")),
            Kind::Symlink(None) => ('l', String::new(), " -> (target not readable)".into()),
            Kind::Hardlink(to) => ('h', String::new(), format!(" => {to}")),
            Kind::Other => ('?', String::new(), String::new()),
        };
        let mark = if e.escapes() { "  ! outside" } else { "" };
        let line = format!(
            "{t} {} {size:>10} {}{suffix}{mark}\n",
            perms(e.mode),
            e.path
        );
        if out.len() + line.len() > budget {
            break;
        }
        out.push_str(&line);
        shown += 1;
    }
    if shown < l.entries.len() {
        out.push_str(&format!(
            "… {} more entries not shown, to stay within the output limit.\n",
            l.entries.len() - shown
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A data directory with the fixtures in its artifacts directory. Included
    /// at compile time, so the tests do not depend on where they are run from.
    fn staged(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("openadmin-archive-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = ARTIFACTS.ensure(&dir).unwrap();
        for (name, bytes) in [
            ("sample.zip", &include_bytes!("testdata/sample.zip")[..]),
            (
                "sample.tar.gz",
                &include_bytes!("testdata/sample.tar.gz")[..],
            ),
            (
                "gzip-named.zip",
                &include_bytes!("testdata/gzip-named.zip")[..],
            ),
            (
                "zip-named.tar.gz",
                &include_bytes!("testdata/zip-named.tar.gz")[..],
            ),
            (
                "multimember.tar.gz",
                &include_bytes!("testdata/multimember.tar.gz")[..],
            ),
            (
                "deflated-link.zip",
                &include_bytes!("testdata/deflated-link.zip")[..],
            ),
            ("dos.zip", &include_bytes!("testdata/dos.zip")[..]),
        ] {
            std::fs::write(a.join(name), bytes).unwrap();
        }
        dir
    }

    fn entry<'a>(l: &'a Listing, path: &str) -> &'a Entry {
        l.entries
            .iter()
            .find(|e| e.path == path)
            .unwrap_or_else(|| panic!("{path} not listed: {:?}", l.entries))
    }

    /// The shared tree, as both formats should report it. The zip is
    /// deflate-compressed and zip is built with no compression features, so this
    /// is also the test that listing never needs to decompress.
    fn assert_the_common_tree(l: &Listing) {
        assert_eq!(entry(l, "app-1.2.3/").kind, Kind::Dir);
        let app = entry(l, "app-1.2.3/bin/app");
        assert_eq!(
            (app.kind.clone(), app.size, app.mode),
            (Kind::File, 760, Some(0o755))
        );
        let install = entry(l, "app-1.2.3/install.sh");
        assert_eq!(
            install.mode,
            Some(0o755),
            "executable, which decides how a plan runs it"
        );
        assert_eq!(entry(l, "app-1.2.3/etc/app.conf").mode, Some(0o644));
        assert_eq!(
            entry(l, "app-1.2.3/lib/libfoo.so").kind,
            Kind::Symlink(Some("libfoo.so.1".into()))
        );
        assert_eq!(entry(l, "app-1.2.3/doc/résumé.txt").size, 15);
        assert!(
            !l.entries
                .iter()
                .any(|e| e.path.chars().any(|c| (c as u32) < 0x20))
        );
    }

    #[test]
    fn a_compressed_zip_is_listed_whole_without_decompressing_it() {
        let dir = staged("zip");
        let l = list(&dir, "sample.zip", MAX_ENTRIES, BUDGET).unwrap();
        assert_eq!(l.format, "zip");
        assert_eq!(l.total, Some(11));
        assert_eq!(l.stopped, None);
        assert_the_common_tree(&l);
    }

    #[test]
    fn a_tar_gz_is_listed_whole_including_its_hardlinks() {
        let dir = staged("tgz");
        let l = list(&dir, "sample.tar.gz", MAX_ENTRIES, BUDGET).unwrap();
        assert_eq!(l.format, "tar.gz");
        assert_eq!(l.total, Some(12));
        assert_the_common_tree(&l);
        assert_eq!(
            entry(&l, "app-1.2.3/bin/app-alias").kind,
            Kind::Hardlink("app-1.2.3/bin/app".into())
        );
        // `.tgz` is the same format under another name.
        std::fs::copy(
            ARTIFACTS.dir(&dir).join("sample.tar.gz"),
            ARTIFACTS.dir(&dir).join("same.tgz"),
        )
        .unwrap();
        assert_eq!(
            list(&dir, "same.tgz", MAX_ENTRIES, BUDGET).unwrap().total,
            Some(12)
        );
    }

    /// The lines most likely to change a plan: flagged, counted, named in the
    /// summary, and marked where they stand.
    #[test]
    fn entries_that_escape_the_extract_directory_are_flagged() {
        let dir = staged("escape");
        for name in ["sample.zip", "sample.tar.gz"] {
            let l = list(&dir, name, MAX_ENTRIES, BUDGET).unwrap();
            let escaping: Vec<&str> = l
                .entries
                .iter()
                .filter(|e| e.escapes())
                .map(|e| e.path.as_str())
                .collect();
            assert_eq!(escaping, vec!["../evil", "/etc/cron.d/evil"], "{name}");

            let text = render(name, &l, 16 * 1024);
            assert!(
                text.contains("! 2 entries would be written outside"),
                "{text}"
            );
            assert!(text.contains("../evil  ! outside"), "{text}");
        }
        // And the rule itself, beyond what the fixtures hold.
        let e = |p: &str| Entry {
            path: p.into(),
            kind: Kind::File,
            size: 0,
            mode: None,
        };
        assert!(e("a/../../b").escapes());
        assert!(e("..\\windows").escapes());
        assert!(!e("a..b/c").escapes(), "a dotted name is not a `..`");
        assert!(!e("./a/b").escapes());
    }

    /// Escaped, not removed: the name the model sees must be a name in the
    /// archive, or a plan extracting it names a file that is not there.
    #[test]
    fn control_characters_in_names_are_made_visible_not_removed() {
        let dir = staged("controls");
        for name in ["sample.zip", "sample.tar.gz"] {
            let l = list(&dir, name, MAX_ENTRIES, BUDGET).unwrap();
            let bad = entry(&l, "app-1.2.3/bad\\x1b[2Jname");
            assert_eq!(bad.size, 30, "{name}");
            let text = render(name, &l, 16 * 1024);
            assert!(
                !text.contains('\x1b'),
                "no escape reaches the terminal: {name}"
            );
        }
    }

    /// Stopping early is always said, never silent — the rule `artifacts` follows.
    #[test]
    fn a_limit_stops_the_read_and_the_listing_says_so() {
        let dir = staged("limits");

        let l = list(&dir, "sample.tar.gz", 3, BUDGET).unwrap();
        assert_eq!(l.entries.len(), 3);
        assert_eq!(l.stopped, Some(Stopped::Entries(3)));
        assert_eq!(l.total, None, "a stopped tar cannot know how many it holds");
        let text = render("sample.tar.gz", &l, 16 * 1024);
        assert!(text.contains("at least 3 entries"), "{text}");
        assert!(text.contains("stopped after 3 entries"), "{text}");

        // A zip knows its total from the central directory regardless.
        let l = list(&dir, "sample.zip", 3, BUDGET).unwrap();
        assert_eq!((l.entries.len(), l.total), (3, Some(11)));
        assert!(render("sample.zip", &l, 16 * 1024).contains("11 entries"));

        let l = list(&dir, "sample.tar.gz", MAX_ENTRIES, Duration::ZERO).unwrap();
        assert_eq!(l.stopped, Some(Stopped::Time(Duration::ZERO)));
        assert!(render("sample.tar.gz", &l, 16 * 1024).contains("stopped after 0s"));
    }

    /// The output cap cuts the tree, keeping its head and counting the rest. It
    /// cannot hide an escaping entry, which the summary already named.
    #[test]
    fn the_output_cap_keeps_the_head_and_never_hides_an_escape() {
        let dir = staged("cap");
        let l = list(&dir, "sample.zip", MAX_ENTRIES, BUDGET).unwrap();
        let text = render("sample.zip", &l, 700);
        assert!(
            text.len() <= 700,
            "within the cap, tail line included: {}",
            text.len()
        );
        assert!(text.contains("app-1.2.3/"), "the head is kept: {text}");
        assert!(text.contains("more entries not shown"), "{text}");
        assert!(
            !text.contains("/etc/cron.d/evil  ! outside"),
            "cut from the tree: {text}"
        );
        assert!(
            text.contains("!   /etc/cron.d/evil"),
            "but named in the summary: {text}"
        );
    }

    #[test]
    fn a_file_that_is_not_what_its_name_says_is_refused_plainly() {
        let dir = staged("magic");
        let e = list(&dir, "gzip-named.zip", MAX_ENTRIES, BUDGET)
            .unwrap_err()
            .to_string();
        assert!(e.contains("named like a zip but is gzip data"), "{e}");
        let e = list(&dir, "zip-named.tar.gz", MAX_ENTRIES, BUDGET)
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("named like a tar.gz but is a zip archive"),
            "{e}"
        );

        let a = ARTIFACTS.dir(&dir);
        std::fs::write(a.join("notes.txt"), "hello").unwrap();
        std::fs::write(a.join("old.tar.bz2"), "BZh9").unwrap();
        for name in ["notes.txt", "old.tar.bz2"] {
            let e = list(&dir, name, MAX_ENTRIES, BUDGET)
                .unwrap_err()
                .to_string();
            assert!(
                e.contains(".tar.gz, .tgz and .zip are supported"),
                "{name}: {e}"
            );
        }
    }

    /// The containment boundary is `store`'s and is tested there; this only
    /// checks the listing goes through it.
    #[test]
    fn a_name_outside_the_artifacts_directory_is_refused() {
        let dir = staged("outside");
        std::fs::write(
            dir.join("secret.zip"),
            include_bytes!("testdata/sample.zip"),
        )
        .unwrap();
        let e = list(&dir, "../secret.zip", MAX_ENTRIES, BUDGET)
            .unwrap_err()
            .to_string();
        assert!(e.contains("artifact name must be"), "{e}");
    }

    /// A tarball concatenated from several gzip members — `cat a.gz b.gz`, or a
    /// parallel compressor. A single-member decoder stopped after the first and
    /// reported the listing complete, dropping the rest: here that is the
    /// installer and an escaping entry.
    #[test]
    fn every_gzip_member_is_read_not_just_the_first() {
        let dir = staged("multimember");
        let l = list(&dir, "multimember.tar.gz", MAX_ENTRIES, BUDGET).unwrap();
        assert_eq!(l.stopped, None);
        let paths: Vec<&str> = l.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "pkg/",
                "pkg/etc/",
                "pkg/etc/app.conf",
                "pkg/install.sh",
                "../evil"
            ],
            "the entries in the second member are there"
        );
        assert_eq!(entry(&l, "pkg/install.sh").mode, Some(0o755));
        assert!(
            render("multimember.tar.gz", &l, 16 * 1024)
                .contains("! 1 entry would be written outside")
        );
    }

    /// CPython compresses a symlink entry like any other in a deflated zip, so
    /// its target has to be inflated — without zip's own deflate feature, which
    /// this build leaves out.
    #[test]
    fn a_deflated_zip_symlink_still_shows_its_target() {
        let dir = staged("deflated-link");
        let l = list(&dir, "deflated-link.zip", MAX_ENTRIES, BUDGET).unwrap();
        assert_eq!(
            entry(&l, "pkg/lib/libfoo.so").kind,
            Kind::Symlink(Some("libfoo.so.1".into()))
        );
    }

    /// A zip written on Windows stores DOS attributes and no unix mode. zip's
    /// `unix_mode` makes one up — 775, 664 — and shown as real that would answer
    /// "is it executable?" with a guess. Unknown is shown as unknown.
    #[test]
    fn a_mode_the_archive_does_not_store_is_not_invented() {
        let dir = staged("dos");
        let l = list(&dir, "dos.zip", MAX_ENTRIES, BUDGET).unwrap();
        assert_eq!(entry(&l, "docs/").kind, Kind::Dir);
        assert_eq!(entry(&l, "docs/").mode, None);
        assert_eq!(entry(&l, "docs/setup.exe").mode, None);
        let text = render("dos.zip", &l, 16 * 1024);
        assert!(
            text.contains("f ?????????        2 B docs/setup.exe"),
            "{text}"
        );
    }

    #[test]
    fn modes_and_sizes_render_for_a_reader() {
        assert_eq!(perms(Some(0o755)), "rwxr-xr-x");
        assert_eq!(perms(Some(0o640)), "rw-r-----");
        assert_eq!(
            perms(None),
            "?????????",
            "unknown is not \"no permissions\""
        );
        assert_eq!(human(412), "412 B");
        assert_eq!(human(2 * 1024 * 1024 + 100_000), "2.1 MiB");
    }
}
