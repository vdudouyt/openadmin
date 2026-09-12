//! Modals (`design/ui_kits/openadmin/HostDialogs.jsx`).
//!
//! The toolkit is cfdns's: `Clear` + a bordered `Block` + `centered(w, h)` +
//! a `Vec<Line>` in one `Paragraph`, with `button_row` registering its own
//! hitboxes (`/root/cfdns/src/ui/dialogs.rs:16-51`).

use crate::app::form::FormField;
use crate::app::{App, Click};
use crate::ui::theme;
use crate::ui::widgets::{centered, padl, wrap};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph};

const LABEL_W: usize = 14;

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

/// `label  [ value        ]` with the inset well and a cursor when focused.
fn field_line(
    label: &str,
    value: &str,
    focused: bool,
    width: usize,
    placeholder: &str,
) -> Line<'static> {
    let label_style = if focused {
        theme::proxied()
    } else {
        theme::muted()
    };
    let mut spans = vec![
        Span::styled(padl(label, LABEL_W), label_style),
        Span::raw("  "),
        Span::styled(" ", Style::new().bg(theme::BG_INSET)),
    ];
    let shown_len = value.chars().count();
    if value.is_empty() && !focused {
        spans.push(Span::styled(
            placeholder.to_string(),
            theme::faint().bg(theme::BG_INSET),
        ));
    } else {
        spans.push(Span::styled(
            value.to_string(),
            theme::body().bg(theme::BG_INSET),
        ));
    }
    let used =
        1 + if value.is_empty() && !focused {
            placeholder.chars().count()
        } else {
            shown_len
        } + usize::from(focused);
    if focused {
        spans.push(Span::styled("█", theme::proxied().bg(theme::BG_INSET)));
    }
    spans.push(Span::styled(
        " ".repeat(width.saturating_sub(used)),
        Style::new().bg(theme::BG_INSET),
    ));
    Line::from(spans)
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
    if inner.height < 6 {
        return;
    }

    let auto_mount = app.auto_mount_for(&form.name);
    let mount_shown = if form.mount_auto {
        auto_mount
    } else {
        form.mount.clone()
    };
    let mut lines: Vec<Line> = Vec::new();
    let f_is = |x: FormField| form.focus == x;

    // Register each field row as a focus target as it is laid out.
    let push_field = |app: &mut App,
                      lines: &mut Vec<Line>,
                      field: FormField,
                      label: &str,
                      value: &str,
                      width: usize,
                      placeholder: &str| {
        let y = inner.y + lines.len() as u16;
        app.regions.clicks.push((
            Rect::new(inner.x, y, inner.width, 1),
            Click::FocusField(field),
        ));
        lines.push(field_line(label, value, f_is(field), width, placeholder));
    };

    push_field(
        app,
        &mut lines,
        FormField::Name,
        "Host name",
        &form.name,
        34,
        "nickname, e.g. web-01",
    );
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(LABEL_W + 2)),
        Span::styled("a human label — the mount point follows it", theme::faint()),
    ]));

    // Type is a cycler, not a text field.
    {
        let y = inner.y + lines.len() as u16;
        app.regions
            .clicks
            .push((Rect::new(inner.x, y, inner.width, 1), Click::CycleType(1)));
        let focused = f_is(FormField::Type);
        let label_style = if focused {
            theme::proxied()
        } else {
            theme::muted()
        };
        let arrows = if focused {
            ("◂ ", " ▸")
        } else {
            ("  ", " ▾")
        };
        lines.push(Line::from(vec![
            Span::styled(padl("Type", LABEL_W), label_style),
            Span::raw("  "),
            Span::styled(
                format!(" {}{}{} ", arrows.0, form.proto, arrows.1),
                theme::body().bg(theme::BG_INSET),
            ),
        ]));
    }

    push_field(
        app,
        &mut lines,
        FormField::Addr,
        "Address",
        &form.addr,
        34,
        "host or IP",
    );
    push_field(
        app,
        &mut lines,
        FormField::Port,
        "Port",
        &form.port,
        8,
        "22",
    );
    push_field(
        app,
        &mut lines,
        FormField::Mount,
        "Mount point",
        &mount_shown,
        34,
        "/net/<name>",
    );
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(LABEL_W + 2)),
        if form.mount_auto {
            Span::styled(
                "auto from host name — type here to override",
                theme::faint(),
            )
        } else {
            Span::styled("overridden · clear to restore auto", theme::warn())
        },
    ]));
    push_field(
        app,
        &mut lines,
        FormField::Login,
        "Login",
        &form.login,
        24,
        "user",
    );
    let masked = "•".repeat(form.pass.chars().count());
    push_field(
        app,
        &mut lines,
        FormField::Pass,
        "Password",
        &masked,
        24,
        "optional with a key",
    );

    lines.push(Line::default());

    // SSH key row.
    {
        let y = inner.y + lines.len() as u16;
        app.regions
            .clicks
            .push((Rect::new(inner.x, y, inner.width, 1), Click::GenKey));
        let mut spans = vec![
            Span::styled(padl("SSH key", LABEL_W), theme::muted()),
            Span::raw("  "),
        ];
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
    let row = button_row(
        app,
        inner,
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
    );
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
    lines.push(Line::styled("completely unimpaired.", theme::faint()));
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
    fields: &[(&str, &str)],
    focus: usize,
    error: Option<&str>,
    hint: &str,
) {
    let area = f.area();
    let rect = centered(
        area,
        62,
        (9 + fields.len() as u16) + u16::from(error.is_some()) * 2,
    );
    f.render_widget(Clear, rect);
    let block = modal_block(title);
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(e) = error {
        lines.push(Line::styled(e.to_string(), theme::err()));
        lines.push(Line::default());
    }
    for (i, (label, value)) in fields.iter().enumerate() {
        lines.push(field_line(label, value, i == focus, 30, ""));
    }
    lines.push(Line::default());
    lines.push(Line::styled(hint.to_string(), theme::faint()));
    f.render_widget(Paragraph::new(lines), inner);
}
