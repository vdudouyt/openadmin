//! Modals (`design/ui_kits/openadmin/HostDialogs.jsx`).
//!
//! The toolkit is cfdns's: `Clear` + a bordered `Block` + `centered(w, h)` +
//! a `Vec<Line>` in one `Paragraph`, with `button_row` registering its own
//! hitboxes (`/root/cfdns/src/ui/dialogs.rs:16-51`).

use crate::agent::artifacts;
use crate::agent::plan::StepKind;
use crate::app::approve::Row;
use crate::app::bulk::{self, BulkStep};
use crate::app::form::FormField;
use crate::app::{App, Click};
use crate::ui::bash;
use crate::ui::theme;
use crate::ui::widgets::{centered, pad, sanitize, wrap};
use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::symbols::scrollbar;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Clear, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
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

/// Right-aligned buttons that register their own hitboxes, replicating how a
/// `Paragraph` draws a right-aligned line so clicks land where they look.
///
/// Including when the buttons are wider than the row. A `Paragraph` keeps an
/// overlong line's start and cuts its end, whatever its alignment, so the row
/// is drawn from `inner.x` and the last buttons lose their right edge or
/// vanish. Each hitbox is clipped to what is drawn, and a button with nothing
/// on screen gets none; unclipped, a cut button's hitbox ran on past its text
/// over the dialog's frame.
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
    let right = inner.right();
    let mut x = inner.x + inner.width.saturating_sub(content);
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (label, style, click)) in btns.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
            x = x.saturating_add(SEP);
        }
        let w = widths[i];
        let end = x.saturating_add(w).min(right);
        let mut style = *style;
        if end > x {
            let rect = Rect::new(x, y, end - x, 1);
            app.regions.clicks.push((rect, *click));
            if app.is_hovered(rect) {
                style = style.add_modifier(Modifier::REVERSED);
            }
        }
        spans.push(Span::styled(label.clone(), style));
        x = x.saturating_add(w);
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
        app.regions.clicks.push((
            Rect::new(tail.x, tail.y, tail.width, 1),
            Click::FormButton(FormField::KeyButton),
        ));
        // Keyboard focus reverses a button, as the pointer over it does.
        let focus = |b: FormField, style: Style| {
            if f_is(b) {
                style.add_modifier(Modifier::REVERSED)
            } else {
                style
            }
        };
        let mut spans = vec![Span::styled("SSH key", theme::muted()), Span::raw("   ")];
        // The only place a host's key is shown or made, so the key that does it
        // is named beside the button rather than left to the function bar.
        if form.has_key() {
            spans.push(Span::styled("✓ ", theme::ok()));
            spans.push(Span::styled("key installed", theme::body()));
            spans.push(Span::raw("    "));
            spans.push(Span::styled(
                "[ Show public key ]",
                focus(
                    FormField::KeyButton,
                    theme::bright().fg(theme::ORANGE_BRIGHT),
                ),
            ));
            spans.push(Span::raw("  "));
            spans.push(Span::styled("F7", theme::faint()));
        } else {
            spans.push(Span::styled("none", theme::faint()));
            spans.push(Span::raw("    "));
            spans.push(Span::styled(
                "[ Generate SSH key ]",
                focus(FormField::KeyButton, Style::new().fg(theme::ORANGE_BRIGHT)),
            ));
            spans.push(Span::raw("  "));
            spans.push(Span::styled("F7 · prints the public key", theme::faint()));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::default());
    // What the keys do from here: on a button ↵ presses it rather than saving.
    let hint = if form.focus.is_button() {
        "Tab next   ←/→ choose   Enter or Space press   Esc cancel"
    } else {
        "Tab next   ←/→ change type   Enter save   Esc cancel"
    };
    lines.push(Line::styled(hint, theme::faint()));

    let save_label = if form.is_edit() {
        " Save "
    } else {
        " Add Host "
    };
    // Bulk add in Add only; `App::open_bulk` says why. Left of the others, as
    // bulk review's Back is.
    let focused = |b: FormField, style: Style| {
        if form.focus == b {
            style.add_modifier(Modifier::REVERSED)
        } else {
            style
        }
    };
    let mut buttons = Vec::new();
    if !form.is_edit() {
        buttons.push((
            "[ Bulk add ]".to_string(),
            focused(FormField::BulkAdd, theme::body()),
            Click::FormButton(FormField::BulkAdd),
        ));
    }
    buttons.push((
        save_label.to_string(),
        focused(FormField::Save, theme::primary_btn()),
        Click::FormButton(FormField::Save),
    ));
    buttons.push((
        "[ Cancel ]".to_string(),
        focused(FormField::Cancel, theme::body()),
        Click::FormButton(FormField::Cancel),
    ));
    lines.push(button_row(app, tail, lines.len() as u16, &buttons));

    f.render_widget(Paragraph::new(lines), tail);
}

/// The plan confirmation dialog: one row per step, one per host under it, each
/// with its own checkbox.
///
/// The script body is rendered in full and wrapped, never elided. A plan the
/// operator cannot read end to end is one they cannot judge, and that would
/// turn the whole confirmation into theatre.
///
/// It covers the transcript exactly, composer excluded: a centred box capped
/// at 78×34 left most of a large terminal to the transcript it was hiding
/// anyway, and made a long script a long scroll through a small window.
pub fn confirm_plan(f: &mut Frame, app: &mut App) {
    let Some(sel) = app.plan.clone() else { return };
    let rect = app.regions.chat_log.unwrap_or(f.area());
    f.render_widget(Clear, rect);
    // Double-line frame: this is the destructive confirmation.
    let block = danger_block("Confirm Plan");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 6 {
        return;
    }

    let (steps_on, hosts_on) = sel.counts();
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
        match *row {
            Row::Step(i) => {
                let step = &sel.plan.steps[i];
                let on = sel.step_on[i];
                body.push((
                    Line::from(vec![
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
                        // Every character bash reads, coloured by what bash
                        // makes of it — see `bash` for why that is held to
                        // never making live code look inert.
                        for row in bash::script_rows(script, body_w) {
                            // A continuation is marked, so one wrapped command
                            // cannot read as two.
                            let gutter = if row.continuation {
                                "    ┆ "
                            } else {
                                "    │ "
                            };
                            let mut spans = vec![Span::styled(gutter, theme::border_idle())];
                            spans.extend(row.spans);
                            body.push((Line::from(spans), None));
                        }
                    }
                    StepKind::Upload { artifact } => {
                        // The whole destination, not just its directory: a staged
                        // path is kept on the far side, so `nginx/site.conf`
                        // lands in a `nginx/` of its own and the operator should
                        // see where the file actually goes before approving it.
                        //
                        // From the executor's own constant, never a literal here:
                        // the operator is approving a path, so the path they read
                        // has to be the path the file lands at. The name is
                        // trimmed but not canonicalized — `staged()` stats the
                        // filesystem, and this runs on every frame — so a staged
                        // symlink displays as the link rather than its target.
                        let name = artifact.trim();
                        let (_, dest) = artifacts::upload_target(name);
                        body.push((
                            Line::from(vec![
                                Span::styled("    │ ", theme::border_idle()),
                                Span::styled(sanitize(name), theme::muted()),
                                Span::styled(format!(" → {}", sanitize(&dest)), theme::faint()),
                            ]),
                            None,
                        ));
                    }
                }
            }
            Row::Host(i, j) => {
                let id = sel.plan.steps[i].hosts[j];
                let host = app.hosts.iter().find(|h| h.id == id);
                let name = host
                    .map(|h| h.name.clone())
                    .unwrap_or_else(|| format!("#{id} (gone)"));
                // Proposed before the filter changed: said so, not unchecked —
                // whether it runs is still the operator's call.
                let hidden = host.is_some_and(|h| !app.is_visible(h));
                let on = sel.host_on[i][j];
                let active = sel.host_active(i, j);
                body.push((
                    Line::from(vec![
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
                        Span::styled(
                            if hidden { " · hidden by filter" } else { "" },
                            theme::faint(),
                        ),
                    ]),
                    Some(r),
                ));
            }
        }
    }

    // A plan the operator cannot read end to end is one they cannot judge, so
    // the body scrolls rather than truncating.
    //
    // The view and the cursor each lead in turn. A cursor that moved pulls the
    // window onto its row. A window that was scrolled keeps its place and
    // brings the cursor along to the nearest row in sight, so ▸ and Space stay
    // on something visible and ↑/↓ carry on from where the operator is
    // reading. Inside a script longer than the window no row is in sight, and
    // the cursor waits where it was.
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
    let mut cursor = sel.cursor;
    if sel.follow {
        if cursor_line < scroll {
            scroll = cursor_line;
        } else if cursor_line >= scroll + view {
            scroll = cursor_line + 1 - view;
        }
    } else if !(scroll..scroll + view).contains(&cursor_line) {
        let mut in_sight = body.iter().skip(scroll).take(view).filter_map(|(_, r)| *r);
        let nearest = if cursor_line < scroll {
            in_sight.next()
        } else {
            in_sight.next_back()
        };
        if let Some(r) = nearest {
            cursor = r;
        }
    }
    if let Some(s) = app.plan.as_mut() {
        s.scroll = scroll;
        s.cursor = cursor;
        s.max_scroll = max_scroll;
        s.page = view.saturating_sub(1).max(1);
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
        // The marker goes on here rather than when the body was built, because
        // only now is it settled which row the cursor is on.
        let mut line = line.clone();
        if let Some(r) = row {
            line.spans.insert(
                0,
                Span::styled(if *r == cursor { "▸" } else { " " }, theme::proxied()),
            );
        }
        lines.push(line);
    }
    let hidden = body.len() - scroll - shown;

    // Where the window is, on the frame itself: the right border over the body
    // band becomes the track. `DOUBLE_VERTICAL`'s track is the frame's own `║`,
    // so only the thumb stands out.
    if max_scroll > 0 {
        let band = Rect::new(
            rect.x + rect.width.saturating_sub(1),
            inner.y + header as u16,
            1,
            view as u16,
        );
        // One more position than the last scroll offset, so that at the end
        // the thumb sits flush against the bottom of the band.
        let mut state = ScrollbarState::new(max_scroll + 1)
            .position(scroll)
            .viewport_content_length(view);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .symbols(scrollbar::DOUBLE_VERTICAL)
                .begin_symbol(None)
                .end_symbol(None)
                .track_style(Style::new().fg(theme::ORANGE_DIM))
                .thumb_style(Style::new().fg(theme::ORANGE)),
            band,
            &mut state,
        );
    }

    lines.push(Line::default());
    // Kept short enough to fit the dialog: a clipped hint helps nobody.
    let mut hint = "↑↓ move · Space toggle · a/x · ↵ run · F2 hide · Esc reject".to_string();
    // Named with the key that reaches it: ↑/↓ walk the rows, and would step
    // over every line of a script. No word after the count, so the hint still
    // fits an 80-column terminal.
    if hidden > 0 {
        hint.push_str(&format!(" · PgDn ↓{hidden}"));
    } else if scroll > 0 {
        hint.push_str(&format!(" · PgUp ↑{scroll}"));
    }
    lines.push(Line::styled(hint, theme::faint()));

    // A dead button registers no hitbox, which is the cleanest way to make it
    // dead — there is nothing to click.
    let run_label = if hosts_on == 0 {
        " Nothing selected ".to_string()
    } else {
        format!(" Run {steps_on} step(s) on {hosts_on} host(s) ")
    };
    let buttons: Vec<(String, ratatui::style::Style, Click)> = if hosts_on == 0 {
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
    if hosts_on == 0 {
        lines.push(Line::styled(run_label, theme::faint()).right_aligned());
    }
    let row = button_row(app, inner, lines.len() as u16, &buttons);
    lines.push(row);

    f.render_widget(Paragraph::new(lines), inner);
}

/// sshfs connecting, with a way out.
///
/// The bar has one segment per host: mounted ones solid, the one in flight with
/// a block sweeping across it, pending ones empty. sshfs says nothing while it
/// connects, so there is no fraction to draw for the host in flight — the sweep
/// is there so the dialog visibly has not hung, which on a host that is not
/// answering is the whole question.
pub fn mounting(f: &mut Frame, app: &mut App) {
    let Some(job) = app.mounting.as_ref() else {
        return;
    };
    let progress = job.progress();
    let total = job.hosts.len();
    let current = progress.current.min(total.saturating_sub(1));
    let (name, point) = job.hosts.get(current).cloned().unwrap_or_default();
    let cancelling = job.is_cancelling();
    // Sweep by elapsed time rather than by the spinner's frame counter, which
    // wraps after a handful of frames and would move the block in jumps.
    let tick = (job.started.elapsed().as_millis() / 60) as usize;
    let spinner = app.spinner_char();

    // Seven lines, and `modal_block` pads a row above and below inside the
    // border: 7 + 2 + 2. A first cut at 10 clipped the Cancel button while still
    // registering its hitbox, so the height is pinned by a test that finds the
    // last line on screen.
    let area = f.area();
    let rect = centered(area, 58, 11);
    f.render_widget(Clear, rect);
    let block = modal_block("Mounting");
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let bar_w = inner.width as usize;
    let bar: Vec<Span> = crate::app::mount_job::bar(bar_w, progress.done, total, tick)
        .into_iter()
        .map(|lit| {
            if lit {
                Span::styled("█", Style::new().fg(theme::ORANGE))
            } else {
                Span::styled("░", theme::faint())
            }
        })
        .collect();

    let what = if cancelling {
        "cancelling…".to_string()
    } else if total > 1 {
        format!("host {} of {total} · connecting", current + 1)
    } else {
        "connecting".to_string()
    };

    // No indent of its own: `modal_block` already pads, as every dialog relies on.
    let mut lines = vec![
        Line::from(vec![
            Span::styled(name, theme::bright().add_modifier(Modifier::BOLD)),
            Span::styled(" → ", theme::faint()),
            Span::styled(point, theme::muted()),
        ]),
        Line::default(),
        Line::from(bar),
        Line::from(vec![
            Span::styled(format!("{spinner} "), theme::proxied()),
            Span::styled(what, theme::muted()),
        ]),
        Line::default(),
    ];
    let row = button_row(
        app,
        inner,
        lines.len() as u16,
        &[(
            " Cancel ".to_string(),
            theme::primary_btn(),
            Click::Key(ratatui::crossterm::event::KeyCode::Esc),
        )],
    );
    lines.push(row);
    lines.push(Line::styled("Esc cancel", theme::faint()).right_aligned());

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
    } else if let Some(h) = app
        .pending_delete
        .first()
        .and_then(|id| app.hosts.iter().find(|h| h.id == *id))
    {
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

/// Ctrl+F on Hosts: the filter, typed. Shows as it is typed how many hosts it
/// would list, so a needle that matches nothing is seen before it is applied.
pub fn filter_hosts(f: &mut Frame, app: &mut App) {
    let Some(input) = app.filter_edit.clone() else {
        return;
    };
    let rect = centered(f.area(), 60, 11);
    f.render_widget(Clear, rect);
    let block = modal_block("Filter Hosts");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 7 || inner.width < 20 {
        return;
    }

    f.render_widget(
        Paragraph::new(Line::styled(
            "Show hosts whose name or address contains:",
            theme::muted(),
        )),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let field = Rect::new(inner.x, inner.y + 1, inner.width, 3);
    input_box(f, field, "Name or address", &input, true, "");

    let needle = input.value().trim().to_lowercase();
    let all = app.hosts.len();
    let count = if needle.is_empty() {
        format!("All {all} hosts")
    } else {
        let n = app
            .hosts
            .iter()
            .filter(|h| {
                h.name.to_lowercase().contains(&needle) || h.addr.to_lowercase().contains(&needle)
            })
            .count();
        format!("{n} of {all} match")
    };
    let tail = Rect::new(inner.x, inner.y + 4, inner.width, inner.height - 4);
    let mut lines = vec![
        Line::styled(count, theme::faint()),
        Line::styled("↵ filter · empty shows all · Esc cancel", theme::faint()),
    ];
    let row = button_row(
        app,
        tail,
        lines.len() as u16,
        &[
            (
                " Filter ".to_string(),
                theme::primary_btn(),
                Click::Key(KeyCode::Enter),
            ),
            (
                "[ Cancel ]".to_string(),
                theme::body(),
                Click::Key(KeyCode::Esc),
            ),
        ],
    );
    lines.push(row);
    f.render_widget(Paragraph::new(lines), tail);
}

/// Bulk import: paste a qhostman list, then review what it would add.
///
/// Both steps derive their rows from the text on every frame, against the
/// hosts as they are now; nothing is stored between them but the text.
pub fn bulk_import(f: &mut Frame, app: &mut App) {
    let Some(b) = app.bulk.as_ref() else { return };
    let step = b.step;
    let rows = bulk::review(&b.joined(), &app.hosts, &app.cfg.mount_prefix);
    match step {
        BulkStep::Paste => bulk_paste(f, app, &rows),
        BulkStep::Review => bulk_review(f, app, &rows),
    }
}

/// What the paste amounts to, in one line.
fn bulk_summary(c: &bulk::Counts) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{} to add", c.new),
        if c.new > 0 {
            theme::ok()
        } else {
            theme::muted()
        },
    )];
    if c.skipped > 0 {
        spans.push(Span::styled(
            format!(" · {} already here, skipped", c.skipped),
            theme::muted(),
        ));
    }
    if c.problems > 0 {
        let at = c
            .first_problem
            .map(|l| format!(", first at line {l}"))
            .unwrap_or_default();
        spans.push(Span::styled(
            format!(" · {}{at}", bulk::count(c.problems, "problem")),
            theme::err(),
        ));
    }
    Line::from(spans)
}

fn bulk_paste(f: &mut Frame, app: &mut App, rows: &[bulk::Block]) {
    let area = f.area();
    let rect = centered(area, 76, 28);
    f.render_widget(Clear, rect);
    let block = modal_block("Bulk Import · 1 of 2 — Paste");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    // The format and a blank above the box; the summary, a blank, the hint and
    // the buttons below it. The box gets the rest, and needs three rows to be
    // a box at all.
    const HEAD: u16 = 3;
    const FOOT: u16 = 4;
    if inner.height < HEAD + 3 + FOOT || inner.width < 24 {
        return;
    }

    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Four lines per host: ", theme::muted()),
                Span::styled(bulk::FIELDS, theme::body()),
                Span::styled(".", theme::muted()),
            ]),
            Line::styled(
                "A blank line between hosts. Names already here are skipped.",
                theme::muted(),
            ),
        ]),
        Rect::new(inner.x, inner.y, inner.width, HEAD),
    );

    let boxed = Rect::new(
        inner.x,
        inner.y + HEAD,
        inner.width,
        inner.height - HEAD - FOOT,
    );
    if let Some(b) = app.bulk.as_mut() {
        let t = &mut b.text;
        t.set_block(input_block("Hosts", true));
        t.set_style(theme::body());
        t.set_cursor_style(Style::new().bg(theme::ORANGE_BRIGHT).fg(theme::ORANGE_INK));
        t.set_cursor_line_style(Style::default());
        // Numbered, because a problem is reported by the line it starts on.
        t.set_line_number_style(theme::faint());
        t.set_styled_placeholder(
            ["web-01", "10.0.4.11", "root", "password", "", "web-02", "…"]
                .into_iter()
                .map(|l| Line::styled(l, theme::faint()))
                .collect::<Vec<_>>(),
        );
        f.render_widget(&*t, boxed);
    }

    let foot = Rect::new(inner.x, boxed.y + boxed.height, inner.width, FOOT);
    let mut lines = vec![
        if rows.is_empty() {
            Line::styled("Nothing pasted yet.", theme::faint())
        } else {
            bulk_summary(&bulk::counts(rows))
        },
        Line::default(),
        Line::styled("Tab review · ↵ new line · Esc cancel", theme::faint()),
    ];
    let row = button_row(
        app,
        foot,
        lines.len() as u16,
        &[
            (
                " Review ▸ ".to_string(),
                theme::primary_btn(),
                Click::Key(KeyCode::Tab),
            ),
            (
                "[ Cancel ]".to_string(),
                theme::body(),
                Click::Key(KeyCode::Esc),
            ),
        ],
    );
    lines.push(row);
    f.render_widget(Paragraph::new(lines), foot);
}

fn bulk_review(f: &mut Frame, app: &mut App, rows: &[bulk::Block]) {
    let counts = bulk::counts(rows);
    let blocked = counts.problems > 0 || counts.new == 0;
    // The summary and a blank; a blank, the hint and the buttons — and, when
    // there is nothing it may do, a line saying why the Import button is gone.
    let header = 2usize;
    let footer = 3 + usize::from(blocked);
    let want = 4 + header + rows.len().max(1) + footer;

    let area = f.area();
    let rect = centered(area, 76, want.min(28) as u16);
    f.render_widget(Clear, rect);
    let block = modal_block("Bulk Import · 2 of 2 — Review");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 4 {
        return;
    }

    let mut lines: Vec<Line> = vec![
        if rows.is_empty() {
            Line::styled("Nothing pasted.", theme::muted())
        } else {
            bulk_summary(&counts)
        },
        Line::default(),
    ];

    // Pasted text is shown, so it is sanitized: it came off a clipboard, and a
    // control sequence in it must not drive this terminal.
    let body: Vec<Line> = rows
        .iter()
        .map(|(line, row)| match row {
            bulk::Row::New(r) => Line::from(vec![
                Span::styled("+ ", theme::ok()),
                Span::styled(pad(&sanitize(&r.name), 24), theme::bright()),
                Span::styled(
                    pad(&sanitize(&format!("{}@{}", r.login, r.addr)), 30),
                    theme::body(),
                ),
                Span::styled(
                    crate::ui::screens::hosts::mask_pass(&r.pass),
                    theme::faint(),
                ),
            ]),
            bulk::Row::Exists(name) => Line::from(vec![
                Span::styled("= ", theme::faint()),
                Span::styled(pad(&sanitize(name), 24), theme::muted()),
                Span::styled("already here — skipped", theme::faint()),
            ]),
            bulk::Row::Repeated { name, first_line } => Line::from(vec![
                Span::styled("= ", theme::faint()),
                Span::styled(pad(&sanitize(name), 24), theme::muted()),
                Span::styled(
                    format!("repeated — line {first_line} adds it"),
                    theme::faint(),
                ),
            ]),
            bulk::Row::Invalid(msg) => Line::from(vec![
                Span::styled("✕ ", theme::err()),
                Span::styled(pad(&format!("line {line}"), 10), theme::err()),
                Span::styled(msg.clone(), theme::body()),
            ]),
        })
        .collect();

    let view = (inner.height as usize)
        .saturating_sub(header + footer)
        .max(1);
    let max_scroll = body.len().saturating_sub(view);
    let scroll = app.bulk.as_ref().map_or(0, |b| b.scroll).min(max_scroll);
    if let Some(b) = app.bulk.as_mut() {
        b.scroll = scroll;
        b.max_scroll = max_scroll;
        b.page = view.saturating_sub(1).max(1);
    }
    let shown = body.len().saturating_sub(scroll).min(view);
    lines.extend(body.iter().skip(scroll).take(view).cloned());
    let hidden = body.len() - scroll - shown;

    if max_scroll > 0 {
        let band = Rect::new(
            rect.x + rect.width.saturating_sub(1),
            inner.y + header as u16,
            1,
            view as u16,
        );
        let mut state = ScrollbarState::new(max_scroll + 1)
            .position(scroll)
            .viewport_content_length(view);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .symbols(scrollbar::VERTICAL)
                .begin_symbol(None)
                .end_symbol(None)
                .track_style(theme::border_focused())
                .thumb_style(Style::new().fg(theme::ORANGE)),
            band,
            &mut state,
        );
    }

    lines.push(Line::default());
    let mut hint = "↑↓ scroll · ↵ import · ⇧Tab back · Esc cancel".to_string();
    if hidden > 0 {
        hint.push_str(&format!(" · PgDn ↓{hidden}"));
    } else if scroll > 0 {
        hint.push_str(&format!(" · PgUp ↑{scroll}"));
    }
    lines.push(Line::styled(hint, theme::faint()));

    let back = (
        "[ ◂ Back ]".to_string(),
        theme::body(),
        Click::Key(KeyCode::BackTab),
    );
    let cancel = (
        "[ Cancel ]".to_string(),
        theme::body(),
        Click::Key(KeyCode::Esc),
    );
    // A dead button registers no hitbox, as in the plan dialog: there is
    // nothing to click, and the line says why.
    let buttons = if blocked {
        let why = if counts.problems > 0 {
            format!(
                " Fix {} before importing ",
                bulk::count(counts.problems, "problem")
            )
        } else {
            " Nothing to import ".to_string()
        };
        lines.push(Line::styled(why, theme::faint()).right_aligned());
        vec![back, cancel]
    } else {
        vec![
            back,
            (
                format!(" Import {} ", bulk::count(counts.new, "host")),
                theme::primary_btn(),
                Click::Key(KeyCode::Enter),
            ),
            cancel,
        ]
    };
    let row = button_row(app, inner, lines.len() as u16, &buttons);
    lines.push(row);
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
        head("TABS"),
        k("Alt+1 · 2", "Hosts (OpenAdmin) · Chat"),
        k("Alt+3…9", "the open shells, in the order opened"),
        k("Alt+← →", "previous · next tab"),
        k("F9", "next tab (Chat)"),
        Line::default(),
        head("HOSTS"),
        k("↑ ↓", "move cursor"),
        k("Insert", "mark / unmark host (multi-select)"),
        k("*", "invert marks    Ctrl+A select all"),
        k("Ctrl+F", "filter by name or address — Esc shows all"),
        k("F2 / a", "add — bulk add: F2 in the form"),
        k("F4 / e", "edit — SSH key: F7 in the form"),
        k("↵", "open shell (marked hosts → group tab)"),
        k("F9 · m · u", "mount / unmount · mount · unmount"),
        k("F6", "use as proxy"),
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
        "unimpaired — bar the tab keys that get you out:",
        theme::faint(),
    ));
    lines.push(k("Alt+1…9 ← →", "go to a tab · previous · next"));
    lines.push(Line::default());
    lines.push(k("click", "a header tab goes there, a shell's × closes it"));
    lines.push(k("wheel", "scroll back; typing returns — not in mc, vim"));
    lines.push(k("quit", "switch to Hosts, then F10"));
    lines.push(Line::styled(
        "With no shell open the F-keys work here again.",
        theme::faint(),
    ));
    lines.push(Line::default());
    lines.push(head("MOUSE"));
    lines.push(k("hover", "row highlights, hint in the status bar"));
    lines.push(k("click", "move cursor   double-click opens a shell"));
    lines.push(k("drag", "a scrollbar; click one to jump there"));

    // Sized from what it says: a fixed height had fallen eight lines short,
    // and the MOUSE section was never on screen. Four is the border and the
    // padding. A terminal too short for all of it still truncates.
    let rect = centered(f.area(), 66, lines.len() as u16 + 4);
    f.render_widget(Clear, rect);
    let block = modal_block("Help · Key Bindings");
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let _ = app;
    let h = inner.height as usize;
    if lines.len() > h {
        lines.truncate(h);
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// An error, waiting to be dismissed.
///
/// The message is capped to what the screen can hold with the button still on
/// it. `centered` clamps a frame taller than the terminal, and a clamped frame
/// clips from the bottom — which is where Dismiss is, so an error long enough
/// would have been a dialog with no way out. What does not fit is counted.
pub fn alert(f: &mut Frame, app: &mut App) {
    let Some(msg) = app.alert.clone() else {
        return;
    };
    const W: u16 = 64;
    // Borders and padding, then a blank line, the button and its hint.
    const CHROME: u16 = 2 + 2 + 3;
    let area = f.area();
    let inner_w = W.min(area.width).saturating_sub(2 + 4) as usize;

    // Sanitized line by line, keeping the lines: an error is often text a remote
    // server or sshfs chose, and it must not drive this terminal.
    let clean = msg.lines().map(sanitize).collect::<Vec<_>>().join("\n");
    let mut text: Vec<String> = wrap(&clean, inner_w.saturating_sub(2));
    let room = area.height.saturating_sub(CHROME).max(1) as usize;
    let hidden = text.len().saturating_sub(room);
    if hidden > 0 {
        text.truncate(room.saturating_sub(1));
    }

    let rect = centered(area, W, text.len() as u16 + u16::from(hidden > 0) + CHROME);
    f.render_widget(Clear, rect);
    let block = modal_block("Error");
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut lines: Vec<Line> = text
        .into_iter()
        .enumerate()
        .map(|(i, l)| {
            Line::from(vec![
                Span::styled(if i == 0 { "✕ " } else { "  " }, theme::err()),
                Span::styled(l, theme::body()),
            ])
        })
        .collect();
    if hidden > 0 {
        lines.push(Line::styled(
            format!(
                "  … {hidden} more line{}",
                if hidden == 1 { "" } else { "s" }
            ),
            theme::faint(),
        ));
    }
    lines.push(Line::default());
    let row = button_row(
        app,
        inner,
        lines.len() as u16,
        &[(
            " Dismiss ".to_string(),
            theme::primary_btn(),
            Click::Dismiss,
        )],
    );
    lines.push(row);
    lines.push(Line::styled("Esc / Enter dismiss", theme::faint()).right_aligned());
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
