//! Screen 3 — agentic chat (`design/ui_kits/openadmin/ChatScreen.jsx`).
//! Rendering only; no model is called.

use crate::app::App;
use crate::app::chat::Turn;
use crate::ui::theme;
use crate::ui::widgets::wrap;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let [body, composer] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).areas(area);

    let right = format!(" {} · {} hosts in context ", app.cfg.model, app.hosts.len());
    let block = Block::bordered()
        .border_style(theme::border_idle())
        .style(Style::new().bg(theme::BG_BASE))
        .title_top(Line::styled(
            " Agent ",
            theme::bright().add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::styled(right, theme::muted()).right_aligned());
    let inner = block.inner(body);
    f.render_widget(block, body);

    let width = inner.width.saturating_sub(2) as usize;
    if width < 8 || inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for turn in &app.chat.turns {
        match turn {
            Turn::User(text) => {
                for (i, l) in wrap(text, width.saturating_sub(2)).into_iter().enumerate() {
                    lines.push(Line::from(vec![
                        Span::styled(
                            if i == 0 { "» " } else { "  " },
                            theme::proxied().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(l, theme::bright()),
                    ]));
                }
                lines.push(Line::default());
            }
            Turn::Assistant(text) => {
                for l in wrap(text, width) {
                    lines.push(Line::styled(l, theme::body()));
                }
                lines.push(Line::default());
            }
            Turn::Tool {
                name,
                arg,
                status,
                out,
            } => {
                // A boxed subtree: header rule, then output under a tree rule.
                lines.push(Line::from(vec![
                    Span::styled("┌ ", theme::border_idle()),
                    Span::styled(name.clone(), theme::bright()),
                    Span::styled(" · ", theme::faint()),
                    Span::styled(arg.clone(), theme::muted()),
                    Span::raw("  "),
                    Span::styled(status.glyph(), status_style(*status)),
                    Span::styled(format!(" {}", status.label()), status_style(*status)),
                ]));
                let n = out.len();
                for (i, line) in out.iter().enumerate() {
                    let rule = if i + 1 == n { "└ " } else { "│ " };
                    lines.push(Line::from(vec![
                        Span::styled(rule, theme::border_idle()),
                        Span::styled(line.clone(), theme::muted()),
                    ]));
                }
                lines.push(Line::default());
            }
        }
    }

    // Pin to the bottom: the newest turn is the one worth seeing.
    let h = inner.height as usize;
    if lines.len() > h {
        lines.drain(..lines.len() - h);
    }
    f.render_widget(Paragraph::new(lines), inner);

    render_composer(f, composer, app);
}

fn status_style(s: crate::app::chat::ToolStatus) -> Style {
    use crate::app::chat::ToolStatus::*;
    match s {
        Ok => theme::ok(),
        Fail => theme::err(),
        Empty => theme::muted(),
        Running => theme::proxied(),
    }
}

fn render_composer(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered()
        .border_style(theme::border_focused())
        .style(Style::new().bg(theme::BG_BASE))
        .title_bottom(
            Line::styled(
                " Enter send   @ add host to context   ^R run command ",
                theme::faint(),
            )
            .right_aligned(),
        );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut spans = vec![Span::styled(
        "» ",
        theme::proxied().add_modifier(Modifier::BOLD),
    )];
    if app.chat.draft.is_empty() {
        spans.push(Span::styled(
            "ask the agent to inspect or change a host…",
            theme::faint(),
        ));
    } else {
        spans.push(Span::styled(app.chat.draft.clone(), theme::body()));
    }
    spans.push(Span::styled("█", theme::proxied()));
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}
