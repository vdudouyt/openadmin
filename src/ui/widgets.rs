//! Small shared rendering helpers.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::symbols::{line, scrollbar};
use ratatui::widgets::ScrollbarState;

/// The scrollbar beside a list or a terminal: a light line for the track and a
/// heavy one for the thumb, never the usual full block.
///
/// Both places it is used have full-width bars running up to it — the Hosts
/// cursor row, and whatever a program in a pane draws in inverse video, mc's
/// cursor line or vim's status line. A filled cell beside one joined the two
/// into a single shape; a line keeps a sliver of background between them and
/// reads as part of the frame. The begin and end arrows are never drawn.
pub const LINE_SCROLLBAR: scrollbar::Set = scrollbar::Set {
    track: line::VERTICAL,
    thumb: line::THICK_VERTICAL,
    begin: "",
    end: "",
};

/// A vertical scrollbar's numbers: where its thumb is drawn, and which scroll
/// position a thumb at a given row stands for, so a click or a drag can land
/// where it looks.
///
/// `thumb` restates ratatui-widgets' `Scrollbar::part_lengths` rather than
/// approximating it: a thumb drawn one row off from where a drag puts it is a
/// thumb that slips under the pointer. The test beside it renders ratatui's
/// own scrollbar and holds the two to the same cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollGeometry {
    /// Cells in the track: the bar's height, with no arrows drawn.
    pub track: usize,
    /// The last scroll position; 0 is the top.
    pub max: usize,
    /// How much is in view at once, in the same units as a position.
    pub viewport: usize,
}

impl ScrollGeometry {
    /// What ratatui draws from: `max + 1` positions, so the thumb sits flush
    /// against both ends.
    pub fn state(&self, position: usize) -> ScrollbarState {
        ScrollbarState::new(self.max + 1)
            .position(position)
            .viewport_content_length(self.viewport)
    }

    /// The thumb at `position`: its first row and its length.
    pub fn thumb(&self, position: usize) -> (usize, usize) {
        // Integer division rounding to nearest, as ratatui's does.
        let round = |n: usize, d: usize| (n + d / 2) / d;
        if self.track == 0 {
            return (0, 0);
        }
        let whole = self.max + self.viewport;
        if whole == 0 {
            return (0, self.track);
        }
        let len = round(self.viewport * self.track, whole).clamp(1, self.track);
        let start = round(position.min(self.max) * self.track, whole).min(self.track - len);
        (start, len)
    }

    /// The position whose thumb starts at `row`, or the nearest one that
    /// does. The first and last rows the thumb can reach are the first and
    /// last positions, so dragging to either end always gets there — with
    /// hundreds of lines to a row, the last row is not otherwise the end.
    pub fn position_at(&self, row: i32) -> usize {
        let (_, len) = self.thumb(0);
        let travel = self.track.saturating_sub(len);
        if row <= 0 || travel == 0 {
            return 0;
        }
        let row = row as usize;
        if row >= travel {
            return self.max;
        }
        // The thumb's first row never falls as the position rises, so the
        // first position that reaches `row` is found by halving.
        let (mut lo, mut hi) = (0, self.max);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.thumb(mid).0 >= row {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        if lo > 0 && row - self.thumb(lo - 1).0 < self.thumb(lo).0 - row {
            lo - 1
        } else {
            lo
        }
    }
}

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

/// Strip terminal control sequences from text that came from somewhere else.
///
/// Remote output is full of them — `journalctl` colours its levels, `ls
/// --color` colours its names — and ratatui does not filter: a `Span`'s
/// characters go into buffer cells and the backend writes them straight out.
/// An `ESC` from a host you are debugging would therefore be interpreted by
/// *your* terminal, which is a machine you did not intend to give control of.
///
/// Tabs become spaces because a raw tab in a cell breaks the column alignment
/// the whole grid depends on.
#[allow(dead_code)] // used by the chat transcript renderer, next commit
pub fn sanitize(text: &str) -> String {
    const TAB_STOP: usize = 8;
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                // CSI: ESC [ params… final-byte in 0x40..=0x7e.
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC: ESC ] … terminated by BEL or ST (ESC \).
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\x07' {
                            break;
                        }
                        if c == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                // Anything else is a two-character escape; both are dropped.
                _ => {}
            },
            '\t' => {
                let pad = TAB_STOP - (out.chars().count() % TAB_STOP);
                out.extend(std::iter::repeat_n(' ', pad));
            }
            // Other C0 and the C1 range carry no text.
            c if (c as u32) < 0x20 || ('\u{80}'..='\u{9f}').contains(&c) => {}
            '\u{7f}' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{LINE_SCROLLBAR, ScrollGeometry, sanitize, wrap};

    /// Tracks, histories and views of the sizes this app has: a short list, a
    /// panel of rows, a shell's thousands of lines of history.
    fn geometries() -> Vec<ScrollGeometry> {
        let mut out = Vec::new();
        for track in [1, 2, 3, 5, 8, 14, 29] {
            for max in [0, 1, 2, 3, 7, 29, 43, 200, 5000] {
                for viewport in [1, 3, track] {
                    out.push(ScrollGeometry {
                        track,
                        max,
                        viewport,
                    });
                }
            }
        }
        out
    }

    fn positions(g: &ScrollGeometry) -> Vec<usize> {
        let mut p: Vec<usize> = (0..=g.max.min(60)).collect();
        p.extend([g.max / 3, g.max / 2, g.max.saturating_sub(1), g.max]);
        p
    }

    /// Where a drag puts the thumb has to be where ratatui draws it, or the
    /// thumb slips under the pointer: rendered, cell for cell.
    #[test]
    fn the_thumb_is_where_ratatui_draws_it() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::widgets::{Scrollbar, ScrollbarOrientation, StatefulWidget};
        for g in geometries() {
            for pos in positions(&g) {
                let area = Rect::new(0, 0, 1, g.track as u16);
                let mut buf = Buffer::empty(area);
                let mut state = g.state(pos);
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .symbols(LINE_SCROLLBAR)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .render(area, &mut buf, &mut state);
                let drawn: Vec<usize> = (0..g.track)
                    .filter(|&y| buf[(0, y as u16)].symbol() == "┃")
                    .collect();
                let (start, len) = g.thumb(pos);
                assert_eq!(
                    (drawn.first().copied().unwrap_or(0), drawn.len()),
                    (start, len),
                    "{g:?} at {pos}"
                );
            }
        }
    }

    /// Every row the thumb can be dragged to maps back to a position drawn on
    /// that row, when one is; the ends are the first and last positions; and
    /// moving down never scrolls up.
    #[test]
    fn every_row_the_thumb_reaches_maps_back_to_it() {
        for g in geometries() {
            let (_, len) = g.thumb(0);
            let travel = g.track - len;
            assert_eq!(g.position_at(-3), 0, "{g:?}");
            assert_eq!(g.position_at(0), 0, "{g:?}");
            // A thumb that fills its track cannot move, so no row means the
            // end; the app does not drag one at all.
            if travel > 0 {
                assert_eq!(g.position_at(travel as i32), g.max, "{g:?}");
                assert_eq!(g.position_at(travel as i32 + 3), g.max, "{g:?}");
            }
            let mut last = 0;
            for row in 0..=travel {
                let pos = g.position_at(row as i32);
                assert!(pos >= last, "{g:?}: row {row} went back to {pos}");
                last = pos;
                if (0..=g.max).any(|p| g.thumb(p).0 == row) {
                    assert_eq!(g.thumb(pos).0, row, "{g:?}: row {row} -> {pos}");
                }
            }
        }
    }

    #[test]
    fn sanitize_strips_colour_without_eating_the_text() {
        // What `journalctl` and `ls --color` actually emit.
        assert_eq!(sanitize("\x1b[31mFAILED\x1b[0m"), "FAILED");
        assert_eq!(sanitize("\x1b[1;32m ok \x1b[m rest"), " ok  rest");
        assert_eq!(sanitize("plain"), "plain");
    }

    /// The reason this exists: a host being debugged must not be able to drive
    /// the operator's terminal.
    #[test]
    fn sanitize_defuses_screen_control() {
        // Clear screen, cursor home, scroll region, and a title-setting OSC.
        assert_eq!(sanitize("\x1b[2J\x1b[H\x1b[1;5rgotcha"), "gotcha");
        assert_eq!(sanitize("\x1b]0;pwned\x07after"), "after");
        assert_eq!(sanitize("\x1b]0;pwned\x1b\\after"), "after");
        // A bare escape, and a two-character sequence.
        assert_eq!(sanitize("a\x1bZb"), "ab");
        assert_eq!(sanitize("a\x1b"), "a");
        // No ESC survives, whatever the input.
        for evil in ["\x1b[2J", "\x1b]0;x\x07", "\x1b(B", "\x1b[?1049h"] {
            assert!(!sanitize(evil).contains('\x1b'), "{evil:?} leaked an ESC");
        }
    }

    #[test]
    fn sanitize_expands_tabs_and_drops_other_controls() {
        assert_eq!(sanitize("a\tb"), "a       b");
        assert_eq!(sanitize("ab\tc"), "ab      c");
        assert_eq!(sanitize("carriage\rreturn"), "carriagereturn");
        assert_eq!(sanitize("bell\x07"), "bell");
        assert_eq!(sanitize("nul\0byte"), "nulbyte");
    }

    #[test]
    fn sanitize_keeps_unicode() {
        assert_eq!(sanitize("héllo ✓ 日本"), "héllo ✓ 日本");
    }

    #[test]
    fn wraps_on_words_newlines_and_long_words() {
        assert_eq!(wrap("hello world foo", 11), vec!["hello world", "foo"]);
        assert_eq!(wrap("one\ntwo", 10), vec!["one", "two"]);
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap("", 10), vec![String::new()]);
    }
}
