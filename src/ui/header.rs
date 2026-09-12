//! Header band: brand mark + wordmark on the left, numbered screen tabs right,
//! then a full-width rule (`design/ui_kits/openadmin/AppChrome.jsx:10-37`).

use crate::app::{App, Screen};
use crate::ui::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let [top, divider] =
        Layout::vertical([Constraint::Length(2), Constraint::Length(1)]).areas(area);

    let brand = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("▟███▙ ", theme::proxied()),
            Span::styled("OpenAdmin", theme::bright().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("▜███▛ ", theme::proxied()),
            Span::styled("remote hosts · shells · agent", theme::muted()),
        ]),
    ]);
    f.render_widget(brand, top);

    render_tags(f, top, app);

    let rule = "─".repeat(area.width as usize);
    f.render_widget(
        Paragraph::new(Line::styled(rule, theme::border_idle())),
        divider,
    );
}

/// The `1 Hosts  2 Shells  3 Chat` strip, right-aligned on the header's second
/// row, with each tab registering its own hitbox.
fn render_tags(f: &mut Frame, top: Rect, app: &mut App) {
    let counts = [
        Some(app.hosts.len()),
        Some(app.term.tab_count()),
        None::<usize>,
    ];

    // Measure first so the strip can be right-aligned and hit-tested exactly.
    let mut widths = Vec::new();
    for (i, s) in Screen::ALL.iter().enumerate() {
        let mut w = 1 + 1 + s.label().chars().count(); // "1 Hosts"
        if let Some(n) = counts[i] {
            w += 1 + n.to_string().chars().count();
        }
        widths.push(w as u16 + 2); // one space of padding each side
    }
    let total: u16 = widths.iter().sum::<u16>() + 2 * (widths.len() as u16 - 1);
    if total >= top.width {
        return;
    }

    let mut x = top.x + top.width - total;
    let y = top.y + 1;
    let mut spans: Vec<Span> = Vec::new();
    for (i, s) in Screen::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        let active = app.screen == *s;
        app.regions
            .screen_tabs
            .push((Rect::new(x, y, widths[i], 1), *s));
        let hovered = app.is_hovered(Rect::new(x, y, widths[i], 1));

        let (num_style, lab_style) = if active {
            (theme::primary_btn(), theme::primary_btn())
        } else if hovered {
            (theme::hover().fg(theme::ORANGE_BRIGHT), theme::hover())
        } else {
            (theme::proxied(), theme::muted())
        };
        spans.push(Span::styled(format!(" {}", i + 1), num_style));
        spans.push(Span::styled(format!(" {}", s.label()), lab_style));
        if let Some(n) = counts[i] {
            spans.push(Span::styled(
                format!(" {n}"),
                lab_style.patch(theme::faint()),
            ));
        }
        spans.push(Span::styled(" ", lab_style));
        x += widths[i] + 2;
    }

    let strip = Rect::new(top.x, y, top.width, 1);
    f.render_widget(Paragraph::new(Line::from(spans).right_aligned()), strip);
}
