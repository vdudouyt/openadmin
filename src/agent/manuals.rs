//! Manuals the operator has written, in `<datadir>/manuals/`.
//!
//! The agent knows how to administer machines in general and nothing about *this*
//! fleet: that the standby is promoted with a particular script, that a config is
//! rolled in a particular order, who gets woken at 3am. A manual is how the
//! operator says so once instead of typing it into the chat every session.
//!
//! So a manual is the operator speaking, and the model is told to treat it that
//! way — where one covers the task, its way is the way, ahead of whatever the
//! model would otherwise have done. That is the opposite of how the output of a
//! `readonly_` probe is treated, which is data from a machine being diagnosed.
//!
//! Path handling is `store::MANUALS`, shared with `artifacts`. What is here is
//! what makes a manual a manual: a one-line description taken from the file, and
//! a bounded read of the whole of it.

use crate::agent::store::MANUALS;
use crate::ui::widgets::sanitize;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// One manual, as the index names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manual {
    /// Relative to the manuals directory, `/`-separated: `linux/tuning.md`.
    pub filename: String,
    /// The file's first meaningful line. What the model picks by.
    pub description: String,
}

/// How much of a file is read to describe it. Enough for a first line several
/// times over, so describing a hundred manuals never reads a hundred whole
/// files.
const SNIFF: usize = 4096;

/// The longest description carried into the index. The index sits in the system
/// prompt, one line per manual, and a manual whose first line is a paragraph
/// should not take the width of the screen with it.
const DESCRIPTION_MAX: usize = 160;

pub fn dir(datadir: &Path) -> PathBuf {
    MANUALS.dir(datadir)
}

/// Create the directory if it is missing, owner-only like `keys/`. Called at
/// startup, so the operator can see where a manual goes.
pub fn ensure_dir(datadir: &Path) -> Result<PathBuf> {
    MANUALS.ensure(datadir)
}

/// Every manual, recursively, sorted by name, with its description.
///
/// The `bool` is whether the scan stopped on a limit. A truncated index has to
/// say so, or the model concludes a manual does not exist when it does — and acts
/// on general knowledge where the operator had written down the answer.
pub fn list(datadir: &Path) -> Result<(Vec<Manual>, bool)> {
    let (names, truncated) = MANUALS.walk(datadir)?;
    let mut out = Vec::with_capacity(names.len());
    for filename in names {
        let Ok((real, _)) = MANUALS.resolve(datadir, &filename) else {
            continue;
        };
        // A file with no readable text is not a manual. Skipped rather than
        // listed with a line of mojibake for a description — and `read` refuses
        // it too, so the index never advertises what a fetch would turn down.
        let Some(head) = sniff(&real) else { continue };
        out.push(Manual {
            filename,
            description: describe(&head),
        });
    }
    Ok((out, truncated))
}

/// The first `SNIFF` bytes as text, or `None` if this is not a text file.
///
/// Valid UTF-8 is the test, not the absence of a NUL. A NUL is what `grep` looks
/// for, and it is nearly always there in a binary — but "nearly" put a PNG in the
/// index with a line of mojibake for a description, because forty bytes of
/// compressed data happened not to contain one. Arbitrary bytes are almost never
/// valid UTF-8, so this asks the question the reader will ask later anyway.
///
/// A multi-byte character can straddle the end of the chunk, which is a truncated
/// read and not a binary: an error with no `error_len` means the sequence was cut
/// off, so take the part that was whole.
fn sniff(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; SNIFF];
    let n = f.read(&mut buf).ok()?;
    buf.truncate(n);
    if buf.contains(&0) {
        return None;
    }
    match std::str::from_utf8(&buf) {
        Ok(text) => Some(text.to_string()),
        Err(e) if e.error_len().is_none() => {
            Some(String::from_utf8_lossy(&buf[..e.valid_up_to()]).into_owned())
        }
        Err(_) => None,
    }
}

/// A one-line description from the beginning of a manual.
///
/// "The first line" is the rule, but the first line of a real markdown file is
/// often `# Title` or the `---` of some front matter, and neither is a
/// description. So: the first line with something in it that is not only a
/// delimiter, with any heading marker taken off the front.
fn describe(head: &str) -> String {
    let line = head
        .lines()
        .map(str::trim)
        .find(|l| {
            !l.is_empty()
                // `---` and `===` are front-matter fences and setext underlines;
                // ``` opens a code block. None of them describes anything.
                && !l.chars().all(|c| c == '-')
                && !l.chars().all(|c| c == '=')
                && !l.starts_with("```")
        })
        .unwrap_or("");
    let line = line.trim_start_matches('#').trim();
    // A YAML front-matter `title:` names the manual; the word "title" does not.
    let line = match line.split_once(':') {
        Some((key, rest)) if key.eq_ignore_ascii_case("title") && !rest.trim().is_empty() => {
            rest.trim()
        }
        _ => line,
    };
    let line = sanitize(line);
    if line.chars().count() <= DESCRIPTION_MAX {
        return line;
    }
    let cut: String = line.chars().take(DESCRIPTION_MAX).collect();
    format!("{cut}…")
}

/// One manual, whole, for the model to read.
///
/// `cap` bounds what enters the context. Over it, the **head** is kept and the
/// rest is named — never the head-and-tail elision `exec::Collector` does for
/// command output. A manual is a procedure, and eliding its middle drops steps 4
/// to 7 in a way the model cannot see; truncating the end at least leaves it
/// knowing where it stopped.
pub fn read(datadir: &Path, name: &str, cap: usize) -> Result<String> {
    let (real, filename) = MANUALS.resolve(datadir, name)?;
    if sniff(&real).is_none() {
        bail!("{filename:?} is not a text file, so it is not a manual");
    }
    // Lossy rather than strict: `sniff` has already turned away what is not text,
    // and a single bad byte halfway down an otherwise readable manual should not
    // deny the model the rest of it — with a UTF-8 error for a reason, which is
    // not something it can act on.
    let raw = std::fs::read(&real).with_context(|| format!("read manual {filename:?}"))?;
    let body = sanitize_lines(&String::from_utf8_lossy(&raw));

    let kept: String = body.chars().take(cap).collect();
    if kept.len() == body.len() {
        return Ok(format!("{filename}\n\n{body}"));
    }
    // Back up to a line boundary, so the text does not stop mid-sentence when a
    // whole line is nearly free.
    let kept = match kept.rfind('\n') {
        Some(i) if i > kept.len() / 2 => &kept[..i],
        _ => &kept[..],
    };
    let dropped = body.len() - kept.len();
    Ok(format!(
        "{filename}\n\n{kept}\n\n[{dropped} more bytes of {filename} were not shown. This is \
         the beginning of the manual, not all of it — if what you need is further in, say so \
         rather than guessing.]"
    ))
}

/// Strip control sequences line by line, keeping the lines.
///
/// A manual's text reaches the model's context and the operator's terminal, and
/// nothing upstream sanitizes a tool result that is not a probe.
fn sanitize_lines(text: &str) -> String {
    text.lines().map(sanitize).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("openadmin-manuals-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, name: &str, body: &str) {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn a_manual_is_named_by_its_path_and_described_by_its_first_line() {
        let data = scratch("list");
        let m = ensure_dir(&data).unwrap();
        write(
            &m,
            "db-failover.md",
            "# Promoting the standby\n\nSteps:\n1. …\n",
        );
        write(&m, "linux/tuning.md", "Sysctls we set, and why\n");
        write(&m, "oncall.txt", "Who to wake, and when\n");

        let (found, truncated) = list(&data).unwrap();
        assert!(!truncated);
        let pairs: Vec<(&str, &str)> = found
            .iter()
            .map(|x| (x.filename.as_str(), x.description.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("db-failover.md", "Promoting the standby"),
                ("linux/tuning.md", "Sysctls we set, and why"),
                ("oncall.txt", "Who to wake, and when"),
            ]
        );
        // And a name it advertises is a name a fetch takes, including the one in
        // a subdirectory.
        for f in &found {
            read(&data, &f.filename, 4096)
                .unwrap_or_else(|e| panic!("{} was listed but will not fetch: {e}", f.filename));
        }
    }

    /// "The first line" is the rule, but the first line of a real markdown file
    /// is often a fence or an underline, and neither describes anything.
    #[test]
    fn the_description_skips_what_is_not_a_description() {
        assert_eq!(
            describe("# Promoting the standby\nbody\n"),
            "Promoting the standby"
        );
        assert_eq!(describe("## Deep heading\n"), "Deep heading");
        // YAML front matter: the title names the manual, the word "title" does not.
        assert_eq!(
            describe("---\ntitle: Sysctls we set\n---\nbody\n"),
            "Sysctls we set"
        );
        // A colon in an ordinary line is left alone.
        assert_eq!(describe("Ports: which and why\n"), "Ports: which and why");
        assert_eq!(describe("title:\nnext\n"), "title:");
        // A setext underline belongs to the line above it, which we already took.
        assert_eq!(
            describe("Rolling a config change\n=======\n"),
            "Rolling a config change"
        );
        // Leading blank lines are not the description.
        assert_eq!(describe("\n\n   Indented opener\n"), "Indented opener");
        // A fence first means the description is the line after it.
        assert_eq!(describe("```\ncode\n```\n"), "code");
        // Nothing at all is an empty description, not a panic.
        assert_eq!(describe(""), "");
        assert_eq!(describe("\n\n"), "");
        // Control sequences from a file never reach the prompt.
        assert_eq!(describe("\x1b[31mRed title\x1b[0m\n"), "Red title");
        // A paragraph for a first line is cut, and says it was.
        let long = "x".repeat(DESCRIPTION_MAX + 50);
        let d = describe(&long);
        assert_eq!(d.chars().count(), DESCRIPTION_MAX + 1);
        assert!(d.ends_with('…'), "{d}");
    }

    #[test]
    fn an_empty_manual_is_listed_with_no_description() {
        let data = scratch("blank");
        let m = ensure_dir(&data).unwrap();
        write(&m, "todo.md", "");
        let (found, _) = list(&data).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].filename, "todo.md");
        assert_eq!(found[0].description, "");
    }

    /// Neither listed nor readable, in that order: the index must never advertise
    /// what a fetch would turn down.
    #[test]
    fn a_binary_file_is_not_a_manual() {
        let data = scratch("binary");
        let m = ensure_dir(&data).unwrap();
        std::fs::write(m.join("logo.png"), [0x89, b'P', b'N', b'G', 0x00, 0x1a]).unwrap();
        write(&m, "real.md", "A real manual\n");

        let (found, _) = list(&data).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|x| x.filename.as_str())
                .collect::<Vec<_>>(),
            vec!["real.md"]
        );
        let e = read(&data, "logo.png", 4096).unwrap_err().to_string();
        assert!(e.contains("not a text file"), "{e}");
    }

    /// The bug a NUL test missed: compressed bytes often contain no NUL, and a
    /// PNG went into the index with a line of mojibake for a description, then
    /// failed its fetch with a UTF-8 error the model could do nothing with.
    /// Arbitrary bytes are almost never valid UTF-8, which is the better test.
    #[test]
    fn a_binary_with_no_nul_byte_is_still_not_a_manual() {
        let data = scratch("nonul");
        let m = ensure_dir(&data).unwrap();
        // No NUL anywhere, and not valid UTF-8 — a lone continuation byte cannot
        // start a sequence.
        std::fs::write(m.join("blob.bin"), [0xff, 0x81, 0xfe, 0x9c, 0x01, 0x7f]).unwrap();
        let (found, _) = list(&data).unwrap();
        assert!(found.is_empty(), "{found:?}");
        let e = read(&data, "blob.bin", 4096).unwrap_err().to_string();
        assert!(e.contains("not a text file"), "{e}");
    }

    /// A multi-byte character straddling the end of the sniffed chunk is a
    /// truncated read, not a binary.
    #[test]
    fn a_character_split_by_the_sniff_boundary_is_not_a_binary() {
        let data = scratch("straddle");
        let m = ensure_dir(&data).unwrap();
        // Pad so that a three-byte character starts one byte before the cut.
        let body = format!(
            "Padded opener
{}€ and after",
            "x".repeat(SNIFF - 16)
        );
        std::fs::write(m.join("wide.md"), &body).unwrap();
        let (found, _) = list(&data).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].description, "Padded opener");
        // And the whole thing still reads, character intact.
        let text = read(&data, "wide.md", 64 * 1024).unwrap();
        assert!(text.contains("€ and after"), "{text}");
    }

    #[test]
    fn a_fetch_returns_the_whole_manual_under_the_cap() {
        let data = scratch("read");
        let m = ensure_dir(&data).unwrap();
        write(&m, "short.md", "Title\n\nOne step, then another.\n");
        let text = read(&data, "short.md", 16 * 1024).unwrap();
        assert!(text.starts_with("short.md\n\n"), "it names itself: {text}");
        assert!(text.contains("One step, then another."), "{text}");
        assert!(!text.contains("not shown"), "nothing was dropped: {text}");
    }

    /// A procedure truncated in the middle loses steps four to seven without
    /// saying so, so the head is kept and the rest is named. This is the one
    /// place the codebase does *not* use the head-and-tail elision that command
    /// output gets.
    #[test]
    fn an_oversized_manual_keeps_its_beginning_and_says_what_it_dropped() {
        let data = scratch("big");
        let m = ensure_dir(&data).unwrap();
        let body: String = (0..400)
            .map(|i| format!("line {i} of the procedure\n"))
            .collect();
        write(&m, "long.md", &body);

        let text = read(&data, "long.md", 1024).unwrap();
        assert!(text.contains("line 0 of the procedure"), "the head is kept");
        assert!(!text.contains("line 399"), "the tail is not: {text}");
        assert!(
            text.contains("more bytes of long.md were not shown"),
            "{text}"
        );
        assert!(text.contains("not all of it"), "{text}");
        // Explicitly not the middle-elision marker `Captured` uses: a manual that
        // lost its middle would read as complete.
        assert!(!text.contains("output truncated"), "{text}");
        assert!(!text.contains("…\n"), "no elision in the body: {text}");
        // Cut at a line boundary, so it does not stop mid-word.
        let head = text.split("\n\n[").next().unwrap();
        assert!(
            head.ends_with("procedure"),
            "cut on a line: {:?}",
            &head[head.len() - 40..]
        );
    }

    #[test]
    fn a_missing_manual_is_refused_by_name() {
        let data = scratch("missing");
        ensure_dir(&data).unwrap();
        let e = read(&data, "nope.md", 4096).unwrap_err().to_string();
        assert!(e.contains("no manual named"), "{e}");
        // The escapes are `store`'s, but the noun is this store's.
        let e = read(&data, "../config.toml", 4096).unwrap_err().to_string();
        assert!(e.contains("manual name must be"), "{e}");
    }

    #[test]
    fn a_missing_directory_lists_nothing_rather_than_failing() {
        let data = scratch("absent");
        let (found, truncated) = list(&data).unwrap();
        assert!(found.is_empty());
        assert!(!truncated);
    }
}
