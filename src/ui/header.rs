//! Header band: one tab strip from the left edge — the OpenAdmin logo, which
//! is the Hosts tab, then `2 Chat`, then one tab per open shell. All on a
//! single row, so the chrome costs as little of the terminal as possible.
//!
//! One strip rather than screen tabs plus a shell-tab strip, so `Alt`+digit
//! and `Alt+←/→` reach a shell the same way they reach a screen. The logo
//! carries no digit: it is the first tab, and `Alt+1` is Hosts.
//!
//! The design's mark is a solid orange block two rows tall
//! (`design/assets/cloudflare-ascii-logo.txt`); at this height it is the same
//! honest primitive drawn once.

use crate::app::{App, Tab};
use crate::ui::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Columns between two tabs.
const GAP: u16 = 1;

const WORDMARK: &str = "OpenAdmin";

/// Columns `n` takes written out.
fn digits(n: usize) -> u16 {
    n.to_string().chars().count() as u16
}

/// Columns the logo tab occupies: `" ███ OpenAdmin "`, or `" ███ "` with the
/// wordmark shed.
fn logo_width(wordmark: bool) -> u16 {
    if wordmark {
        5 + 1 + WORDMARK.chars().count() as u16
    } else {
        5
    }
}

/// Columns the Chat tab occupies: `" 2 Chat "`, or `" 2 "` with its label shed.
fn chat_width(label: bool) -> u16 {
    if label {
        3 + 1 + Tab::Chat.label().chars().count() as u16
    } else {
        3
    }
}

/// Columns one shell tab occupies: `" 3 ● title 2 × "`.
///
/// Counts chars, not display cells, so a CJK nickname measures narrow. The
/// strip is clipped to its band, so the consequence is a truncated tab rather
/// than one that reaches past the edge.
fn shell_width(number: usize, tab: &crate::term::manager::Tab) -> u16 {
    let count = if tab.group {
        1 + tab.panes.len().to_string().chars().count()
    } else {
        0
    };
    // " N" + " ●" + " title" + count + " ×" + " "
    1 + digits(number) + (6 + tab.title.chars().count() + count) as u16
}

/// The strip, from the left edge, each tab registering its own hitbox.
///
/// The logo and Chat are always drawn — they are the way back from anywhere.
/// When room runs short the shells go first, down to the active one; then the
/// wordmark, leaving the mark; then Chat's label. The shell tabs get what is
/// left, as a window around the active one with `‹ ›` marking what fell off.
pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let shell_widths: Vec<u16> = app
        .term
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| GAP + shell_width(i + 3, t))
        .collect();
    let n = shell_widths.len();
    let active = app.term.active.unwrap_or(0).min(n.saturating_sub(1));
    // The least the shells need: the active one.
    let shells_min = shell_widths.get(active).copied().unwrap_or(0);

    let pinned_total = |wordmark: bool, label: bool| logo_width(wordmark) + GAP + chat_width(label);
    let Some((wordmark, label)) = [(true, true), (false, true), (false, false)]
        .into_iter()
        .find(|&(w, l)| pinned_total(w, l) + shells_min <= area.width)
        .or(Some((false, false)).filter(|&(w, l)| pinned_total(w, l) <= area.width))
    else {
        return;
    };
    let pinned_w = pinned_total(wordmark, label);
    let avail = area.width - pinned_w;
    let (start, end) = if n == 0 || avail == 0 {
        (0, 0)
    } else {
        window(&shell_widths, active, avail)
    };
    let mark_l = u16::from(start > 0);
    let mark_r = u16::from(end < n && end > start);
    let shells_w = (shell_widths[start..end].iter().sum::<u16>() + mark_l + mark_r).min(avail);
    let total = pinned_w + shells_w;

    let right = area.x + area.width;
    let y = area.y;
    let current = app.current_tab();
    let mut spans: Vec<Span> = Vec::new();
    let mut x = area.x;

    // The logo, which is the Hosts tab. On the orange of the showing tab the
    // mark turns light, or it would vanish into its own background.
    let w = logo_width(wordmark);
    let rect = Rect::new(x, y, w, 1);
    app.regions.screen_tabs.push((rect, Tab::Hosts));
    let (mark, word) = if current == 0 {
        let on = theme::primary_btn();
        (on.fg(theme::FG_BRIGHT), on)
    } else if app.is_hovered(rect) {
        (
            theme::hover().fg(theme::ORANGE),
            theme::hover().add_modifier(Modifier::BOLD),
        )
    } else {
        (
            theme::proxied(),
            theme::bright().add_modifier(Modifier::BOLD),
        )
    };
    spans.push(Span::styled(" ", word));
    spans.push(Span::styled("███", mark));
    if wordmark {
        spans.push(Span::styled(format!(" {WORDMARK}"), word));
    }
    spans.push(Span::styled(" ", word));
    x += w;

    spans.push(Span::raw(" ".repeat(GAP as usize)));
    x += GAP;
    let w = chat_width(label);
    let rect = Rect::new(x, y, w, 1);
    app.regions.screen_tabs.push((rect, Tab::Chat));
    let (num_style, lab_style) = tab_styles(current == 1, app.is_hovered(rect));
    spans.push(Span::styled(" 2", num_style));
    if label {
        spans.push(Span::styled(format!(" {}", Tab::Chat.label()), lab_style));
    }
    spans.push(Span::styled(" ", lab_style));
    x += w;

    if mark_l > 0 {
        spans.push(Span::styled("‹", theme::faint()));
        x += 1;
    }
    let shown = app.term.tabs.iter().zip(&shell_widths).enumerate();
    for (i, (tab, w)) in shown.take(end).skip(start) {
        spans.push(Span::raw(" ".repeat(GAP as usize)));
        x += GAP;
        let width = w - GAP;
        // A tab wider than the space left is drawn truncated (ratatui clips
        // the Paragraph), so its hitbox must be clipped to match.
        let visible = width.min(right.saturating_sub(x));
        if visible == 0 {
            break;
        }
        let rect = Rect::new(x, y, visible, 1);
        let hovered = app.is_hovered(rect);
        let is_active = current == i + 2;
        app.regions.screen_tabs.push((rect, Tab::Shell(i)));
        let (num_style, base) = tab_styles(is_active, hovered);

        // Green dot for a single host, yellow for a group.
        let dot_fg = if tab.group {
            theme::YELLOW_MARK
        } else {
            theme::GREEN
        };
        let dot = if is_active { base } else { base.fg(dot_fg) };

        spans.push(Span::styled(format!(" {}", i + 3), num_style));
        spans.push(Span::styled(" ", base));
        spans.push(Span::styled("●", dot));
        spans.push(Span::styled(format!(" {}", tab.title), base));
        if tab.group {
            spans.push(Span::styled(
                format!(" {}", tab.panes.len()),
                base.patch(theme::faint()),
            ));
        }
        // The × only appears under the pointer, or on the shell showing — and
        // only when it was not truncated away.
        let close_rect = Rect::new(x + width - 3, y, 2, 1);
        if (hovered || is_active) && visible == width {
            app.regions.shell_closes.push((close_rect, i));
            let close_style = if app.is_hovered(close_rect) {
                base.fg(theme::RED)
            } else {
                base
            };
            spans.push(Span::styled(" ×", close_style));
        } else {
            spans.push(Span::styled("  ", base));
        }
        spans.push(Span::styled(" ", base));
        // Advance by exactly what was drawn, so the registered hitboxes cannot
        // drift away from the glyphs.
        x += visible;
    }
    if mark_r > 0 {
        spans.push(Span::styled("›", theme::faint()));
    }

    // Draw into exactly the columns measured: a Paragraph patches its style
    // across its whole rect.
    let strip = Rect::new(area.x, area.y, total, area.height);
    f.render_widget(Paragraph::new(Line::from(spans)), strip);
}

/// `(number, label)` styles for a tab that is showing, hovered, or neither.
fn tab_styles(active: bool, hovered: bool) -> (Style, Style) {
    if active {
        (theme::primary_btn(), theme::primary_btn())
    } else if hovered {
        (theme::hover().fg(theme::ORANGE_BRIGHT), theme::hover())
    } else {
        (theme::proxied(), theme::muted())
    }
}

/// Choose the run of tabs to show, always including `active`.
///
/// The host table's centred window (`ui::screens::hosts`) assumes uniform row
/// heights; tabs have varying widths, so this grows outward from the active tab
/// instead, reserving a column for each overflow mark it will need.
fn window(widths: &[u16], active: usize, avail: u16) -> (usize, usize) {
    let n = widths.len();
    let used = |s: usize, e: usize| -> u16 {
        widths[s..e].iter().copied().fold(0u16, u16::saturating_add)
            + u16::from(s > 0)
            + u16::from(e < n)
    };

    let (mut start, mut end) = (active, active + 1);
    loop {
        let mut grew = false;
        if end < n && used(start, end + 1) <= avail {
            end += 1;
            grew = true;
        }
        if start > 0 && used(start - 1, end) <= avail {
            start -= 1;
            grew = true;
        }
        if !grew {
            break;
        }
    }
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::window;

    #[test]
    fn everything_fits_when_there_is_room() {
        assert_eq!(window(&[10, 10, 10], 0, 40), (0, 3));
    }

    /// The active tab must be rendered whatever else is dropped.
    #[test]
    fn the_active_tab_is_always_inside_the_window() {
        let widths = [10u16; 8];
        for active in 0..8 {
            let (s, e) = window(&widths, active, 25);
            assert!(
                s <= active && active < e,
                "active {active} fell outside {s}..{e}"
            );
        }
    }

    #[test]
    fn the_window_grows_to_fill_the_space_but_no_further() {
        // 25 columns, 10 per tab, one column for each overflow mark.
        let widths = [10u16; 8];
        let (s, e) = window(&widths, 4, 25);
        let body: u16 = widths[s..e].iter().sum();
        let marks = u16::from(s > 0) + u16::from(e < widths.len());
        assert!(body + marks <= 25, "{s}..{e} overflows");
        assert_eq!(e - s, 2, "two tabs plus both marks is the most that fits");
    }

    /// A tab wider than the strip still gets shown rather than nothing.
    #[test]
    fn an_oversized_tab_is_still_selected() {
        assert_eq!(window(&[50, 50], 1, 10), (1, 2));
    }
}
