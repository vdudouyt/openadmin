//! Screen 3 — agentic chat (`design/ui_kits/openadmin/ChatScreen.jsx`).
//! Rendering only; no model is called.

use crate::app::App;
use crate::app::chat::{PlanState, Turn};
use crate::ui::theme;
use crate::ui::widgets::{sanitize, wrap};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let [body, composer] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).areas(area);

    // Say plainly when there is no model rather than rendering an empty label.
    let right = if app.cfg.agent.configured() {
        format!(
            " {} · {} hosts in context ",
            app.cfg.agent.model,
            app.hosts.len()
        )
    } else {
        " no model set · F2 to configure ".to_string()
    };
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
                    Span::styled(sanitize(arg), theme::muted()),
                    Span::raw("  "),
                    Span::styled(status.glyph(), status_style(*status)),
                    Span::styled(format!(" {}", status.label()), status_style(*status)),
                ]));
                // Output comes from a machine being debugged: sanitized so its
                // escape sequences cannot drive this terminal, and wrapped so a
                // long line is readable rather than cut at the right edge.
                let body: Vec<String> = out
                    .iter()
                    .flat_map(|l| wrap(&sanitize(l), width.saturating_sub(2)))
                    .collect();
                let n = body.len();
                for (i, line) in body.iter().enumerate() {
                    let rule = if i + 1 == n { "└ " } else { "│ " };
                    lines.push(Line::from(vec![
                        Span::styled(rule, theme::border_idle()),
                        Span::styled(line.clone(), theme::muted()),
                    ]));
                }
                lines.push(Line::default());
            }
            Turn::Plan {
                id,
                title,
                steps,
                hosts,
                state,
            } => {
                let (glyph, label, style) = match state {
                    PlanState::Proposed => ("▸", "awaiting review", theme::proxied()),
                    PlanState::Rejected => ("×", "rejected", theme::muted()),
                    PlanState::Ran => ("✓", "ran", theme::ok()),
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("{glyph} plan #{id} "), style),
                    Span::styled(sanitize(title), theme::bright()),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("  ", theme::faint()),
                    Span::styled(
                        format!("{steps} step(s) · {hosts} host(s) · "),
                        theme::faint(),
                    ),
                    Span::styled(label, style),
                ]));
                lines.push(Line::default());
            }
        }
    }

    // A window over the transcript rather than a drain, so the operator can
    // look back at what a plan did. The maximum is written back because the
    // renderer is the only thing that knows how tall the transcript is.
    let h = inner.height as usize;
    let max_scroll = lines.len().saturating_sub(h);
    app.chat.max_scroll = max_scroll;
    let scroll = app.chat.scroll.min(max_scroll);
    app.chat.scroll = scroll;
    let end = lines.len() - scroll;
    let start = end.saturating_sub(h);
    let visible: Vec<Line> = lines[start..end].to_vec();
    f.render_widget(Paragraph::new(visible), inner);

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
    let hint = if app.busy {
        " ^C cancel   PgUp/PgDn scroll "
    } else {
        " Enter send   PgUp/PgDn scroll "
    };
    let block = Block::bordered()
        .border_style(theme::border_focused())
        .style(Style::new().bg(theme::BG_BASE))
        .title_bottom(Line::styled(hint, theme::faint()).right_aligned());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut spans = vec![Span::styled(
        "» ",
        theme::proxied().add_modifier(Modifier::BOLD),
    )];
    if app.busy && app.chat.draft.is_empty() {
        spans.push(Span::styled("working… ^C to stop", theme::proxied()));
        f.render_widget(Paragraph::new(Line::from(spans)), inner);
        return;
    }
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
