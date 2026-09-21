//! Screen 1 — known hosts (`design/ui_kits/openadmin/HostsScreen.jsx`).
//!
//! Rendered as full-width `Line`s rather than the `Table` widget, so the
//! selection bar is gapless and the columns line up exactly — the same reason
//! cfdns hand-renders its grid (`/root/cfdns/src/ui/table.rs:1-2`).
//!
//! Two orthogonal selection signals, per the design: the orange cursor bar is
//! *where you are*, the yellow `●` marks are *what you tagged*, so a marked row
//! stays readable underneath the cursor.

use crate::app::App;
use crate::db::model::HostRecord;
use crate::ui::theme;
use crate::ui::widgets::{pad, padl};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

/// Fixed column widths; ADDR and MOUNT POINT absorb the slack. The mockup's
/// 120-column reference is `HostsScreen.jsx:6`.
struct Cols {
    mark: usize,
    name: usize,
    proto: usize,
    addr: usize,
    port: usize,
    gap: usize,
    mount: usize,
    login: usize,
    pass: usize,
    mnt: usize,
    key: usize,
    prx: usize,
}

/// Solve the layout for `width`.
///
/// Columns are shed in increasing order of value before the text columns get
/// squeezed into uselessness, then whatever is left is split between NAME,
/// ADDR and MOUNT POINT in the mockup's proportions (`HostsScreen.jsx:6`
/// budgets 56 columns across the three at width 120). A zero width renders as
/// an empty string, so a shed column simply disappears.
fn cols(width: usize) -> Cols {
    let mark = 2;
    // MNT is six wide because `[yes]` is five and a cell needs a gap after it;
    // KEY is one glyph and keeps the five MNT used to have.
    let (mut proto, port, mut gap, mut login, mut pass, mnt, mut key, mut prx) =
        (6usize, 6, 2, 11, 11, 6, 5, 5);
    if width < 100 {
        pass = 0;
    }
    if width < 86 {
        login = 0;
    }
    if width < 70 {
        proto = 0;
        gap = 0;
    }
    if width < 58 {
        key = 0;
        prx = 0;
    }
    let fixed = mark + proto + port + gap + login + pass + mnt + key + prx;
    let flex = width.saturating_sub(fixed);
    let name = (flex * 14 / 56).min(24);
    let addr = flex.saturating_sub(name) * 22 / 34;
    let mount = flex.saturating_sub(name + addr);
    Cols {
        mark,
        name,
        proto,
        addr,
        port,
        gap,
        mount,
        login,
        pass,
        mnt,
        key,
        prx,
    }
}

pub(crate) fn mask_pass(p: &str) -> String {
    if p.is_empty() {
        "—".to_string()
    } else {
        "•".repeat(p.chars().count().min(8))
    }
}

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let right = if app.marked.is_empty() {
        format!(" {} hosts ", app.hosts.len())
    } else {
        format!(" {} marked of {} ", app.marked.len(), app.hosts.len())
    };
    let block = Block::bordered()
        .border_style(theme::border_focused())
        .style(Style::new().bg(theme::BG_BASE))
        .title_top(Line::styled(
            " Known Hosts ",
            theme::bright().add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::styled(right, theme::muted()).right_aligned());
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 || inner.width < 20 {
        return;
    }

    let c = cols(inner.width as usize);
    let mut lines: Vec<Line> = Vec::new();

    // Column headers.
    let h = theme::col_header();
    lines.push(Line::from(vec![
        Span::styled(pad("", c.mark), h),
        Span::styled(pad("NAME", c.name), h),
        Span::styled(pad("TYPE", c.proto), h),
        Span::styled(pad("ADDR", c.addr), h),
        Span::styled(padl("PORT", c.port), h),
        Span::styled(pad("", c.gap), h),
        Span::styled(pad("MOUNT POINT", c.mount), h),
        Span::styled(pad("LOGIN", c.login), h),
        Span::styled(pad("PASSWORD", c.pass), h),
        Span::styled(pad("MNT", c.mnt), h),
        Span::styled(pad("PRX", c.prx), h),
        Span::styled(pad("KEY", c.key), h),
    ]));

    // Scroll window, keeping the cursor centered like the mockup.
    let visible = inner.height.saturating_sub(1) as usize;
    let start = if app.hosts.len() > visible {
        app.cursor
            .saturating_sub(visible / 2)
            .min(app.hosts.len() - visible)
    } else {
        0
    };
    app.scroll = start;

    // Register hitboxes before drawing, so clicks and pixels agree.
    app.regions.rows = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        visible.min(app.hosts.len().saturating_sub(start)) as u16,
    );
    app.regions.row_start = start;
    let hovered = app.hovered_row();

    for (i, host) in app.hosts.iter().enumerate().skip(start).take(visible) {
        lines.push(row(
            host,
            &c,
            i == app.cursor,
            app.marked.contains(&host.id),
            hovered == Some(i),
        ));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn row(h: &HostRecord, c: &Cols, cursor: bool, marked: bool, hover: bool) -> Line<'static> {
    // Background priority: the cursor bar wins, then the mark wash, then hover.
    let bg = if cursor {
        Some(theme::ORANGE)
    } else if marked {
        Some(theme::BG_SEL)
    } else if hover {
        Some(theme::BG_HOVER)
    } else {
        None
    };
    // Ink on the orange bar must be dark; on the mark wash it stays yellow.
    let ink = if cursor {
        theme::ORANGE_INK
    } else if marked {
        theme::YELLOW_MARK
    } else {
        theme::FG
    };

    let st = |fg| match bg {
        Some(b) => Style::new().fg(fg).bg(b),
        None => Style::new().fg(fg),
    };
    // A per-cell accent collapses to the row ink when the row is highlighted,
    // so contrast is never lost.
    let accent = |fg| if cursor || marked { st(ink) } else { st(fg) };

    let glyph = if marked {
        "●"
    } else if cursor {
        "▸"
    } else {
        " "
    };
    let mark_style = if marked && !cursor {
        st(theme::YELLOW_MARK).add_modifier(Modifier::BOLD)
    } else {
        st(ink)
    };

    // Mounted is spelled out: it is the state an operator acts on from this
    // screen, and a word reads at a glance where a filled circle has to be told
    // apart from an empty one. Both states in the bright orange the old `[gen]`
    // cell had — the word carries the state, the colour marks the column.
    let mnt_text = if h.mounted { "[yes]" } else { "[no]" };
    let mnt_fg = theme::ORANGE_BRIGHT;
    // Whether a key is installed, and nothing more: making one is the edit
    // form's business, so this is a status and not a button.
    let (key_text, key_fg) = if h.key_name.is_empty() {
        ("○", theme::FG_FAINT)
    } else {
        ("●", theme::GREEN)
    };
    let (prx_text, prx_fg) = if h.proxy {
        ("●", theme::ORANGE)
    } else {
        ("○", theme::FG_FAINT)
    };

    let name_style = if marked && !cursor {
        st(theme::YELLOW_MARK).add_modifier(Modifier::BOLD)
    } else {
        st(ink)
    };

    Line::from(vec![
        Span::styled(pad(glyph, c.mark), mark_style),
        Span::styled(pad(&h.name, c.name), name_style),
        Span::styled(
            pad(&h.proto.to_uppercase(), c.proto),
            accent(theme::proto_color(&h.proto)),
        ),
        Span::styled(pad(&h.addr, c.addr), st(ink)),
        Span::styled(padl(&h.port.to_string(), c.port), accent(theme::FG_MUTED)),
        Span::styled(pad("", c.gap), st(ink)),
        Span::styled(pad(&h.mount_point, c.mount), accent(theme::FG_MUTED)),
        Span::styled(pad(&h.login, c.login), st(ink)),
        Span::styled(
            pad(&mask_pass(&h.pass), c.pass),
            accent(if h.pass.is_empty() {
                theme::FG_DISABLED
            } else {
                theme::FG_MUTED
            }),
        ),
        Span::styled(pad(mnt_text, c.mnt), accent(mnt_fg)),
        Span::styled(pad(prx_text, c.prx), accent(prx_fg)),
        Span::styled(pad(key_text, c.key), accent(key_fg)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(c: &Cols) -> usize {
        c.mark
            + c.name
            + c.proto
            + c.addr
            + c.port
            + c.gap
            + c.mount
            + c.login
            + c.pass
            + c.mnt
            + c.key
            + c.prx
    }

    /// The table must never draw wider than the panel, at any size.
    #[test]
    fn columns_never_overflow_the_available_width() {
        for w in 20usize..=250 {
            let c = cols(w);
            assert!(total(&c) <= w, "width {w}: columns sum to {}", total(&c));
        }
    }

    #[test]
    fn a_full_width_table_matches_the_mockup_proportions() {
        let c = cols(120);
        assert_eq!(total(&c), 120, "the row fills the panel exactly");
        assert!(c.name >= 14, "NAME got {}", c.name);
        assert!(c.addr >= 20, "ADDR got {}", c.addr);
        assert!(c.mount >= 18, "MOUNT POINT got {}", c.mount);
        // Every column is present at the design width.
        assert!(c.pass > 0 && c.login > 0 && c.proto > 0 && c.key > 0 && c.prx > 0);
    }

    /// Narrow terminals shed columns rather than mangling every cell.
    #[test]
    fn narrow_widths_shed_low_value_columns_first() {
        let c = cols(90);
        assert_eq!(c.pass, 0, "PASSWORD goes first");
        assert!(c.login > 0 && c.name >= 10);

        let c = cols(64);
        assert_eq!(c.login, 0);
        assert_eq!(c.proto, 0);
        assert!(c.name >= 8, "NAME stays readable: {}", c.name);
        assert!(c.addr >= 10, "ADDR stays readable: {}", c.addr);
        assert!(c.mnt > 0, "the mount indicator survives");

        // `[yes]` never loses its bracket to the next column.
        assert!(cols(120).mnt > "[yes]".chars().count());
    }

    #[test]
    fn passwords_are_masked_and_capped() {
        assert_eq!(mask_pass(""), "—");
        assert_eq!(mask_pass("abc"), "•••");
        assert_eq!(
            mask_pass("0123456789abcdef"),
            "••••••••",
            "mask caps at 8 bullets"
        );
    }
}
