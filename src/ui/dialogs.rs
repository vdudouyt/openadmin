//! Modals (`design/ui_kits/openadmin/HostDialogs.jsx`).
//!
//! The toolkit is cfdns's: `Clear` + a bordered `Block` + `centered(w, h)` +
//! a `Vec<Line>` in one `Paragraph`, with `button_row` registering its own
//! hitboxes (`/root/cfdns/src/ui/dialogs.rs:16-51`).

use crate::agent::plan::StepKind;
use crate::app::approve::Row;
use crate::app::form::FormField;
use crate::app::{App, Click};
use crate::ui::theme;
use crate::ui::widgets::{centered, sanitize, wrap};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph};
use tui_input::Input;

fn modal_block(title: &str) -> Block<'static> {
    Block::bordered()
        .border_style(theme::border_focused())
        .style(Style::new().bg(theme::BG_PANEL).fg(theme::FG))
        .padding(Padding::symmetric(2, 1))
        .title_top(Line::styled(
            format!(" {title} "),
            theme::bright().add_modifier(Modifier::BOLD),
        ))
}

/// A double-line frame marks a destructive confirmation, per the design.
fn danger_block(title: &str) -> Block<'static> {
    modal_block(title)
        .border_type(BorderType::Double)
        .border_style(Style::new().fg(theme::ORANGE_DIM))
}

/// Right-aligned buttons that register their own hitboxes, replicating
/// `Line::right_aligned()`'s geometry so clicks land where they look.
fn button_row(
    app: &mut App,
    inner: Rect,
    line: u16,
    btns: &[(String, Style, Click)],
) -> Line<'static> {
    const SEP: u16 = 3;
    let widths: Vec<u16> = btns
        .iter()
        .map(|(l, _, _)| l.chars().count() as u16)
        .collect();
    let content: u16 = widths.iter().sum::<u16>() + SEP * btns.len().saturating_sub(1) as u16;
    let y = inner.y + line;
    let mut x = inner.x + inner.width.saturating_sub(content);
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (label, style, click)) in btns.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
            x += SEP;
        }
        let w = widths[i];
        let rect = Rect::new(x, y, w, 1);
        app.regions.clicks.push((rect, *click));
        let style = if app.is_hovered(rect) {
            style.add_modifier(Modifier::REVERSED)
        } else {
            *style
        };
        spans.push(Span::styled(label.clone(), style));
        x += w;
    }
    Line::from(spans).right_aligned()
}

/// The frame every text input wears: the dialog's own background inside an
/// orange box, with the label in the top border.
///
/// Only the label goes in a border. Notes used to hang in the bottom one,
/// right-aligned, and they read as a break in the frame rather than a caption:
/// the line stops, grey text runs across the gap, and the box looks
/// unfinished. A label is short and sits in the corner where a frame expects a
/// title; a sentence is neither.
///
/// Inputs used to be an inset *well* — a patch of `BG_INSET`, two shades
/// darker than the panel — which read as a hole cut in the dialog rather than
/// a control sitting on it. The Chat composer (`ui/screens/chat.rs`) was the
/// one place that did it the other way, and it is the one that looked right.
///
/// Every input is orange, so focus cannot be carried by hue: the focused box
/// is bright and bold where the others are dim, and it is the only one
/// wearing a cursor.
fn input_block(label: &str, focused: bool) -> Block<'static> {
    let (border, label_style) = if focused {
        (
            Style::new().fg(theme::ORANGE_BRIGHT),
            Style::new()
                .fg(theme::ORANGE_BRIGHT)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (Style::new().fg(theme::ORANGE_DIM), theme::muted())
    };
    Block::bordered()
        .border_style(border)
        .style(Style::new().bg(theme::BG_PANEL))
        .padding(Padding::horizontal(1))
        .title_top(Line::styled(format!(" {label} "), label_style))
}

/// One text input. `area` is the whole box, borders included — three rows.
fn input_box(
    f: &mut Frame,
    area: Rect,
    label: &str,
    input: &Input,
    focused: bool,
    placeholder: &str,
) {
    let block = input_block(label, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if input.value().is_empty() && !focused {
        f.render_widget(
            Paragraph::new(Line::styled(placeholder.to_string(), theme::faint())),
            inner,
        );
        return;
    }
    // A value wider than the box scrolls, and the caret is what it follows;
    // one column is kept for the caret itself so it is never flush against the
    // border with nothing under it. Both numbers come from `tui-input`, in
    // display columns, so a double-width character does not slide the caret
    // away from the text it points at.
    let scroll = input.visual_scroll(inner.width.saturating_sub(1) as usize);
    f.render_widget(
        Paragraph::new(Line::styled(input.value().to_string(), theme::body()))
            .scroll((0, scroll as u16)),
        inner,
    );
    if !focused {
        return;
    }
    // The caret is a reversed cell rather than a block glyph, so the character
    // under it stays readable — which is what tells you whether Delete is
    // about to take the one you meant.
    let col = input.visual_cursor().saturating_sub(scroll) as u16;
    if col < inner.width {
        f.buffer_mut()[(inner.x + col, inner.y)]
            .set_style(Style::new().bg(theme::ORANGE_BRIGHT).fg(theme::ORANGE_INK));
    }
}

/// The protocol cycler wears the same box, so a row of controls reads as a row
/// of controls rather than a field and a gadget.
fn cycle_box(f: &mut Frame, area: Rect, label: &str, value: &str, focused: bool) {
    let block = input_block(label, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let (l, r) = if focused {
        ("◂ ", " ▸")
    } else {
        ("  ", " ▾")
    };
    let style = if focused {
        theme::bright()
    } else {
        theme::body()
    };
    f.render_widget(
        Paragraph::new(Line::styled(format!("{l}{value}{r}"), style)),
        inner,
    );
}

/// Split a row into a wide field and a narrow one beside it.
///
/// On a narrow terminal the preferred width is given up rather than the
/// second field: squeezing a box down to `┌ ┐` shows nothing and still costs
/// the row. Below twice the usable minimum the pair does not fit at all, and
/// the second width comes back zero for the caller to skip.
fn columns(total: u16, first: u16) -> (u16, u16) {
    const MIN: u16 = 14;
    if total < MIN * 2 + 1 {
        return (total, 0);
    }
    let first = first.clamp(MIN, total - MIN - 1);
    (first, total - first - 1)
}

pub fn host_form(f: &mut Frame, app: &mut App) {
    let Some(form) = app.form.clone() else { return };
    let area = f.area();
    let rect = centered(area, 72, 20);
    f.render_widget(Clear, rect);
    let block = modal_block(if form.is_edit() {
        "Edit Host"
    } else {
        "Add Host"
    });
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 6 || inner.width < 24 {
        return;
    }

    // While the mount point is automatic the field shows what the name would
    // generate; the caret still belongs to the field's own input.
    let mount_shown = if form.mount_auto {
        Input::new(app.auto_mount_for(form.name.value())).with_cursor(form.mount.cursor())
    } else {
        form.mount.clone()
    };
    let f_is = |x: FormField| form.focus == x;

    // Boxed fields cost three rows each, so the pairs that belong together sit
    // together: a port beside its address, a password beside its login. That
    // buys back the height the borders spend and reads better than a column of
    // eight lonely boxes.
    let bottom = inner.y + inner.height;
    let mut y = inner.y;
    let place = |h: u16, y: &mut u16| -> Option<Rect> {
        if *y + h > bottom {
            return None;
        }
        let r = Rect::new(inner.x, *y, inner.width, h);
        *y += h;
        Some(r)
    };

    if let Some(row) = place(3, &mut y) {
        let (a, b) = columns(row.width, 40);
        let name = Rect::new(row.x, row.y, a, 3);
        app.regions
            .clicks
            .push((name, Click::FocusField(FormField::Name)));
        input_box(
            f,
            name,
            "Host name",
            &form.name,
            f_is(FormField::Name),
            "nickname, e.g. web-01",
        );
        if b > 0 {
            let ty = Rect::new(row.x + a + 1, row.y, b, 3);
            app.regions.clicks.push((ty, Click::CycleType(1)));
            cycle_box(f, ty, "Type", &form.proto, f_is(FormField::Type));
        }
    }

    if let Some(row) = place(3, &mut y) {
        let (a, b) = columns(row.width, 48);
        let addr = Rect::new(row.x, row.y, a, 3);
        app.regions
            .clicks
            .push((addr, Click::FocusField(FormField::Addr)));
        input_box(
            f,
            addr,
            "Address",
            &form.addr,
            f_is(FormField::Addr),
            "host or IP",
        );
        if b > 0 {
            let port = Rect::new(row.x + a + 1, row.y, b, 3);
            app.regions
                .clicks
                .push((port, Click::FocusField(FormField::Port)));
            input_box(f, port, "Port", &form.port, f_is(FormField::Port), "22");
        }
    }

    if let Some(row) = place(3, &mut y) {
        app.regions
            .clicks
            .push((row, Click::FocusField(FormField::Mount)));
        // Whether the mount point still follows the host name is state, not
        // advice: typing sets it, clearing the field restores it, and without
        // a word for it the operator cannot tell which one they are looking
        // at. It goes in the label, which is where a box carries its name.
        input_box(
            f,
            row,
            if form.mount_auto {
                "Mount point · auto"
            } else {
                "Mount point · overridden"
            },
            &mount_shown,
            f_is(FormField::Mount),
            "/net/<name>",
        );
    }

    if let Some(row) = place(3, &mut y) {
        let (a, b) = columns(row.width, 33);
        let login = Rect::new(row.x, row.y, a, 3);
        app.regions
            .clicks
            .push((login, Click::FocusField(FormField::Login)));
        input_box(
            f,
            login,
            "Login",
            &form.login,
            f_is(FormField::Login),
            "user",
        );
        if b > 0 {
            let pass = Rect::new(row.x + a + 1, row.y, b, 3);
            app.regions
                .clicks
                .push((pass, Click::FocusField(FormField::Pass)));
            // Bullets, one per character, with the caret where it really is:
            // moving through a password you cannot see is the case that needs
            // a caret most.
            let masked = Input::new("•".repeat(form.pass.value().chars().count()))
                .with_cursor(form.pass.cursor());
            input_box(
                f,
                pass,
                "Password",
                &masked,
                f_is(FormField::Pass),
                "optional with a key",
            );
        }
    }

    // Whatever is left holds the key row, the reminder and the buttons.
    if y >= bottom {
        return;
    }
    let tail = Rect::new(inner.x, y, inner.width, bottom - y);
    let mut lines: Vec<Line> = Vec::new();
    {
        app.regions
            .clicks
            .push((Rect::new(tail.x, tail.y, tail.width, 1), Click::GenKey));
        let mut spans = vec![Span::styled("SSH key", theme::muted()), Span::raw("   ")];
        if form.has_key() {
            spans.push(Span::styled("✓ ", theme::ok()));
            spans.push(Span::styled("key installed", theme::body()));
            spans.push(Span::raw("    "));
            spans.push(Span::styled(
                "[ Show public key ]",
                theme::bright().fg(theme::ORANGE_BRIGHT),
            ));
        } else {
            spans.push(Span::styled(
                "[ Generate SSH key ]",
                Style::new().fg(theme::ORANGE_BRIGHT),
            ));
            spans.push(Span::raw("   "));
            spans.push(Span::styled("prints the public key", theme::faint()));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::default());
    lines.push(Line::styled(
        "Tab next field   ←/→ change type   Enter save   Esc cancel",
        theme::faint(),
    ));

    let save_label = if form.is_edit() {
        " Save "
    } else {
        " Add Host "
    };
    lines.push(button_row(
        app,
        tail,
        lines.len() as u16,
        &[
            (
                save_label.to_string(),
                theme::primary_btn(),
                Click::Key(ratatui::crossterm::event::KeyCode::Enter),
            ),
            (
                "[ Cancel ]".to_string(),
                theme::body(),
                Click::Key(ratatui::crossterm::event::KeyCode::Esc),
            ),
        ],
    ));

    f.render_widget(Paragraph::new(lines), tail);
}

/// The plan confirmation dialog: one row per step, one per host under it, each
/// with its own checkbox.
///
/// The script body is rendered in full and wrapped, never elided. A plan the
/// operator cannot read end to end is one they cannot judge, and that would
/// turn the whole confirmation into theatre.
pub fn confirm_plan(f: &mut Frame, app: &mut App) {
    let Some(sel) = app.plan.clone() else { return };
    let area = f.area();
    let width = 78u16.min(area.width);
    let height = (area.height as i32 - 4).clamp(12, 34) as u16;
    let rect = centered(area, width, height);
    f.render_widget(Clear, rect);
    // Double-line frame: this is the destructive confirmation.
    let block = danger_block("Confirm Plan");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 6 {
        return;
    }

    let (steps_on, runs) = sel.counts();
    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(
                sanitize(&sel.plan.title),
                theme::bright().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("   plan #{}", sel.plan.id), theme::faint()),
        ]),
        Line::styled("Nothing has run yet.", theme::muted()),
        Line::default(),
    ];

    let rows = sel.rows();
    let body_w = inner.width.saturating_sub(6) as usize;
    // `(line, row index)` so the window can be anchored on the cursor's row.
    let mut body: Vec<(Line, Option<usize>)> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        let focused = r == sel.cursor;
        match *row {
            Row::Step(i) => {
                let step = &sel.plan.steps[i];
                let on = sel.step_on[i];
                body.push((
                    Line::from(vec![
                        Span::styled(if focused { "▸" } else { " " }, theme::proxied()),
                        Span::styled(
                            if on { "[x] " } else { "[ ] " },
                            if on { theme::ok() } else { theme::faint() },
                        ),
                        Span::styled(format!("{} ", i + 1), theme::faint()),
                        Span::styled(
                            sanitize(&step.summary),
                            if on { theme::bright() } else { theme::muted() },
                        ),
                        Span::styled(format!("  ({})", step.kind.label()), theme::faint()),
                    ]),
                    Some(r),
                ));
                // What the step actually does, verbatim.
                match &step.kind {
                    StepKind::Scriptlet { script } => {
                        for l in script.lines().flat_map(|l| wrap(&sanitize(l), body_w)) {
                            body.push((
                                Line::from(vec![
                                    Span::styled("    │ ", theme::border_idle()),
                                    Span::styled(l, theme::muted()),
                                ]),
                                None,
                            ));
                        }
                    }
                    StepKind::Upload { artifact } => {
                        // The whole destination, not just its directory: a staged
                        // path is kept on the far side, so `nginx/site.conf`
                        // lands in a `nginx/` of its own and the operator should
                        // see where the file actually goes before approving it.
                        body.push((
                            Line::from(vec![
                                Span::styled("    │ ", theme::border_idle()),
                                Span::styled(sanitize(artifact), theme::muted()),
                                Span::styled(
                                    format!(
                                        " → /tmp/openadmin-plan-{}/{}",
                                        sel.plan.id,
                                        sanitize(artifact)
                                    ),
                                    theme::faint(),
                                ),
                            ]),
                            None,
                        ));
                    }
                }
            }
            Row::Host(i, j) => {
                let id = sel.plan.steps[i].hosts[j];
                let name = app
                    .hosts
                    .iter()
                    .find(|h| h.id == id)
                    .map(|h| h.name.clone())
                    .unwrap_or_else(|| format!("#{id} (gone)"));
                let on = sel.host_on[i][j];
                let active = sel.host_active(i, j);
                body.push((
                    Line::from(vec![
                        Span::styled(if focused { "▸" } else { " " }, theme::proxied()),
                        Span::styled("    ", theme::faint()),
                        Span::styled(
                            if on { "[x] " } else { "[ ] " },
                            if active { theme::ok() } else { theme::faint() },
                        ),
                        Span::styled(
                            name,
                            if active {
                                theme::body()
                            } else {
                                theme::faint()
                            },
                        ),
                    ]),
                    Some(r),
                ));
            }
        }
    }

    // A plan the operator cannot read end to end is one they cannot judge, so
    // the body scrolls rather than truncating. The window follows the cursor.
    let header = lines.len();
    let footer = 3; // blank + hint + buttons
    let view = (inner.height as usize)
        .saturating_sub(header + footer)
        .max(1);
    let cursor_line = body
        .iter()
        .position(|(_, r)| *r == Some(sel.cursor))
        .unwrap_or(0);
    let max_scroll = body.len().saturating_sub(view);
    let mut scroll = sel.scroll.min(max_scroll);
    if cursor_line < scroll {
        scroll = cursor_line;
    } else if cursor_line >= scroll + view {
        scroll = cursor_line + 1 - view;
    }
    if let Some(s) = app.plan.as_mut() {
        s.scroll = scroll;
    }
    let shown = body.len().saturating_sub(scroll).min(view);
    for (k, (line, row)) in body.iter().skip(scroll).take(view).enumerate() {
        let y = inner.y + (header + k) as u16;
        // Hitboxes are registered only for rows actually on screen, so a click
        // can never land on a step scrolled out of view.
        if let Some(r) = row {
            match rows[*r] {
                Row::Step(i) => app.regions.clicks.push((
                    Rect::new(inner.x, y, inner.width, 1),
                    Click::ToggleStep(i as u16),
                )),
                Row::Host(i, j) => app.regions.clicks.push((
                    Rect::new(inner.x, y, inner.width, 1),
                    Click::ToggleHost(i as u16, j as u16),
                )),
            }
        }
        lines.push(line.clone());
    }
    let hidden = body.len() - scroll - shown;

    lines.push(Line::default());
    // Kept short enough to fit the dialog: a clipped hint helps nobody.
    // The disarmed hint is the only place the extra ↵ is explained, so it says
    // so plainly rather than leaving the first one looking broken.
    let mut hint = if sel.armed {
        "↑↓ move · Space toggle · a/x · ↵ run · F2 hide · Esc reject".to_string()
    } else {
        "↑↓ move · Space toggle · a/x · ↵ twice to run · F2 hide · Esc reject".to_string()
    };
    if hidden > 0 {
        hint.push_str(&format!(" · ↓{hidden} more"));
    }
    lines.push(Line::styled(hint, theme::faint()));

    // A dead button registers no hitbox, which is the cleanest way to make it
    // dead — there is nothing to click.
    let run_label = if runs == 0 {
        " Nothing selected ".to_string()
    } else {
        format!(" Run {steps_on} step(s) on {runs} host(s) ")
    };
    let buttons: Vec<(String, ratatui::style::Style, Click)> = if runs == 0 {
        vec![(
            "[ Reject ]".to_string(),
            theme::body(),
            Click::Key(ratatui::crossterm::event::KeyCode::Esc),
        )]
    } else {
        vec![
            (
                run_label.clone(),
                theme::danger_btn(),
                Click::Key(ratatui::crossterm::event::KeyCode::Enter),
            ),
            (
                "[ Reject ]".to_string(),
                theme::body(),
                Click::Key(ratatui::crossterm::event::KeyCode::Esc),
            ),
        ]
    };
    if runs == 0 {
        lines.push(Line::styled(run_label, theme::faint()).right_aligned());
    }
    let row = button_row(app, inner, lines.len() as u16, &buttons);
    lines.push(row);

    f.render_widget(Paragraph::new(lines), inner);
}

pub fn confirm_delete(f: &mut Frame, app: &mut App) {
    let names: Vec<String> = app
        .pending_delete
        .iter()
        .filter_map(|id| app.hosts.iter().find(|h| h.id == *id))
        .map(|h| h.name.clone())
        .collect();
    if names.is_empty() {
        return;
    }
    let many = names.len() > 1;

    let area = f.area();
    let rect = centered(area, 58, (9 + names.len().min(5)) as u16);
    f.render_widget(Clear, rect);
    let block = danger_block("Confirm Delete");
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut lines = vec![Line::from(vec![
        Span::styled("Delete ", theme::body()),
        Span::styled(
            if many {
                format!("{} hosts", names.len())
            } else {
                format!("\"{}\"", names[0])
            },
            if many {
                Style::new()
                    .fg(theme::YELLOW_MARK)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme::bright().add_modifier(Modifier::BOLD)
            },
        ),
        Span::styled(" ?", theme::body()),
    ])];

    if many {
        for n in names.iter().take(4) {
            lines.push(Line::styled(format!("  {n}"), theme::muted()));
        }
        if names.len() > 4 {
            lines.push(Line::styled(
                format!("  … and {} more", names.len() - 4),
                theme::faint(),
            ));
        }
    } else if let Some(h) = app.hosts.iter().find(|h| h.name == names[0]) {
        lines.push(Line::styled(
            format!("  {}@{}:{}", h.login, h.addr, h.port),
            theme::muted(),
        ));
    }

    lines.push(Line::default());
    lines.push(Line::styled(
        "Saved credentials and keys are removed too.",
        theme::muted(),
    ));
    lines.push(Line::default());
    let row = button_row(
        app,
        inner,
        lines.len() as u16,
        &[
            (
                " Delete ".to_string(),
                theme::danger_btn(),
                Click::Key(ratatui::crossterm::event::KeyCode::Enter),
            ),
            (
                "[ Cancel ]".to_string(),
                theme::body(),
                Click::Key(ratatui::crossterm::event::KeyCode::Esc),
            ),
        ],
    );
    lines.push(row);
    lines.push(Line::styled("Y delete   N cancel", theme::faint()).right_aligned());

    f.render_widget(Paragraph::new(lines), inner);
}

pub fn show_key(f: &mut Frame, app: &mut App) {
    let Some((host, public)) = app.key_dialog.clone() else {
        return;
    };
    let area = f.area();
    let width = 86u16.min(area.width);
    let inner_w = width.saturating_sub(6) as usize;
    let chunks: Vec<String> = public
        .chars()
        .collect::<Vec<_>>()
        .chunks(inner_w.max(1))
        .map(|c| c.iter().collect())
        .collect();

    let rect = centered(area, width, (11 + chunks.len()) as u16);
    f.render_widget(Clear, rect);
    let block = modal_block("SSH Public Key");
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Public key for ", theme::body()),
            Span::styled(host, theme::proxied().add_modifier(Modifier::BOLD)),
            Span::styled("  ·  ed25519", theme::muted()),
        ]),
        Line::default(),
        Line::from(vec![
            Span::styled("Add this line to ", theme::muted()),
            Span::styled("~/.ssh/authorized_keys", Style::new().fg(theme::BLUE)),
            Span::styled(" on the remote host:", theme::muted()),
        ]),
        Line::default(),
    ];
    for c in &chunks {
        lines.push(Line::styled(c.clone(), theme::ok().bg(theme::BG_INSET)));
    }
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("Private key stored at ", theme::muted()),
        Span::styled(
            crate::keys::keys_dir(&app.datadir).display().to_string(),
            theme::faint(),
        ),
    ]));
    lines.push(Line::default());
    let row = button_row(
        app,
        inner,
        lines.len() as u16,
        &[(
            "[ Close ]".to_string(),
            theme::body(),
            Click::Key(ratatui::crossterm::event::KeyCode::Esc),
        )],
    );
    lines.push(row);

    f.render_widget(Paragraph::new(lines), inner);
}

pub fn help(f: &mut Frame, app: &App) {
    let area = f.area();
    let rect = centered(area, 66, 26);
    f.render_widget(Clear, rect);
    let block = modal_block("Help · Key Bindings");
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let k = |key: &str, desc: &str| {
        Line::from(vec![
            Span::styled(
                crate::ui::widgets::pad(key, 14),
                Style::new().fg(theme::ORANGE_BRIGHT),
            ),
            Span::styled(desc.to_string(), theme::body()),
        ])
    };
    let head = |t: &str| Line::styled(t.to_string(), theme::col_header());

    let mut lines = vec![
        head("SCREENS"),
        k("Alt+1/2/3", "Hosts · Shells · Chat"),
        k("Alt+← →", "previous · next screen"),
        k("F9", "cycle screen"),
        Line::default(),
        head("HOSTS"),
        k("↑ ↓", "move cursor"),
        k("Insert", "mark / unmark host (multi-select)"),
        k("*", "invert marks    Ctrl+A select all"),
        k("F2 / F3", "add · edit"),
        k("F4", "mount / unmount"),
        k("F5", "open shell (marked hosts → group tab)"),
        k("F6", "use as proxy"),
        k("F7", "generate SSH key"),
        k("F8", "delete"),
        Line::default(),
        head("SHELLS"),
    ];
    // The one place the app deliberately gives up the keyboard, so it says why.
    lines.push(Line::styled(
        "a focused pane takes every key — F1-F10, Tab, Esc,",
        theme::faint(),
    ));
    lines.push(Line::styled(
        "Ctrl and Alt chords — so mc, GNU Screen and vim are",
        theme::faint(),
    ));
    lines.push(Line::styled(
        "unimpaired — bar one chord that gets you out:",
        theme::faint(),
    ));
    lines.push(k("Alt+← →", "previous · next screen"));
    lines.push(Line::default());
    lines.push(k("click", "the 1/2/3 tabs switch screens — the way out"));
    lines.push(k("click", "a shell tab selects it, its × closes it"));
    lines.push(k("quit", "switch to Hosts, then F10"));
    lines.push(Line::styled(
        "With no shell open the F-keys work here again.",
        theme::faint(),
    ));
    lines.push(Line::default());
    lines.push(head("MOUSE"));
    lines.push(k("hover", "row highlights, hint in the status bar"));
    lines.push(k("click", "move cursor   double-click opens a shell"));

    let _ = app;
    let h = inner.height as usize;
    if lines.len() > h {
        lines.truncate(h);
    }
    f.render_widget(Paragraph::new(lines), inner);
}

pub fn alert(f: &mut Frame, msg: &str) {
    let area = f.area();
    let text = wrap(msg, 52);
    let rect = centered(area, 58, (text.len() + 6) as u16);
    f.render_widget(Clear, rect);
    let block = modal_block("Error");
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut lines: Vec<Line> = text
        .into_iter()
        .map(|l| Line::styled(l, theme::err()))
        .collect();
    lines.push(Line::default());
    lines.push(Line::styled("any key to dismiss", theme::faint()).right_aligned());
    f.render_widget(Paragraph::new(lines), inner);
}

/// Used by the pre-loop unlock/create screens, which run before `App` exists.
pub fn password_prompt(
    f: &mut Frame,
    title: &str,
    fields: &[(&str, &Input)],
    focus: usize,
    error: Option<&str>,
    hint: &str,
) {
    let area = f.area();
    // Six rows of chrome — frame, padding, a blank and the hint — plus three
    // for every boxed field, and two more when there is an error to show.
    let height = 6 + 3 * fields.len() as u16 + u16::from(error.is_some()) * 2;
    // Wide enough for the whole key hint: a reminder clipped mid-word is worse
    // than no reminder, and this is the one screen with no other way out.
    let rect = centered(area, 72, height);
    f.render_widget(Clear, rect);
    let block = modal_block(title);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height == 0 || inner.width < 12 {
        return;
    }

    let bottom = inner.y + inner.height;
    let mut y = inner.y;
    let line = |f: &mut Frame, y: u16, l: Line<'static>| {
        f.render_widget(Paragraph::new(l), Rect::new(inner.x, y, inner.width, 1));
    };
    if let Some(e) = error {
        line(f, y, Line::styled(e.to_string(), theme::err()));
        y += 2;
    }
    for (i, (label, value)) in fields.iter().enumerate() {
        if y + 3 > bottom {
            return;
        }
        input_box(
            f,
            Rect::new(inner.x, y, inner.width, 3),
            label,
            value,
            i == focus,
            "",
        );
        y += 3;
    }
    y += 1;
    if y < bottom {
        line(f, y, Line::styled(hint.to_string(), theme::faint()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn row(buf: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>()
    }

    /// A box's bottom border carries nothing.
    ///
    /// Notes used to hang there, right-aligned, and they read as a break in
    /// the frame rather than a caption: the line stopped, grey text ran across
    /// the gap, and the corner never arrived. Only the label goes in a border,
    /// and only in the top one.
    #[test]
    fn an_input_puts_nothing_in_its_bottom_border() {
        let mut term = Terminal::new(TestBackend::new(30, 3)).unwrap();
        term.draw(|f| {
            input_box(
                f,
                Rect::new(0, 0, 30, 3),
                "Mount point · auto",
                &Input::new("/net/web-01".to_string()),
                true,
                "",
            )
        })
        .unwrap();
        let buf = term.backend().buffer().clone();
        assert_eq!(row(&buf, 0).trim_end(), "┌ Mount point · auto ────────┐");
        assert_eq!(row(&buf, 2).trim_end(), "└────────────────────────────┘");
    }

    /// The cycler wears the same frame, so it cannot drift back either.
    #[test]
    fn the_cycler_puts_nothing_in_its_bottom_border() {
        let mut term = Terminal::new(TestBackend::new(20, 3)).unwrap();
        term.draw(|f| cycle_box(f, Rect::new(0, 0, 20, 3), "Type", "SSH", false))
            .unwrap();
        let buf = term.backend().buffer().clone();
        assert_eq!(row(&buf, 2).trim_end(), "└──────────────────┘");
    }
}
