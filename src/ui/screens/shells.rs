//! The open shells, tabs 3 and on (`design/ui_kits/openadmin/ShellsScreen.jsx`).
//!
//! This screen spends as little of the terminal on itself as it can: the shell
//! tabs live on the header row beside Hosts and Chat, and a tab holding a
//! single pane draws no title rule at all — the header tab already names the
//! host. What is left is one row of chrome.
//!
//! Pane rects come from `TerminalManager::pane_rects`, and each pane is cut into
//! title, terminal and scrollbar by `TerminalManager::pane_areas` — the same
//! functions that drive `sync_sizes`, so what is drawn and what the PTY believes
//! can never disagree.

use crate::app::{App, ScrollTarget};
use crate::term::manager::TerminalManager;
use crate::term::scrollback::{self, History};
use crate::ui::theme;
use crate::ui::widgets::{LINE_SCROLLBAR, ScrollGeometry};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation};
use tui_term::widget::{Cursor, PseudoTerminal};

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    // Nothing to draw with no shell open — and no tab leads here then.
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
        if let Some(bar) = render_pane(f, *rect, app, *id, focused, chrome) {
            app.regions.scrollbars.push((bar, ScrollTarget::Pane(*id)));
        }
    }
}

/// Returns the scrollbar's band when one is drawn, for the mouse to find.
fn render_pane(
    f: &mut Frame,
    rect: Rect,
    app: &App,
    id: u64,
    focused: bool,
    chrome: u16,
) -> Option<Rect> {
    let session = app.term.session(id)?;

    let areas = TerminalManager::pane_areas(rect, chrome);
    if chrome > 0 {
        render_pane_title(f, areas.title, session, focused, rect.width);
    }

    let Ok(mut parser) = session.parser().lock() else {
        return None;
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
    let h = history?;
    render_scrollbar(f, areas.bar, h, focused);
    (areas.bar.width > 0).then_some(areas.bar)
}

/// Where the view is in a pane's history. Nothing is drawn with no history, or
/// on the alternate screen, which has none — the column stays blank.
fn render_scrollbar(f: &mut Frame, area: Rect, h: History, focused: bool) {
    // vt100 counts back from the live screen and a scrollbar counts down from
    // the top, so the live screen is the last position. `lines + 1` positions
    // put the thumb flush against each end, as in the dialogs.
    let mut state = scrollbar_geometry(h, area.height).state(h.lines - h.offset);
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

/// A pane's scrollbar, `rows` tall: positions count down from the oldest line
/// held (0) to the live screen (`lines`), the reverse of vt100's offset.
pub fn scrollbar_geometry(h: History, rows: u16) -> ScrollGeometry {
    ScrollGeometry {
        track: rows as usize,
        max: h.lines,
        viewport: rows as usize,
    }
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
