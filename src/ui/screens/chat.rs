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
use ratatui::widgets::{Block, Padding, Paragraph};
use tui_textarea::WrapMode;

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    configure_composer(app);
    let wanted = app.chat.draft.measure(area.width).preferred_rows;
    let [body, composer] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(wanted)]).areas(area);
    // Recorded before any early return: the plan dialog is drawn over it.
    app.regions.chat_log = Some(body);

    // Say plainly when there is no model rather than rendering an empty label.
    let right = if app.cfg.agent.configured() {
        format!(
            " {} · {} hosts in context ",
            app.cfg.agent.model,
            app.hosts.len()
        )
    } else {
        " no model set · see [agent] in config.toml ".to_string()
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
    // Which transcript line carries the pending plan card, if any.
    let mut card_line: Option<usize> = None;
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
            // Every line of it, wrapped: the useful part of a backend error is
            // rarely the first line. Sanitized, because it is text a remote
            // server chose.
            Turn::Error(text) => {
                let mut first = true;
                for raw in text.lines() {
                    for l in wrap(&sanitize(raw), width.saturating_sub(2)) {
                        lines.push(Line::from(vec![
                            Span::styled(if first { "✕ " } else { "  " }, theme::err()),
                            Span::styled(l, theme::err()),
                        ]));
                        first = false;
                    }
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
                    PlanState::Proposed => {
                        ("▸", "awaiting review — F2, or click here", theme::proxied())
                    }
                    PlanState::Rejected => ("×", "rejected", theme::muted()),
                    PlanState::Ran => ("✓", "ran", theme::ok()),
                };
                if *state == PlanState::Proposed {
                    // Remembered so a click on the card opens the dialog; the
                    // y is fixed up after the scroll window is known.
                    card_line = Some(lines.len());
                }
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

    // The card is only clickable while it is actually on screen.
    app.regions.plan_card = card_line
        .filter(|l| (start..end).contains(l))
        .map(|l| Rect::new(inner.x, inner.y + (l - start) as u16, inner.width, 2));

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

/// Dress the composer and tell it how tall it may be.
///
/// Done before the layout is split, because the height comes from asking the
/// text area to measure itself — and it can only do that once it knows the
/// frame it is wearing, since the borders are rows too.
fn configure_composer(app: &mut App) {
    let hint = if app.busy {
        " ^C cancel   PgUp/PgDn scroll "
    } else if app.pending_plan.is_some() {
        " F2 review   ↵ send   ^J newline "
    } else {
        " ↵ send   ^J newline   PgUp/PgDn scroll "
    };
    let block = Block::bordered()
        .border_style(theme::border_focused())
        .style(Style::new().bg(theme::BG_BASE))
        // The same breathing room the dialog inputs have, so text does not sit
        // against the frame.
        .padding(Padding::horizontal(1))
        .title_bottom(Line::styled(hint, theme::faint()).right_aligned());

    let draft = &mut app.chat.draft;
    draft.set_block(block);
    draft.set_style(theme::body());
    draft.set_cursor_style(Style::new().bg(theme::ORANGE_BRIGHT).fg(theme::ORANGE_INK));
    draft.set_cursor_line_style(Style::default());
    draft.set_placeholder_text("ask the agent to inspect or change a host…");
    draft.set_placeholder_style(theme::faint());
    // A long line folds instead of scrolling sideways. Text that slides out of
    // view as you type is text you cannot re-read, and a prompt is written to
    // be re-read before it is sent. `Word` keeps words whole and splits only a
    // token too long to fit — a path or a URL, which is most of what gets
    // pasted here.
    draft.set_wrap_mode(WrapMode::Word);
    // The box grows with the draft and stops, after which it scrolls inside
    // itself: the rows come out of the transcript above, and a composer free
    // to eat the screen would be the worse trade.
    draft.set_min_rows(3);
    draft.set_max_rows(10);
}

fn render_composer(f: &mut Frame, area: Rect, app: &mut App) {
    // While a turn is running the composer says so rather than inviting input
    // it would only queue behind the model.
    if app.busy && app.chat.draft.is_empty() {
        let block = app.chat.draft.block().cloned();
        let inner = block.as_ref().map_or(area, |b| b.inner(area));
        if let Some(b) = block {
            f.render_widget(b, area);
        }
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("» ", theme::proxied().add_modifier(Modifier::BOLD)),
                Span::styled("working… ^C to stop", theme::proxied()),
            ])),
            inner,
        );
        return;
    }
    // The text area draws itself, wearing the frame set above: one orange box,
    // the same as every other input.
    f.render_widget(&app.chat.draft, area);
}
