//! Screen 2 — open shells (`design/ui_kits/openadmin/ShellsScreen.jsx`).
//!
//! A single-host tab is one full-width pane; a grouped tab stacks one titled
//! pane per host. The pane rects come from `TerminalManager::pane_rects`, the
//! same function that drives `sync_sizes`, so what is drawn and what the PTY
//! believes can never disagree.

use crate::app::App;
use crate::term::manager::{PANE_CHROME_ROWS, TerminalManager};
use crate::ui::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use tui_term::widget::{Cursor, PseudoTerminal};

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    if app.term.is_empty() {
        render_empty(f, area);
        return;
    }

    let [tabbar, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    render_tab_bar(f, tabbar, app);

    let Some(tab) = app.term.active_tab() else {
        return;
    };
    let panes = tab.panes.clone();
    let focus = tab.focus;
    let single = panes.len() == 1;

    let rects = TerminalManager::pane_rects(body, panes.len());
    // Drive the PTY geometry from the layout we are about to draw.
    app.term.sync_sizes(&rects);

    app.regions.panes.clear();
    for (i, (id, rect)) in panes.iter().zip(&rects).enumerate() {
        app.regions.panes.push((*rect, i));
        let focused = single || focus == i;
        render_pane(f, *rect, app, *id, focused);
    }
}

fn render_pane(f: &mut Frame, rect: Rect, app: &App, id: u64, focused: bool) {
    let Some(session) = app.term.session(id) else {
        return;
    };

    let [title_a, term_a] =
        Layout::vertical([Constraint::Length(PANE_CHROME_ROWS), Constraint::Min(0)]).areas(rect);

    let exited = session.has_exited();
    let border = if focused {
        theme::border_focused()
    } else {
        theme::border_idle()
    };
    let right = if exited {
        "process exited · Esc-4 to close".to_string()
    } else if focused {
        "active".to_string()
    } else {
        "click to focus".to_string()
    };

    // A one-row title rule: ┌─ host ─────── label ─┐ flattened to a single line.
    let title = format!(" {} ", session.title);
    let label = format!(" {right} ");
    let used = title.chars().count() + label.chars().count() + 2;
    let fill = (rect.width as usize).saturating_sub(used);
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
        title_a,
    );

    let Ok(parser) = session.parser().lock() else {
        return;
    };
    let mut cursor = Cursor::default().style(theme::proxied());
    // Only the focused pane shows a cursor, and never once the child is gone.
    if !focused || exited || parser.screen().hide_cursor() {
        cursor.hide();
    }
    f.render_widget(
        PseudoTerminal::new(parser.screen())
            .cursor(cursor)
            .style(Style::new().bg(theme::BG_BASE)),
        term_a,
    );
}

fn render_tab_bar(f: &mut Frame, area: Rect, app: &mut App) {
    app.regions.shell_tabs.clear();
    app.regions.shell_closes.clear();

    let active = app.term.active;
    let mut spans: Vec<Span> = Vec::new();
    let mut x = area.x;

    for (i, tab) in app.term.tabs.iter().enumerate() {
        let is_active = active == Some(i);
        let count = if tab.group {
            format!(" {}", tab.panes.len())
        } else {
            String::new()
        };
        // " ● title 3 × "
        let width = (4 + tab.title.chars().count() + count.chars().count() + 2) as u16;
        if x + width > area.x + area.width {
            break;
        }
        let rect = Rect::new(x, area.y, width, 1);
        let hovered = app.is_hovered(rect);
        app.regions.shell_tabs.push((rect, i));

        let base = if is_active {
            theme::sel_focused()
        } else if hovered {
            theme::hover()
        } else {
            theme::statusbar()
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
        if !count.is_empty() {
            spans.push(Span::styled(count, base.fg(theme::FG_FAINT)));
        }
        // The × only appears under the pointer, as in the mockup.
        let close_rect = Rect::new(x + width - 2, area.y, 2, 1);
        if hovered || is_active {
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
        spans.push(Span::styled(" ", theme::statusbar()));
        x += width + 1;
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::statusbar()),
        area,
    );
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
