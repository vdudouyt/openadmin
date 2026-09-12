//! Small shared rendering helpers.

use ratatui::layout::{Constraint, Layout, Rect};

/// Truncate (with a trailing `…`) or right-pad `s` to exactly `w` columns.
pub fn pad(s: &str, w: usize) -> String {
    let len = s.chars().count();
    if len == w {
        s.to_string()
    } else if len < w {
        let mut t = s.to_string();
        t.push_str(&" ".repeat(w - len));
        t
    } else if w == 0 {
        String::new()
    } else if w == 1 {
        "…".to_string()
    } else {
        let mut t: String = s.chars().take(w - 1).collect();
        t.push('…');
        t
    }
}

/// Right-align `s` within `w`.
pub fn padl(s: &str, w: usize) -> String {
    let len = s.chars().count();
    if len >= w {
        return pad(s, w);
    }
    let mut t = " ".repeat(w - len);
    t.push_str(s);
    t
}

/// Greedy word-wrap to `width` columns, hard-breaking words longer than the
/// width. Honors existing newlines.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let wlen = word.chars().count();
            if wlen > width {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
                let mut chars: Vec<char> = word.chars().collect();
                while chars.len() > width {
                    lines.push(chars[..width].iter().collect());
                    chars.drain(..width);
                }
                cur = chars.iter().collect();
                continue;
            }
            let cur_len = cur.chars().count();
            let needed = if cur_len == 0 {
                wlen
            } else {
                cur_len + 1 + wlen
            };
            if needed > width {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            } else {
                if !cur.is_empty() {
                    cur.push(' ');
                }
                cur.push_str(word);
            }
        }
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// A centered rect of fixed size, clamped to `area`.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let [_, row, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(h),
        Constraint::Fill(1),
    ])
    .areas(area);
    let [_, cell, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(w),
        Constraint::Fill(1),
    ])
    .areas(row);
    cell
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn wraps_on_words_newlines_and_long_words() {
        assert_eq!(wrap("hello world foo", 11), vec!["hello world", "foo"]);
        assert_eq!(wrap("one\ntwo", 10), vec!["one", "two"]);
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap("", 10), vec![String::new()]);
    }
}
