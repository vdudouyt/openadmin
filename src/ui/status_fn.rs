//! The 1-row status line and the bottom function-key bar
//! (`design/ui_kits/openadmin/AppChrome.jsx:39-65`).
//!
//! The function bar is per-screen. On the Shells screen the caps read `Esc-N`
//! rather than `FN`, because no F-key is reserved there — every one of them
//! goes to the terminal.

use crate::app::{App, Mode, Screen, StatusKind};
use crate::ui::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

pub fn render_status(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(Block::new().style(theme::statusbar()), area);

    let left_text = match app.screen {
        Screen::Hosts => {
            let mut s = format!(
                "{} hosts · {} mounted",
                app.hosts.len(),
                app.mounted_count()
            );
            if !app.marked.is_empty() {
                s.push_str(&format!(" · {} marked", app.marked.len()));
            }
            s
        }
        Screen::Shells => {
            let n = app.term.tab_count();
            let s = format!("{n} shell{} open", if n == 1 { "" } else { "s" });
            // With no function bar here, this line is the only place that can
            // point at the way out of a focused pane.
            if app.term.focused_session().is_some() {
                format!("{s} · keys go to the terminal · click 1/2/3 above")
            } else {
                s
            }
        }
        Screen::Chat => format!("agent · {}", app.cfg.model),
    };

    let msg_w = (area.width / 3).clamp(20, 48);
    let [left_a, hint_a, right_a] = Layout::horizontal([
        Constraint::Length(left_text.chars().count() as u16 + 3),
        Constraint::Min(0),
        Constraint::Length(msg_w),
    ])
    .areas(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(left_text, theme::statusbar()),
        ]))
        .style(theme::statusbar()),
        left_a,
    );

    // Middle: what the pointer is over — a real use of mouse reporting.
    if let Some(hint) = app.hover_hint() {
        f.render_widget(
            Paragraph::new(Line::styled(
                format!("· {hint}"),
                theme::faint().bg(theme::STATUSBAR_BG),
            ))
            .style(theme::statusbar()),
            hint_a,
        );
    }

    let style = match app.status.kind {
        StatusKind::Ok => theme::ok(),
        StatusKind::Warn => theme::warn(),
        StatusKind::Err => theme::err(),
        StatusKind::Loading => theme::proxied(),
        StatusKind::Idle => theme::statusbar(),
    }
    .bg(theme::STATUSBAR_BG);

    let mut right: Vec<Span> = Vec::new();
    if app.busy {
        right.push(Span::styled(
            format!("{} ", app.spinner_char()),
            theme::proxied().bg(theme::STATUSBAR_BG),
        ));
    } else if app.status.kind == StatusKind::Ok {
        right.push(Span::styled("✓ ", theme::ok().bg(theme::STATUSBAR_BG)));
    }
    right.push(Span::styled(app.status.text.clone(), style));
    right.push(Span::styled(" ", theme::statusbar()));
    f.render_widget(
        Paragraph::new(Line::from(right).right_aligned()).style(theme::statusbar()),
        right_a,
    );
}

/// `(cap, label, function-key number, danger)`.
type Entry = (String, String, u8, bool);

fn entries(app: &App) -> Vec<Entry> {
    let e = |cap: &str, label: &str, n: u8, danger: bool| {
        (cap.to_string(), label.to_string(), n, danger)
    };

    match app.mode {
        Mode::HostForm => {
            let save = if app.form.as_ref().is_some_and(|f| f.is_edit()) {
                "Save"
            } else {
                "Add"
            };
            return vec![
                e("Esc", "Cancel", 0xff, false),
                e("↹", "Next field", 0xff, false),
                e("↵", save, 0xff, false),
            ];
        }
        Mode::ConfirmDelete => {
            return vec![e("N", "Cancel", 0xff, false), e("Y", "Delete", 0xff, true)];
        }
        Mode::ShowKey | Mode::Help => return vec![e("Esc", "Close", 0xff, false)],
        Mode::Normal => {}
    }

    match app.screen {
        Screen::Hosts => {
            let targets = app.targets();
            let mount_label = if targets.iter().any(|t| !t.mounted) {
                "Mount"
            } else {
                "Unmount"
            };
            let nm = app.marked.len();
            vec![
                e("F1", "Help", 1, false),
                e("F2", "Add", 2, false),
                e("F3", "Edit", 3, false),
                e("F4", mount_label, 4, false),
                e(
                    "F5",
                    &if nm > 1 {
                        format!("Shell ×{nm}")
                    } else {
                        "Shell".into()
                    },
                    5,
                    false,
                ),
                e("F6", "Proxy", 6, false),
                e("F7", "GenKey", 7, false),
                e("F8", "Delete", 8, true),
                e(
                    "Ins",
                    &if nm > 0 {
                        format!("Marked {nm}")
                    } else {
                        "Mark".into()
                    },
                    0xff,
                    false,
                ),
                e("F10", "Quit", 10, false),
            ]
        }
        // A focused pane owns the whole keyboard, so these are click-only and
        // the caps say so with a pointer glyph instead of a key name. With no
        // pane focused the normal F-keys work again, and the caps show that.
        Screen::Shells => {
            let live = app.term.focused_session().is_some();
            let cap = |n: u8| {
                if live {
                    "▸".to_string()
                } else {
                    format!("F{n}")
                }
            };
            vec![
                e(&cap(1), "Help", 1, false),
                e(&cap(2), "Pane", 2, false),
                e(&cap(3), "Tab", 3, false),
                e(&cap(4), "Close", 4, false),
                e(&cap(5), "New shell", 5, false),
                e(&cap(9), "Screen", 9, false),
                e(&cap(10), "Quit", 10, false),
            ]
        }
        Screen::Chat => vec![
            e("F1", "Help", 1, false),
            e("↵", "Send", 0xff, false),
            e("^R", "Run command", 0xff, false),
            e("@", "Add context", 0xff, false),
            e("F9", "Screen", 9, false),
            e("F10", "Quit", 10, false),
        ],
    }
}

pub fn render_function_bar(f: &mut Frame, area: Rect, app: &mut App) {
    f.render_widget(Block::new().style(theme::statusbar()), area);

    app.regions.fn_bar_y = area.y;
    let items = entries(app);

    let mut spans: Vec<Span> = Vec::new();
    let mut x = area.x;
    for (cap, label, fkey, danger) in items {
        let cap_s = format!(" {cap} ");
        let lab_s = format!(" {label}");
        let seg_w = (cap_s.chars().count() + lab_s.chars().count()) as u16;
        if x + seg_w > area.x + area.width {
            break;
        }
        let rect = Rect::new(x, area.y, seg_w, 1);
        // 0xff marks a cap with no app action (a hint, not a button).
        if fkey != 0xff {
            app.regions.fkeys.push((x, x + seg_w, fkey));
        }
        let hovered = app.is_hovered(rect) && fkey != 0xff;

        let cap_style = if hovered {
            theme::primary_btn()
        } else {
            theme::keycap()
        };
        spans.push(Span::styled(cap_s, cap_style));
        let label_style = if danger {
            theme::err().bg(theme::STATUSBAR_BG)
        } else if hovered {
            theme::bright().bg(theme::STATUSBAR_BG)
        } else {
            theme::statusbar()
        };
        spans.push(Span::styled(lab_s, label_style));
        spans.push(Span::styled("  ", theme::statusbar()));
        x += seg_w + 2;
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::statusbar()),
        area,
    );
}
