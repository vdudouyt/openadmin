//! Translating crossterm input into the byte sequences a terminal program
//! expects. This is the layer that decides whether arrow keys work inside mc
//! and vim, so it implements real xterm encoding rather than an approximation.

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use vt100::{MouseProtocolEncoding, MouseProtocolMode};

const ESC: u8 = 0x1b;

/// xterm's modifier parameter: 1 + shift(1) + alt(2) + ctrl(4).
fn modifier_param(m: KeyModifiers) -> u8 {
    1 + u8::from(m.contains(KeyModifiers::SHIFT))
        + 2 * u8::from(m.contains(KeyModifiers::ALT))
        + 4 * u8::from(m.contains(KeyModifiers::CONTROL))
}

/// `CSI <n> ~`, or `CSI <n> ; <m> ~` when modified.
fn tilde(n: u8, m: u8) -> Vec<u8> {
    if m > 1 {
        format!("\x1b[{n};{m}~").into_bytes()
    } else {
        format!("\x1b[{n}~").into_bytes()
    }
}

/// Cursor-style keys: `SS3 <f>` in application mode, `CSI <f>` otherwise, and
/// always `CSI 1 ; <m> <f>` when modified — the DECCKM distinction that mc and
/// vim depend on.
fn cursor_key(final_byte: char, m: u8, app_cursor: bool) -> Vec<u8> {
    if m > 1 {
        format!("\x1b[1;{m}{final_byte}").into_bytes()
    } else if app_cursor {
        format!("\x1bO{final_byte}").into_bytes()
    } else {
        format!("\x1b[{final_byte}").into_bytes()
    }
}

/// Map a character to its control byte, as a terminal driver would.
fn control_byte(c: char) -> Option<u8> {
    match c {
        ' ' | '@' => Some(0x00),
        'a'..='z' => Some(c as u8 - b'a' + 1),
        'A'..='Z' => Some(c as u8 - b'A' + 1),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' => Some(0x1f),
        '?' => Some(0x7f),
        _ => None,
    }
}

/// Encode one key press for the child process.
///
/// `app_cursor` is the DECCKM state from `vt100::Screen::application_cursor()`.
pub fn encode(key: KeyEvent, app_cursor: bool) -> Vec<u8> {
    let m = modifier_param(key.modifiers);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    let mut out = match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                match control_byte(c) {
                    Some(b) => vec![b],
                    None => c.to_string().into_bytes(),
                }
            } else {
                c.to_string().into_bytes()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        // The terminal convention is DEL for Backspace; Ctrl+Backspace sends BS.
        KeyCode::Backspace => vec![if ctrl { 0x08 } else { 0x7f }],
        KeyCode::Esc => vec![ESC],
        KeyCode::Up => cursor_key('A', m, app_cursor),
        KeyCode::Down => cursor_key('B', m, app_cursor),
        KeyCode::Right => cursor_key('C', m, app_cursor),
        KeyCode::Left => cursor_key('D', m, app_cursor),
        KeyCode::Home => cursor_key('H', m, app_cursor),
        KeyCode::End => cursor_key('F', m, app_cursor),
        KeyCode::Insert => tilde(2, m),
        KeyCode::Delete => tilde(3, m),
        KeyCode::PageUp => tilde(5, m),
        KeyCode::PageDown => tilde(6, m),
        // F1-F4 are SS3-based; F5 and up use the numbered tilde form.
        KeyCode::F(n @ 1..=4) => {
            let final_byte = (b'P' + (n - 1)) as char;
            if m > 1 {
                format!("\x1b[1;{m}{final_byte}").into_bytes()
            } else {
                format!("\x1bO{final_byte}").into_bytes()
            }
        }
        KeyCode::F(n) => match n {
            5 => tilde(15, m),
            6 => tilde(17, m),
            7 => tilde(18, m),
            8 => tilde(19, m),
            9 => tilde(20, m),
            10 => tilde(21, m),
            11 => tilde(23, m),
            12 => tilde(24, m),
            _ => Vec::new(),
        },
        KeyCode::Null => vec![0],
        _ => Vec::new(),
    };

    // Alt is an ESC prefix — but only where the modifier was not already
    // folded into a CSI parameter above.
    if alt
        && !out.is_empty()
        && matches!(
            key.code,
            KeyCode::Char(_) | KeyCode::Enter | KeyCode::Backspace | KeyCode::Tab
        )
    {
        out.insert(0, ESC);
    }
    out
}

/// Wrap pasted text in bracketed-paste markers when the child asked for them,
/// so editors do not auto-indent every line.
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(text.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        text.as_bytes().to_vec()
    }
}

/// Encode a mouse event for a child that has enabled mouse reporting.
///
/// `col`/`row` are already pane-local and 0-based. Returns `None` when the
/// child does not want this event, in which case the mouse belongs to
/// OpenAdmin's own UI.
pub fn encode_mouse(
    ev: &MouseEvent,
    col: u16,
    row: u16,
    mode: MouseProtocolMode,
    encoding: MouseProtocolEncoding,
) -> Option<Vec<u8>> {
    let motion = matches!(ev.kind, MouseEventKind::Drag(_) | MouseEventKind::Moved);
    let wanted = match mode {
        MouseProtocolMode::None => false,
        // Press-only mode never reports releases or motion.
        MouseProtocolMode::Press => {
            matches!(
                ev.kind,
                MouseEventKind::Down(_) | MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            )
        }
        MouseProtocolMode::PressRelease => !motion,
        // Button-motion reports drags but not free movement.
        MouseProtocolMode::ButtonMotion => !matches!(ev.kind, MouseEventKind::Moved),
        MouseProtocolMode::AnyMotion => true,
    };
    if !wanted {
        return None;
    }

    let (button, release) = match ev.kind {
        MouseEventKind::Down(b) => (mouse_button(b), false),
        MouseEventKind::Up(b) => (mouse_button(b), true),
        MouseEventKind::Drag(b) => (mouse_button(b) + 32, false),
        MouseEventKind::Moved => (35, false),
        MouseEventKind::ScrollUp => (64, false),
        MouseEventKind::ScrollDown => (65, false),
        MouseEventKind::ScrollLeft => (66, false),
        MouseEventKind::ScrollRight => (67, false),
    };

    let mut code = button;
    if ev.modifiers.contains(KeyModifiers::SHIFT) {
        code += 4;
    }
    if ev.modifiers.contains(KeyModifiers::ALT) {
        code += 8;
    }
    if ev.modifiers.contains(KeyModifiers::CONTROL) {
        code += 16;
    }

    // Terminal coordinates are 1-based.
    let (x, y) = (col as u32 + 1, row as u32 + 1);

    Some(match encoding {
        MouseProtocolEncoding::Sgr => format!(
            "\x1b[<{};{};{}{}",
            code,
            x,
            y,
            if release { 'm' } else { 'M' }
        )
        .into_bytes(),
        // The original X10 encoding offsets everything by 32 and cannot express
        // a release, which is reported as button 3.
        _ => {
            let b = if release { 3 } else { code };
            let clamp = |v: u32| -> u8 { (v.min(223) as u8).saturating_add(32) };
            vec![ESC, b'[', b'M', b.saturating_add(32), clamp(x), clamp(y)]
        }
    })
}

fn mouse_button(b: MouseButton) -> u8 {
    match b {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }
    fn key_mod(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    /// The DECCKM switch: mc and vim put the terminal in application-cursor
    /// mode and then expect SS3-prefixed arrows.
    #[test]
    fn arrows_follow_application_cursor_mode() {
        assert_eq!(encode(key(KeyCode::Up), false), b"\x1b[A");
        assert_eq!(encode(key(KeyCode::Up), true), b"\x1bOA");
        assert_eq!(encode(key(KeyCode::Left), false), b"\x1b[D");
        assert_eq!(encode(key(KeyCode::Left), true), b"\x1bOD");
    }

    /// A modified arrow always uses the CSI form, even in application mode.
    #[test]
    fn modified_arrows_use_the_csi_parameter_form() {
        assert_eq!(
            encode(key_mod(KeyCode::Up, KeyModifiers::CONTROL), true),
            b"\x1b[1;5A"
        );
        assert_eq!(
            encode(key_mod(KeyCode::Right, KeyModifiers::SHIFT), false),
            b"\x1b[1;2C"
        );
    }

    #[test]
    fn function_keys_split_between_ss3_and_tilde_forms() {
        assert_eq!(encode(key(KeyCode::F(1)), false), b"\x1bOP");
        assert_eq!(encode(key(KeyCode::F(4)), false), b"\x1bOS");
        assert_eq!(encode(key(KeyCode::F(5)), false), b"\x1b[15~");
        assert_eq!(encode(key(KeyCode::F(9)), false), b"\x1b[20~");
        assert_eq!(encode(key(KeyCode::F(10)), false), b"\x1b[21~");
        assert_eq!(encode(key(KeyCode::F(12)), false), b"\x1b[24~");
    }

    #[test]
    fn shift_f_keys_carry_the_modifier_parameter() {
        assert_eq!(
            encode(key_mod(KeyCode::F(2), KeyModifiers::SHIFT), false),
            b"\x1b[1;2Q"
        );
        assert_eq!(
            encode(key_mod(KeyCode::F(5), KeyModifiers::SHIFT), false),
            b"\x1b[15;2~"
        );
    }

    /// GNU Screen's prefix is Ctrl+A; it must arrive as 0x01.
    #[test]
    fn control_letters_map_to_the_control_range() {
        assert_eq!(
            encode(key_mod(KeyCode::Char('a'), KeyModifiers::CONTROL), false),
            vec![0x01]
        );
        assert_eq!(
            encode(key_mod(KeyCode::Char('c'), KeyModifiers::CONTROL), false),
            vec![0x03]
        );
        assert_eq!(
            encode(key_mod(KeyCode::Char('z'), KeyModifiers::CONTROL), false),
            vec![0x1a]
        );
        assert_eq!(
            encode(key_mod(KeyCode::Char('['), KeyModifiers::CONTROL), false),
            vec![0x1b]
        );
        assert_eq!(
            encode(key_mod(KeyCode::Char(' '), KeyModifiers::CONTROL), false),
            vec![0x00]
        );
    }

    #[test]
    fn alt_prefixes_an_escape() {
        assert_eq!(
            encode(key_mod(KeyCode::Char('x'), KeyModifiers::ALT), false),
            vec![ESC, b'x']
        );
    }

    #[test]
    fn basic_editing_keys() {
        assert_eq!(encode(key(KeyCode::Enter), false), b"\r");
        assert_eq!(encode(key(KeyCode::Tab), false), b"\t");
        assert_eq!(encode(key(KeyCode::BackTab), false), b"\x1b[Z");
        assert_eq!(encode(key(KeyCode::Backspace), false), vec![0x7f]);
        assert_eq!(encode(key(KeyCode::Esc), false), vec![ESC]);
        assert_eq!(encode(key(KeyCode::Delete), false), b"\x1b[3~");
        assert_eq!(encode(key(KeyCode::PageUp), false), b"\x1b[5~");
    }

    #[test]
    fn utf8_characters_survive() {
        assert_eq!(encode(key(KeyCode::Char('é')), false), "é".as_bytes());
    }

    #[test]
    fn unhandled_keys_produce_nothing() {
        let ev = KeyEvent::new(KeyCode::CapsLock, KeyModifiers::empty());
        assert!(encode(ev, false).is_empty());
        assert_eq!(KeyEventKind::Press, ev.kind);
    }

    #[test]
    fn paste_is_bracketed_only_when_requested() {
        assert_eq!(encode_paste("hi", false), b"hi");
        assert_eq!(encode_paste("hi", true), b"\x1b[200~hi\x1b[201~");
    }

    fn mouse(kind: MouseEventKind) -> MouseEvent {
        MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        }
    }

    #[test]
    fn no_mouse_reporting_means_the_ui_keeps_the_event() {
        let ev = mouse(MouseEventKind::Down(MouseButton::Left));
        assert!(
            encode_mouse(
                &ev,
                3,
                4,
                MouseProtocolMode::None,
                MouseProtocolEncoding::Sgr
            )
            .is_none()
        );
    }

    #[test]
    fn sgr_press_and_release_use_one_based_coordinates() {
        let down = mouse(MouseEventKind::Down(MouseButton::Left));
        assert_eq!(
            encode_mouse(
                &down,
                3,
                4,
                MouseProtocolMode::PressRelease,
                MouseProtocolEncoding::Sgr
            )
            .unwrap(),
            b"\x1b[<0;4;5M"
        );
        let up = mouse(MouseEventKind::Up(MouseButton::Left));
        assert_eq!(
            encode_mouse(
                &up,
                3,
                4,
                MouseProtocolMode::PressRelease,
                MouseProtocolEncoding::Sgr
            )
            .unwrap(),
            b"\x1b[<0;4;5m"
        );
    }

    #[test]
    fn press_only_mode_drops_releases_and_motion() {
        let up = mouse(MouseEventKind::Up(MouseButton::Left));
        assert!(
            encode_mouse(
                &up,
                0,
                0,
                MouseProtocolMode::Press,
                MouseProtocolEncoding::Sgr
            )
            .is_none()
        );
        let moved = mouse(MouseEventKind::Moved);
        assert!(
            encode_mouse(
                &moved,
                0,
                0,
                MouseProtocolMode::ButtonMotion,
                MouseProtocolEncoding::Sgr
            )
            .is_none()
        );
        // ...but AnyMotion wants it.
        assert!(
            encode_mouse(
                &moved,
                0,
                0,
                MouseProtocolMode::AnyMotion,
                MouseProtocolEncoding::Sgr
            )
            .is_some()
        );
    }

    #[test]
    fn scroll_wheel_uses_the_high_button_codes() {
        let up = mouse(MouseEventKind::ScrollUp);
        assert_eq!(
            encode_mouse(
                &up,
                0,
                0,
                MouseProtocolMode::Press,
                MouseProtocolEncoding::Sgr
            )
            .unwrap(),
            b"\x1b[<64;1;1M"
        );
    }

    #[test]
    fn modifiers_are_folded_into_the_button_code() {
        let mut ev = mouse(MouseEventKind::Down(MouseButton::Left));
        ev.modifiers = KeyModifiers::CONTROL;
        assert_eq!(
            encode_mouse(
                &ev,
                0,
                0,
                MouseProtocolMode::PressRelease,
                MouseProtocolEncoding::Sgr
            )
            .unwrap(),
            b"\x1b[<16;1;1M"
        );
    }

    #[test]
    fn legacy_x10_encoding_offsets_by_32() {
        let down = mouse(MouseEventKind::Down(MouseButton::Left));
        assert_eq!(
            encode_mouse(
                &down,
                3,
                4,
                MouseProtocolMode::PressRelease,
                MouseProtocolEncoding::Default
            )
            .unwrap(),
            vec![ESC, b'[', b'M', 32, 36, 37]
        );
    }
}
