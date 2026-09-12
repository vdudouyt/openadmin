//! Header band: numbered screen tabs on the right, and on the left either the
//! brand or — on the Shells screen — the open shell tabs. All on a single row,
//! so the chrome costs as little of the terminal as possible.
//!
//! The design's mark is a solid orange block two rows tall
//! (`design/assets/cloudflare-ascii-logo.txt`); at this height it is the same
//! honest primitive drawn once.

use crate::app::{App, Screen};
use crate::ui::screens;
use crate::ui::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    // Draw the screen tabs first: they own the right edge, and how much room
    // they leave decides what fits beside them. They are also the only mouse
    // route off a focused pane, so they never yield.
    let used = render_tabs(f, area, app);
    let room = area.width.saturating_sub(used);
    let left = Rect::new(area.x, area.y, room, area.height);

    // The Shells screen has no tab strip of its own — its tabs live here, and
    // the brand steps aside for them. With nothing open there is nothing to
    // name, so the brand keeps the space.
    if app.screen == Screen::Shells && !app.term.is_empty() {
        screens::shells::render_tab_strip(f, left, app);
        return;
    }

    // Shed the wordmark whole rather than truncating it mid-word; the mark
    // alone still reads as the brand.
    let brand: Vec<Span> = if room >= 14 {
        vec![
            Span::styled("███ ", theme::proxied()),
            Span::styled("OpenAdmin", theme::bright().add_modifier(Modifier::BOLD)),
        ]
    } else if room >= 3 {
        vec![Span::styled("███", theme::proxied())]
    } else {
        Vec::new()
    };
    if !brand.is_empty() {
        // Clip to the room the tabs left. A Paragraph does not erase what is
        // under it, but it does patch its style across its whole rect, so a
        // styled one drawn into `area` would recolour the strip.
        f.render_widget(Paragraph::new(Line::from(brand)), left);
    }
}

/// The `1 Hosts  2 Shells  3 Chat` strip, right-aligned, each tab registering
/// its own hitbox. On the Shells screen these are the way back out of a
/// focused terminal, so they must always be reachable.
fn render_tabs(f: &mut Frame, area: Rect, app: &mut App) -> u16 {
    let counts = [
        Some(app.hosts.len()),
        Some(app.term.tab_count()),
        None::<usize>,
    ];

    // Measure first so the strip can be right-aligned and hit-tested exactly.
    let measure = |labels: bool| -> Vec<u16> {
        Screen::ALL
            .iter()
            .enumerate()
            .map(|(i, s)| {
                // A leading space and the digit, then optionally the label and
                // its count, then a trailing space.
                let mut w = 2usize;
                if labels {
                    w += 1 + s.label().chars().count();
                    if let Some(n) = counts[i] {
                        w += 1 + n.to_string().chars().count();
                    }
                }
                w as u16 + 1
            })
            .collect()
    };
    let span =
        |widths: &[u16]| -> u16 { widths.iter().sum::<u16>() + 2 * (widths.len() as u16 - 1) };

    // These tabs are the only mouse route off a focused pane, now that the
    // Shells screen draws no function bar — so when the full strip will not
    // fit, shed the labels rather than the tabs themselves.
    let mut widths = measure(true);
    let mut labels = true;
    if span(&widths) > area.width {
        widths = measure(false);
        labels = false;
        if span(&widths) > area.width {
            return 0;
        }
    }
    let total = span(&widths);

    let mut x = area.x + area.width - total;
    let y = area.y;
    let mut spans: Vec<Span> = Vec::new();
    for (i, s) in Screen::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        let rect = Rect::new(x, y, widths[i], 1);
        let active = app.screen == *s;
        app.regions.screen_tabs.push((rect, *s));
        let hovered = app.is_hovered(rect);

        let (num_style, lab_style) = if active {
            (theme::primary_btn(), theme::primary_btn())
        } else if hovered {
            (theme::hover().fg(theme::ORANGE_BRIGHT), theme::hover())
        } else {
            (theme::proxied(), theme::muted())
        };
        spans.push(Span::styled(format!(" {}", i + 1), num_style));
        if labels {
            spans.push(Span::styled(format!(" {}", s.label()), lab_style));
            if let Some(n) = counts[i] {
                spans.push(Span::styled(
                    format!(" {n}"),
                    lab_style.patch(theme::faint()),
                ));
            }
        }
        spans.push(Span::styled(" ", lab_style));
        x += widths[i] + 2;
    }

    // Draw into exactly the columns measured, so the strip and the left band
    // are provably disjoint instead of merely drawn in the right order.
    let strip = Rect::new(area.x + area.width - total, area.y, total, area.height);
    f.render_widget(Paragraph::new(Line::from(spans)), strip);
    total
}
