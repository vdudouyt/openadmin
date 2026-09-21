//! Screen 2 — open shells (`design/ui_kits/openadmin/ShellsScreen.jsx`).
//!
//! This screen spends as little of the terminal on itself as it can: the shell
//! tabs live on the header row beside the screen tabs, and a tab holding a
//! single pane draws no title rule at all — the header tab already names the
//! host. What is left is one row of chrome.
//!
//! Pane rects come from `TerminalManager::pane_rects`, and each pane is cut into
//! title, terminal and scrollbar by `TerminalManager::pane_areas` — the same
//! functions that drive `sync_sizes`, so what is drawn and what the PTY believes
//! can never disagree.

use crate::app::App;
use crate::term::manager::TerminalManager;
use crate::term::scrollback::{self, History};
use crate::ui::theme;
use crate::ui::widgets::LINE_SCROLLBAR;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use tui_term::widget::{Cursor, PseudoTerminal};

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    if app.term.is_empty() {
        render_empty(f, area);
        return;
    }

    let Some(tab) = app.term.active_tab() else {
        return;
    };
    let panes = tab.panes.clone();
    let focus = tab.focus;
    let single = panes.len() == 1;

    let rects = TerminalManager::pane_rects(area, panes.len());
    // Drive the PTY geometry from the layout we are about to draw.
    app.term.sync_sizes(&rects);
    // One source of truth for whether a title row exists — the same value the
    // resizer and the mouse mapping use.
    let chrome = app.term.pane_chrome_rows();

    app.regions.panes.clear();
    for (i, (id, rect)) in panes.iter().zip(&rects).enumerate() {
        // The whole pane, title and scrollbar included: the wheel over either
        // still means this pane.
        app.regions.panes.push((*rect, i));
        let focused = single || focus == i;
        render_pane(f, *rect, app, *id, focused, chrome);
    }
}

fn render_pane(f: &mut Frame, rect: Rect, app: &App, id: u64, focused: bool, chrome: u16) {
    let Some(session) = app.term.session(id) else {
        return;
    };

    let areas = TerminalManager::pane_areas(rect, chrome);
    if chrome > 0 {
        render_pane_title(f, areas.title, session, focused, rect.width);
    }

    let Ok(mut parser) = session.parser().lock() else {
        return;
    };
    // Read under the lock the terminal is drawn with, so the bar and the text
    // describe the same moment.
    let history = scrollback::history(parser.screen_mut());
    let mut cursor = Cursor::default().style(theme::proxied());
    // Only the focused pane shows a cursor, and never once the child is gone.
    // Scrolled back, tui-term moves it down with the text, off the bottom.
    if !focused || session.has_exited() || parser.screen().hide_cursor() {
        cursor.hide();
    }
    // Drawn into the terminal's own area: the widget clears what it is given,
    // and the scrollbar column is not part of it.
    f.render_widget(
        PseudoTerminal::new(parser.screen())
            .cursor(cursor)
            .style(Style::new().bg(theme::BG_BASE)),
        areas.term,
    );
    if let Some(h) = history {
        render_scrollbar(f, areas.bar, h, focused);
    }
}

/// Where the view is in a pane's history. Nothing is drawn with no history, or
/// on the alternate screen, which has none — the column stays blank.
fn render_scrollbar(f: &mut Frame, area: Rect, h: History, focused: bool) {
    // vt100 counts back from the live screen and a scrollbar counts down from
    // the top, so the live screen is the last position. `lines + 1` positions
    // put the thumb flush against each end, as in the dialogs.
    let mut state = ScrollbarState::new(h.lines + 1)
        .position(h.lines - h.offset)
        .viewport_content_length(area.height as usize);
    // The title rule's convention: orange on the pane that has the keyboard.
    let thumb = if focused {
        theme::ORANGE
    } else {
        theme::LINE_STRONG
    };
    // A line for a thumb, not a block: programs draw full-width inverse bars
    // right up to this column (`LINE_SCROLLBAR` says why).
    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .symbols(LINE_SCROLLBAR)
            .begin_symbol(None)
            .end_symbol(None)
            .track_style(theme::border_idle())
            .thumb_style(Style::new().fg(thumb)),
        area,
        &mut state,
    );
}

/// The one-row rule above a pane: `─ host ───────── label ─`.
///
/// Only drawn when it says something: a stacked group needs it to tell panes
/// apart, and an exited pane needs somewhere to report that it is gone.
fn render_pane_title(
    f: &mut Frame,
    area: Rect,
    session: &crate::term::session::TerminalSession,
    focused: bool,
    width: u16,
) {
    let exited = session.has_exited();
    let border = if focused {
        theme::border_focused()
    } else {
        theme::border_idle()
    };
    let right = if exited {
        "process exited · click × to close"
    } else if focused {
        "active"
    } else {
        "click to focus"
    };

    let title = format!(" {} ", session.title);
    let label = format!(" {right} ");
    let used = title.chars().count() + label.chars().count() + 2;
    let fill = (width as usize).saturating_sub(used);
    let title_style = if focused {
        theme::bright().add_modifier(Modifier::BOLD)
    } else {
        theme::muted()
    };
    let label_style = if exited {
        theme::warn()
    } else {
        theme::faint()
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("─", border),
            Span::styled(title, title_style),
            Span::styled("─".repeat(fill), border),
            Span::styled(label, label_style),
            Span::styled("─", border),
        ])),
        area,
    );
}

/// Columns one tab occupies: `" ● title 3 × "`.
///
/// Counts chars, not display cells, so a CJK nickname measures narrow. The
/// strip is clipped to its band, so the consequence is a truncated tab rather
/// than one that reaches into the screen tabs.
fn tab_width(tab: &crate::term::manager::Tab) -> u16 {
    let count = if tab.group {
        1 + tab.panes.len().to_string().chars().count()
    } else {
        0
    };
    // " " + "●" + " title" + count + " ×" + " "
    (6 + tab.title.chars().count() + count) as u16
}

/// The shell tabs, rendered into whatever the header's screen tabs left over.
///
/// Called by `ui::header::render`, which owns the row; this only ever paints
/// inside `area`, because a `Paragraph` pads its whole rect and would blank the
/// screen tabs otherwise.
pub fn render_tab_strip(f: &mut Frame, area: Rect, app: &mut App) {
    app.regions.shell_tabs.clear();
    app.regions.shell_closes.clear();
    if app.term.is_empty() || area.width == 0 {
        return;
    }

    let widths: Vec<u16> = app.term.tabs.iter().map(tab_width).collect();
    let n = widths.len();
    let active = app.term.active.unwrap_or(0).min(n - 1);
    let (start, end) = window(&widths, active, area.width);

    let mut spans: Vec<Span> = Vec::new();
    let mut x = area.x;
    // Overflow marks: without a status line, a tab silently falling off the
    // edge looks unrecoverable.
    if start > 0 {
        spans.push(Span::styled("‹", theme::faint()));
        x += 1;
    }

    for (i, tab) in app.term.tabs.iter().enumerate().take(end).skip(start) {
        let is_active = app.term.active == Some(i);
        let width = widths[i];
        // A tab wider than the space left is drawn truncated (ratatui clips the
        // Paragraph), so its hitbox must be clipped to match — otherwise it
        // would reach into the screen tabs and shadow the escape route.
        let visible = width.min((area.x + area.width).saturating_sub(x));
        if visible == 0 {
            break;
        }
        let rect = Rect::new(x, area.y, visible, 1);
        let hovered = app.is_hovered(rect);
        app.regions.shell_tabs.push((rect, i));

        let base = if is_active {
            theme::sel_focused()
        } else if hovered {
            theme::hover()
        } else {
            Style::new().fg(theme::FG_MUTED)
        };
        // Green dot for a single host, yellow for a group.
        let dot_fg = if tab.group {
            theme::YELLOW_MARK
        } else {
            theme::GREEN
        };
        let dot = if is_active { base } else { base.fg(dot_fg) };

        spans.push(Span::styled(" ", base));
        spans.push(Span::styled("●", dot));
        spans.push(Span::styled(format!(" {}", tab.title), base));
        if tab.group {
            spans.push(Span::styled(
                format!(" {}", tab.panes.len()),
                base.fg(theme::FG_FAINT),
            ));
        }
        // The × only appears under the pointer, or on the active tab — and
        // only when it was not truncated away.
        let close_rect = Rect::new(x + width - 2, area.y, 2, 1);
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

        spans.push(Span::raw(" "));
        // Advance by exactly what was drawn, so the registered hitboxes cannot
        // drift away from the glyphs.
        x += visible;
    }

    if end < n {
        spans.push(Span::styled("›", theme::faint()));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
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

fn render_empty(f: &mut Frame, area: Rect) {
    let block = Block::bordered()
        .border_style(theme::border_focused())
        .style(Style::new().bg(theme::BG_BASE))
        .title_top(Line::styled(
            " Shells ",
            theme::bright().add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::default(),
        Line::styled("  No open shells.", theme::muted()),
        Line::default(),
        Line::from(vec![
            Span::styled("  Go to ", theme::faint()),
            Span::styled("Hosts", theme::proxied()),
            Span::styled(" and press ", theme::faint()),
            Span::styled("F5", theme::bright()),
            Span::styled(" on a host to open one.", theme::faint()),
        ]),
        Line::default(),
        Line::from(vec![
            Span::styled("  Mark several hosts with ", theme::faint()),
            Span::styled("Insert", Style::new().fg(theme::YELLOW_MARK)),
            Span::styled(" first and F5 opens a single", theme::faint()),
        ]),
        Line::styled(
            "  grouped tab with one stacked pane per host.",
            theme::faint(),
        ),
    ];
    f.render_widget(Paragraph::new(lines), inner);
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
