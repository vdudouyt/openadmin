//! Color tokens from `claude-design/colors_and_type.css`, plus semantic styles
//! so call-sites never hardcode hex. This is the full palette; not every token
//! is referenced yet.
#![allow(dead_code)]

use ratatui::style::{Color, Modifier, Style};

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

// Orange family — the single accent.
pub const ORANGE: Color = rgb(0xf6, 0x82, 0x1f);
pub const ORANGE_BRIGHT: Color = rgb(0xfb, 0xad, 0x41);
pub const ORANGE_DIM: Color = rgb(0xa8, 0x58, 0x16);
pub const ORANGE_INK: Color = rgb(0x1a, 0x12, 0x00);

// Surfaces (low-contrast dark grays).
pub const BG_BASE: Color = rgb(0x1a, 0x1a, 0x1c);
pub const BG_PANEL: Color = rgb(0x21, 0x21, 0x24);
pub const BG_ALT: Color = rgb(0x23, 0x23, 0x27); // zebra
pub const BG_INSET: Color = rgb(0x15, 0x15, 0x17); // wells / textareas
pub const BG_SEL: Color = rgb(0x3a, 0x2f, 0x1c); // selected (unfocused)

// Foreground (dim white, stepped).
pub const FG_BRIGHT: Color = rgb(0xe6, 0xe6, 0xe8);
pub const FG: Color = rgb(0xc2, 0xc2, 0xc6);
pub const FG_MUTED: Color = rgb(0x8b, 0x8b, 0x92);
pub const FG_FAINT: Color = rgb(0x5c, 0x5c, 0x63);
pub const FG_DISABLED: Color = rgb(0x3f, 0x3f, 0x45);

// Semantic hues (desaturated).
pub const GREEN: Color = rgb(0x6c, 0xc0, 0x8b);
pub const RED: Color = rgb(0xe0, 0x58, 0x4f);
pub const BLUE: Color = rgb(0x5b, 0x9b, 0xc7);
pub const YELLOW: Color = rgb(0xd6, 0xa2, 0x3e);
pub const MAGENTA: Color = rgb(0xb8, 0x88, 0xc9);
/// Bright highlight for multi-selected (Insert-marked) rows.
pub const YELLOW_MARK: Color = rgb(0xe8, 0xc4, 0x5a);

// Structure.
/// Row under the mouse pointer (design token --bg-hover).
pub const BG_HOVER: Color = rgb(0x2c, 0x2c, 0x31);

pub const LINE: Color = rgb(0x34, 0x34, 0x3a);
pub const LINE_STRONG: Color = rgb(0x4a, 0x4a, 0x52);

// Status bar.
pub const STATUSBAR_BG: Color = rgb(0x2a, 0x2a, 0x2f);

// ---- semantic style builders --------------------------------------------------

pub fn base() -> Style {
    Style::new().bg(BG_BASE).fg(FG)
}
pub fn body() -> Style {
    Style::new().fg(FG)
}
pub fn muted() -> Style {
    Style::new().fg(FG_MUTED)
}
pub fn faint() -> Style {
    Style::new().fg(FG_FAINT)
}
pub fn bright() -> Style {
    Style::new().fg(FG_BRIGHT)
}
pub fn col_header() -> Style {
    Style::new().fg(FG_MUTED).add_modifier(Modifier::BOLD)
}
pub fn sel_focused() -> Style {
    Style::new().bg(ORANGE).fg(ORANGE_INK)
}
pub fn sel_unfocused() -> Style {
    Style::new().bg(BG_SEL).fg(FG_BRIGHT)
}
pub fn zebra(even: bool) -> Style {
    Style::new().bg(if even { BG_PANEL } else { BG_ALT })
}
pub fn keycap() -> Style {
    Style::new()
        .fg(ORANGE_BRIGHT)
        .bg(LINE)
        .add_modifier(Modifier::BOLD)
}
pub fn statusbar() -> Style {
    Style::new().bg(STATUSBAR_BG).fg(FG_MUTED)
}
pub fn border_focused() -> Style {
    Style::new().fg(ORANGE)
}
pub fn border_idle() -> Style {
    Style::new().fg(LINE)
}
pub fn border_modal() -> Style {
    Style::new().fg(ORANGE_DIM)
}
pub fn ok() -> Style {
    Style::new().fg(GREEN)
}
pub fn warn() -> Style {
    Style::new().fg(YELLOW)
}
pub fn err() -> Style {
    Style::new().fg(RED)
}
pub fn proxied() -> Style {
    Style::new().fg(ORANGE)
}
pub fn dns_only() -> Style {
    Style::new().fg(FG_MUTED)
}
pub fn disabled_glyph() -> Style {
    Style::new().fg(FG_DISABLED)
}
pub fn primary_btn() -> Style {
    Style::new()
        .bg(ORANGE)
        .fg(ORANGE_INK)
        .add_modifier(Modifier::BOLD)
}
pub fn danger_btn() -> Style {
    Style::new()
        .bg(RED)
        .fg(rgb(0x1a, 0x00, 0x00))
        .add_modifier(Modifier::BOLD)
}

/// Host protocol accent: SSH is blue, FTP magenta (HostsScreen.jsx:47).
pub fn proto_color(proto: &str) -> Color {
    match proto.to_ascii_uppercase().as_str() {
        "SSH" => BLUE,
        "FTP" => MAGENTA,
        _ => FG_MUTED,
    }
}

pub fn hover() -> Style {
    Style::new().bg(BG_HOVER).fg(FG_BRIGHT)
}

/// A marked (Insert-tagged) row: yellow ink on the warm wash, so it stays
/// readable when the orange cursor bar is elsewhere.
pub fn marked() -> Style {
    Style::new()
        .bg(BG_SEL)
        .fg(YELLOW_MARK)
        .add_modifier(Modifier::BOLD)
}
