//! Scrolling a pane back through its history.
//!
//! vt100 keeps the history — `Config.scrollback` lines per session, the oldest
//! dropped once it is full — and knows how far back the view is. What it does
//! not do is decide when the wheel may move that view, or say how much history
//! there is. Both are here, as functions over the screen rather than methods
//! that lock, so the renderer can use them under the lock it already holds.

/// How much history a pane holds, and how far back its view is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct History {
    /// Lines that have scrolled off the top and been kept.
    pub lines: usize,
    /// How far back the view is, in lines: 0 is the live screen.
    pub offset: usize,
}

/// The pane's history, or `None` when there is none to show — nothing has
/// scrolled off yet, or the alternate screen is up. vt100 gives that grid no
/// history at all, which is what keeps mc, vim and less out of this.
///
/// Takes `&mut` only to read. vt100 keeps the stored line count private and
/// reveals it by clamping: an offset set past the end lands on the oldest line
/// held. So the offset is pushed to the end, read, and put back. Call it under
/// a lock the caller already holds; std's `Mutex` is not reentrant.
pub fn history(screen: &mut vt100::Screen) -> Option<History> {
    if screen.alternate_screen() {
        return None;
    }
    let offset = screen.scrollback();
    screen.set_scrollback(usize::MAX);
    let lines = screen.scrollback();
    screen.set_scrollback(offset);
    (lines > 0).then_some(History { lines, offset })
}

/// Move the view `lines` back in time; negative moves toward the live screen.
///
/// Refused where the wheel belongs to the program rather than to us:
/// - on the alternate screen, where a full-screen program is drawing. Nor is
///   the wheel turned into arrow keys there: mc and its like are not scrolled.
/// - when the program asked for mouse reports. Over the terminal it is sent
///   the wheel; over the pane's title or scrollbar it cannot be told a
///   position, but the wheel is still its, so the history stays put.
pub fn scroll(screen: &mut vt100::Screen, lines: isize) {
    if screen.alternate_screen() || screen.mouse_protocol_mode() != vt100::MouseProtocolMode::None {
        return;
    }
    // vt100 clamps at the oldest line; the live screen is 0.
    screen.set_scrollback(screen.scrollback().saturating_add_signed(lines));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A five-row screen that has been sent `n` numbered lines. The last line's
    /// newline leaves the cursor on an empty bottom row, so `n - 4` of them
    /// have scrolled into history.
    fn fed(n: usize) -> vt100::Parser {
        let mut p = vt100::Parser::new(5, 20, 100);
        feed(&mut p, 1..=n);
        p
    }

    fn feed(p: &mut vt100::Parser, lines: std::ops::RangeInclusive<usize>) {
        for i in lines {
            p.process(format!("line {i:02}\r\n").as_bytes());
        }
    }

    fn top_row(p: &vt100::Parser) -> String {
        p.screen()
            .contents()
            .lines()
            .next()
            .unwrap_or("")
            .to_string()
    }

    /// The count is read through vt100's clamp, which moves the view to read
    /// it — so it has to come back to where it was.
    #[test]
    fn history_counts_the_lines_held_and_leaves_the_view_where_it_was() {
        let mut p = fed(20);
        p.screen_mut().set_scrollback(3);
        assert_eq!(
            history(p.screen_mut()),
            Some(History {
                lines: 16,
                offset: 3
            })
        );
        assert_eq!(p.screen().scrollback(), 3, "the view was not moved");
        assert_eq!(top_row(&p), "line 14");
    }

    #[test]
    fn nothing_scrolled_off_means_no_history() {
        let mut p = fed(3);
        assert_eq!(history(p.screen_mut()), None);
    }

    #[test]
    fn the_wheel_stops_at_the_oldest_line_and_at_the_live_screen() {
        let mut p = fed(20);
        scroll(p.screen_mut(), 1000);
        assert_eq!(p.screen().scrollback(), 16, "no further back than held");
        assert_eq!(top_row(&p), "line 01");
        scroll(p.screen_mut(), -1000);
        assert_eq!(
            p.screen().scrollback(),
            0,
            "and no further forward than now"
        );
        scroll(p.screen_mut(), -3);
        assert_eq!(p.screen().scrollback(), 0);
    }

    /// What makes reading back usable while a command is still printing: each
    /// line pushed into history moves the offset with it, so the text in view
    /// holds still.
    #[test]
    fn output_while_scrolled_back_keeps_the_view_on_the_same_lines() {
        let mut p = fed(20);
        scroll(p.screen_mut(), 3);
        assert_eq!(top_row(&p), "line 14");

        feed(&mut p, 21..=22);
        assert_eq!(p.screen().scrollback(), 5, "the offset followed the output");
        assert_eq!(top_row(&p), "line 14", "and the view did not move");
    }

    /// The boundary the operator drew: a full-screen program is not scrolled.
    /// vt100 gives the alternate screen no history, and resets the shell's
    /// view to live on the way in.
    #[test]
    fn the_alternate_screen_has_no_history_and_does_not_scroll() {
        let mut p = fed(20);
        scroll(p.screen_mut(), 3);

        p.process(b"\x1b[?1049h");
        assert_eq!(history(p.screen_mut()), None);
        scroll(p.screen_mut(), 5);
        assert_eq!(p.screen().scrollback(), 0, "the wheel did nothing");

        // Leaving it, the shell's history is all still there.
        p.process(b"\x1b[?1049l");
        assert_eq!(
            history(p.screen_mut()),
            Some(History {
                lines: 16,
                offset: 0
            })
        );
    }

    /// A program that asked for the mouse owns the wheel, even where it cannot
    /// be told a position.
    #[test]
    fn a_program_that_asked_for_the_mouse_keeps_its_history_still() {
        let mut p = fed(20);
        p.process(b"\x1b[?1000h");
        scroll(p.screen_mut(), 3);
        assert_eq!(p.screen().scrollback(), 0);
    }
}
