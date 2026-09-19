//! Bulk import of qhostman's host lists.
//!
//! The format is qhostman's: four lines per host — name, address, login,
//! password — and a blank line between hosts. Structure follows cfdns's bulk
//! wizard (`/root/cfdns/src/app/bulk.rs`): the text is the state, and what it
//! would do is derived from it rather than stored, so the review is always of
//! the database as it is *now* — the agent can add a host while the dialog is
//! open, and a stale list would then offer to add it a second time.

use crate::db::model::{HostRecord, default_port, mount_for};
use tui_textarea::TextArea;

/// What a block is, in the order it says it: `BLOCK_LINES` of these.
pub const FIELDS: &str = "name, address, login, password";
const BLOCK_LINES: usize = 4;
const PROTO: &str = "ssh";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkStep {
    Paste,
    Review,
}

#[derive(Debug, Clone)]
pub struct BulkImport {
    pub step: BulkStep,
    pub text: TextArea<'static>,
    /// The review's first visible row. The renderer clamps it and writes back
    /// `max_scroll` and `page`, being the only thing that knows the height.
    pub scroll: usize,
    pub max_scroll: usize,
    pub page: usize,
}

impl BulkImport {
    pub fn new() -> Self {
        BulkImport {
            step: BulkStep::Paste,
            text: TextArea::default(),
            scroll: 0,
            max_scroll: 0,
            page: 1,
        }
    }

    pub fn joined(&self) -> String {
        self.text.lines().join("\n")
    }

    pub fn scroll_by(&mut self, delta: isize) {
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .min(self.max_scroll);
    }
}

/// One block of the paste, and what importing it would do.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    New(HostRecord),
    /// A host by this name is already in the database.
    Exists(String),
    /// An earlier block in the same paste already adds this name.
    Repeated {
        name: String,
        first_line: usize,
    },
    Invalid(String),
}

/// Where the block starts in the text, 1-based, and what it amounts to.
pub type Block = (usize, Row);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub new: usize,
    pub skipped: usize,
    pub problems: usize,
    /// The first problem's line, so the operator knows where to look.
    pub first_problem: Option<usize>,
}

/// Split the text into blocks of non-blank lines, each with its first line.
///
/// A lone CR is a line break too: a terminal sends CR for a newline inside a
/// bracketed paste, and `str::lines` only knows LF and CRLF.
fn blocks(text: &str) -> Vec<(usize, Vec<String>)> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out: Vec<(usize, Vec<String>)> = Vec::new();
    let mut open = false;
    for (i, line) in text.split('\n').enumerate() {
        let line = line.trim();
        if line.is_empty() {
            open = false;
            continue;
        }
        if !open {
            out.push((i + 1, Vec::new()));
            open = true;
        }
        if let Some((_, lines)) = out.last_mut() {
            lines.push(line.to_string());
        }
    }
    out
}

/// A block of the wrong size, with what would fix it.
///
/// A whole multiple of four is almost always hosts that lost the blank line
/// between them, and saying so is the difference between a fix and a hunt.
fn wrong_size(n: usize) -> String {
    if n > BLOCK_LINES && n.is_multiple_of(BLOCK_LINES) {
        format!(
            "{n} lines — looks like {} hosts; put a blank line between them",
            n / BLOCK_LINES
        )
    } else {
        format!("{} — expected {BLOCK_LINES}: {FIELDS}", count(n, "line"))
    }
}

/// `1 host`, `2 hosts`.
pub fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// Every block of `text`, classified against the hosts that already exist.
///
/// Names match exactly, as every other lookup by name does. A name already in
/// the database is skipped rather than updated, so pasting the same list again
/// after adding to it adds only what is new.
pub fn review(text: &str, existing: &[HostRecord], mount_prefix: &str) -> Vec<Block> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    blocks(text)
        .into_iter()
        .map(|(line, lines)| {
            let row = match <[String; BLOCK_LINES]>::try_from(lines) {
                Err(lines) => Row::Invalid(wrong_size(lines.len())),
                Ok([name, addr, login, pass]) => {
                    if existing.iter().any(|h| h.name == name) {
                        Row::Exists(name)
                    } else if let Some((_, first)) = seen.iter().find(|(n, _)| *n == name) {
                        Row::Repeated {
                            name,
                            first_line: *first,
                        }
                    } else {
                        seen.push((name.clone(), line));
                        Row::New(HostRecord {
                            mount_point: mount_for(mount_prefix, &name),
                            name,
                            proto: PROTO.to_string(),
                            addr,
                            port: default_port(PROTO),
                            login,
                            pass,
                            ..Default::default()
                        })
                    }
                }
            };
            (line, row)
        })
        .collect()
}

pub fn counts(rows: &[Block]) -> Counts {
    let mut c = Counts::default();
    for (line, row) in rows {
        match row {
            Row::New(_) => c.new += 1,
            Row::Exists(_) | Row::Repeated { .. } => c.skipped += 1,
            Row::Invalid(_) => {
                c.problems += 1;
                c.first_problem.get_or_insert(*line);
            }
        }
    }
    c
}

/// The records an import would write.
pub fn records(rows: &[Block]) -> Vec<HostRecord> {
    rows.iter()
        .filter_map(|(_, row)| match row {
            Row::New(rec) => Some(rec.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = "mydomain-s5000\n1.2.3.4\nroot\n123123\n\n\
                           mydomain-s5001\n2.3.4.5\nroot\n123123\n";

    fn names(rows: &[Block]) -> Vec<String> {
        records(rows).into_iter().map(|r| r.name).collect()
    }

    #[test]
    fn qhostmans_example_is_two_hosts() {
        let rows = review(EXAMPLE, &[], "/net");
        assert_eq!(names(&rows), ["mydomain-s5000", "mydomain-s5001"]);
        let recs = records(&rows);
        assert_eq!(recs[0].addr, "1.2.3.4");
        assert_eq!(recs[1].addr, "2.3.4.5");
        assert_eq!(recs[1].login, "root");
        assert_eq!(recs[1].pass, "123123");
        assert_eq!(rows[1].0, 6, "the second block starts on line 6");
        assert_eq!(
            counts(&rows),
            Counts {
                new: 2,
                ..Default::default()
            }
        );
    }

    #[test]
    fn a_record_gets_what_the_form_would_have_given_it() {
        let rows = review(EXAMPLE, &[], "/net");
        let r = &records(&rows)[0];
        assert_eq!(r.id, 0, "new rows are inserted, never updated");
        assert_eq!(r.proto, "ssh", "stored lowercased, as qhostman does");
        assert_eq!(r.port, 22);
        assert_eq!(r.mount_point, "/net/mydomain-s5000");
        assert!(r.key_name.is_empty() && !r.proxy);
    }

    /// Pastes arrive with CR line endings, trailing spaces and extra blank
    /// lines, none of which is a problem with the list.
    #[test]
    fn line_endings_padding_and_extra_blank_lines_are_tolerated() {
        let messy = "\r\n  mydomain-s5000  \r1.2.3.4\t\rroot\r123123 \r\r \r\r\
                     mydomain-s5001\r\n2.3.4.5\r\nroot\r\n123123";
        let rows = review(messy, &[], "/net");
        assert_eq!(names(&rows), ["mydomain-s5000", "mydomain-s5001"]);
        assert_eq!(records(&rows)[0].addr, "1.2.3.4");
        assert_eq!(records(&rows)[0].pass, "123123");
        assert_eq!(rows[0].0, 2, "a leading blank line is still counted");
    }

    #[test]
    fn a_block_of_the_wrong_size_says_where_and_what_is_expected() {
        let text = "a\n1.1.1.1\nroot\npw\n\nb\n2.2.2.2\nroot\npw\nextra\n";
        let rows = review(text, &[], "/net");
        let c = counts(&rows);
        assert_eq!((c.new, c.problems, c.first_problem), (1, 1, Some(6)));
        let (line, Row::Invalid(msg)) = &rows[1] else {
            panic!("{rows:?}")
        };
        assert_eq!(*line, 6);
        assert_eq!(msg, "5 lines — expected 4: name, address, login, password");

        let rows = review("lonely", &[], "/net");
        assert_eq!(
            rows[0].1,
            Row::Invalid("1 line — expected 4: name, address, login, password".into())
        );
    }

    #[test]
    fn two_hosts_without_a_blank_line_are_named_as_such() {
        let rows = review(&EXAMPLE.replace("\n\n", "\n"), &[], "/net");
        assert_eq!(
            rows,
            [(
                1,
                Row::Invalid("8 lines — looks like 2 hosts; put a blank line between them".into())
            )]
        );
    }

    #[test]
    fn existing_and_repeated_names_are_skipped() {
        let existing = [HostRecord {
            name: "mydomain-s5000".into(),
            ..Default::default()
        }];
        let text = format!("{EXAMPLE}\nmydomain-s5001\n9.9.9.9\nadmin\nx\n");
        let rows = review(&text, &existing, "/net");
        assert_eq!(rows[0].1, Row::Exists("mydomain-s5000".into()));
        assert_eq!(names(&rows), ["mydomain-s5001"]);
        assert_eq!(
            rows[2].1,
            Row::Repeated {
                name: "mydomain-s5001".into(),
                first_line: 6
            }
        );
        assert_eq!(
            records(&rows)[0].addr,
            "2.3.4.5",
            "the first of a repeated name wins"
        );
        let c = counts(&rows);
        assert_eq!((c.new, c.skipped, c.problems), (1, 2, 0));
    }

    #[test]
    fn nothing_pasted_is_nothing_to_do() {
        assert!(review("", &[], "/net").is_empty());
        assert!(review("\n \n\r\n", &[], "/net").is_empty());
    }
}
