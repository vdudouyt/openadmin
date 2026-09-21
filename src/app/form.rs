//! Add/Edit Host form state (`design/ui_kits/openadmin/HostDialogs.jsx`).
//!
//! Modeled on `/root/cfdns/src/app/form.rs`: an explicit field enum drives both
//! the tab order and the renderer, input is filtered per field, and `validate`
//! returns the exact message the status bar flashes while the dialog stays
//! open.

use crate::db::model::{HOST_TYPES, HostRecord, default_port, mount_for};
use ratatui::crossterm::event::{Event, KeyEvent};
use tui_input::Input;
use tui_input::backend::crossterm::to_input_request;

/// What has the keyboard in the form: an input, or one of its buttons.
///
/// The buttons are in the Tab order after the inputs, so everything the dialog
/// offers can be reached without the mouse — each also keeps its own key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Name,
    Type,
    Addr,
    Port,
    Mount,
    Login,
    Pass,
    /// Generate an SSH key, or show the one installed (F7).
    KeyButton,
    /// Bulk add (F2) — in the Add dialog only.
    BulkAdd,
    /// Add Host, or Save when editing (Enter from an input).
    Save,
    /// Cancel (Esc).
    Cancel,
}

impl FormField {
    /// A button takes no text; ↵ or Space presses it.
    pub fn is_button(self) -> bool {
        matches!(
            self,
            FormField::KeyButton | FormField::BulkAdd | FormField::Save | FormField::Cancel
        )
    }
}

/// The inputs, in tab order, matching `FORM_FIELDS` in HostDialogs.jsx:15. The
/// buttons follow them; `FormState::focus_order` has the whole of it.
pub const FORM_FIELDS: [FormField; 7] = [
    FormField::Name,
    FormField::Type,
    FormField::Addr,
    FormField::Port,
    FormField::Mount,
    FormField::Login,
    FormField::Pass,
];

#[derive(Debug, Clone)]
pub struct FormState {
    /// 0 for a new host; the existing row id when editing.
    pub id: i64,
    /// The editable fields are `tui_input::Input` rather than `String`: each
    /// carries its own caret and steps by *grapheme*, so `é` written as `e`
    /// plus a combining accent moves and deletes as the one character a
    /// person sees. Read them with `.value()`.
    pub name: Input,
    pub proto: String,
    pub addr: Input,
    pub port: Input,
    pub mount: Input,
    /// While true the mount point tracks the host name. Cleared the moment the
    /// user types in the mount field, restored when they empty it.
    pub mount_auto: bool,
    pub login: Input,
    pub pass: Input,
    pub key_name: String,
    pub key_value: String,
    pub proxy: bool,
    pub focus: FormField,
}

impl FormState {
    pub fn new(mount_prefix: &str) -> Self {
        let _ = mount_prefix;
        FormState {
            id: 0,
            name: Input::default(),
            proto: HOST_TYPES[0].to_string(),
            addr: Input::default(),
            port: Input::new(default_port(HOST_TYPES[0]).to_string()),
            mount: Input::default(),
            mount_auto: true,
            login: Input::default(),
            pass: Input::default(),
            key_name: String::new(),
            key_value: String::new(),
            proxy: false,
            focus: FormField::Name,
        }
    }

    pub fn from_record(rec: &HostRecord, mount_prefix: &str) -> Self {
        // A stored mount point that still matches what the name would generate
        // is treated as auto, so renaming the host keeps moving it.
        let auto =
            rec.mount_point.is_empty() || rec.mount_point == mount_for(mount_prefix, &rec.name);
        FormState {
            id: rec.id,
            // `Input::new` parks each caret after the value, so editing a host
            // continues a field rather than typing in front of it.
            name: Input::new(rec.name.clone()),
            proto: rec.proto.to_ascii_uppercase(),
            addr: Input::new(rec.addr.clone()),
            port: Input::new(rec.port.to_string()),
            mount: Input::new(rec.mount_point.clone()),
            mount_auto: auto,
            login: Input::new(rec.login.clone()),
            pass: Input::new(rec.pass.clone()),
            key_name: rec.key_name.clone(),
            key_value: rec.key_value.clone(),
            proxy: rec.proxy,
            focus: FormField::Name,
        }
    }

    pub fn is_edit(&self) -> bool {
        self.id != 0
    }

    pub fn has_key(&self) -> bool {
        !self.key_name.is_empty()
    }

    /// Everything Tab reaches, in the order it is drawn: the inputs, the key
    /// button beneath them, then the button row left to right. Bulk add only
    /// in Add, where its button is.
    pub fn focus_order(&self) -> Vec<FormField> {
        let mut order = FORM_FIELDS.to_vec();
        order.push(FormField::KeyButton);
        if !self.is_edit() {
            order.push(FormField::BulkAdd);
        }
        order.extend([FormField::Save, FormField::Cancel]);
        order
    }

    pub fn focus_next(&mut self) {
        let order = self.focus_order();
        let i = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        self.focus = order[(i + 1) % order.len()];
    }

    pub fn focus_prev(&mut self) {
        let order = self.focus_order();
        let i = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        self.focus = order[(i + order.len() - 1) % order.len()];
    }

    /// Cycle the protocol, resetting the port to that protocol's default —
    /// but only when the port still holds the *old* default, so a hand-typed
    /// port survives (`app.jsx:50-54` resets unconditionally; keeping a custom
    /// port is the less surprising behavior).
    pub fn cycle_type(&mut self, dir: i32) {
        let i = HOST_TYPES
            .iter()
            .position(|t| *t == self.proto)
            .unwrap_or(0) as i32;
        let n = HOST_TYPES.len() as i32;
        let old_default = default_port(&self.proto).to_string();
        self.proto = HOST_TYPES[(i + dir).rem_euclid(n) as usize].to_string();
        if self.port.value().is_empty() || self.port.value() == old_default {
            self.port = Input::new(default_port(&self.proto).to_string());
        }
    }

    /// The mount point shown for the current name, honoring the override latch.
    pub fn effective_mount(&self, prefix: &str) -> String {
        if self.mount_auto {
            mount_for(prefix, self.name.value())
        } else {
            self.mount.value().to_string()
        }
    }

    /// Feed a keystroke to the focused field.
    ///
    /// `tui-input` owns the editing vocabulary — arrows, Home/End, word
    /// motions, the readline kills — so this only adds the two rules that are
    /// this form's own: the port takes digits, and the mount point follows the
    /// host name for exactly as long as it is empty.
    ///
    /// Returns whether the key was consumed, so the caller can tell a field
    /// edit from a key the dialog should handle.
    pub fn handle_key(&mut self, key: KeyEvent, prefix: &str) -> bool {
        let Some(req) = to_input_request(&Event::Key(key)) else {
            return false;
        };
        // A non-digit typed into the port is not an error, it is a key this
        // field has no use for.
        if self.focus == FormField::Port
            && let tui_input::InputRequest::InsertChar(c) = req
            && !c.is_ascii_digit()
        {
            return true;
        }
        let focus = self.focus;
        let Some(field) = self.field_mut() else {
            return false;
        };
        let changed = field.handle(req).is_some_and(|s| s.value);
        if changed {
            match focus {
                FormField::Name => self.sync_mount(prefix),
                // Emptying the field hands control back to the host name, by
                // any route: Backspace, ^U, or a word kill that took the last
                // of it.
                FormField::Mount => self.mount_auto = self.mount.value().trim().is_empty(),
                _ => {}
            }
        }
        true
    }

    /// The focused field, or `None` on the protocol cycler or a button, which
    /// are not text.
    fn field_mut(&mut self) -> Option<&mut Input> {
        Some(match self.focus {
            FormField::Name => &mut self.name,
            FormField::Type
            | FormField::KeyButton
            | FormField::BulkAdd
            | FormField::Save
            | FormField::Cancel => return None,
            FormField::Addr => &mut self.addr,
            FormField::Port => &mut self.port,
            FormField::Mount => &mut self.mount,
            FormField::Login => &mut self.login,
            FormField::Pass => &mut self.pass,
        })
    }

    fn sync_mount(&mut self, prefix: &str) {
        if self.mount_auto {
            // Keep the caret at the end: the operator is typing the *name*,
            // and this field is following along behind them.
            self.mount = Input::new(mount_for(prefix, self.name.value()));
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.value().trim().is_empty() {
            return Err("Host name is required.".to_string());
        }
        if self.addr.value().trim().is_empty() {
            return Err("Address is required.".to_string());
        }
        if !self.port.value().is_empty() && self.port.value().parse::<u16>().is_err() {
            return Err("Port must be a number between 0 and 65535.".to_string());
        }
        Ok(())
    }

    pub fn to_record(&self, prefix: &str) -> HostRecord {
        let mount = {
            let m = self.effective_mount(prefix);
            if m.trim().is_empty() {
                mount_for(prefix, self.name.value())
            } else {
                m
            }
        };
        HostRecord {
            id: self.id,
            name: self.name.value().trim().to_string(),
            // qhostman stores the protocol lowercased and displays it upper.
            proto: self.proto.to_lowercase(),
            addr: self.addr.value().trim().to_string(),
            mount_point: mount,
            port: self
                .port
                .value()
                .parse::<i64>()
                .unwrap_or_else(|_| default_port(&self.proto)),
            login: self.login.value().to_string(),
            pass: self.pass.value().to_string(),
            key_name: self.key_name.clone(),
            key_value: self.key_value.clone(),
            proxy: self.proxy,
            mounted: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    fn press(f: &mut FormState, code: KeyCode) {
        f.handle_key(KeyEvent::new(code, KeyModifiers::empty()), "/net");
    }

    fn typed(f: &mut FormState, s: &str) {
        for c in s.chars() {
            press(f, KeyCode::Char(c));
        }
    }

    #[test]
    fn mount_point_follows_the_name_until_overridden() {
        let mut f = FormState::new("/net");
        typed(&mut f, "web-01");
        assert_eq!(f.mount.value(), "/net/web-01");

        // Typing in the mount field latches the override off.
        f.focus = FormField::Mount;
        typed(&mut f, "!");
        assert!(!f.mount_auto);
        let overridden = f.mount.value().to_string();

        // Further name edits no longer move it.
        f.focus = FormField::Name;
        typed(&mut f, "x");
        assert_eq!(f.mount.value(), overridden);
        assert_eq!(f.effective_mount("/net"), overridden);
    }

    #[test]
    fn clearing_the_mount_field_restores_the_automatic_value() {
        let mut f = FormState::new("/net");
        typed(&mut f, "nas");
        f.focus = FormField::Mount;
        typed(&mut f, "Z");
        assert!(!f.mount_auto);
        // Delete back through the override.
        for _ in 0..40 {
            press(&mut f, KeyCode::Backspace);
        }
        assert!(f.mount_auto, "an emptied mount field goes back to auto");
        assert_eq!(f.effective_mount("/net"), "/net/nas");
        // Backspacing past empty must not start eating the automatic value.
        press(&mut f, KeyCode::Backspace);
        assert_eq!(f.effective_mount("/net"), "/net/nas");
    }

    #[test]
    fn editing_an_existing_host_detects_whether_the_mount_was_customized() {
        let auto = HostRecord {
            name: "web-01".into(),
            mount_point: "/net/web-01".into(),
            ..Default::default()
        };
        assert!(FormState::from_record(&auto, "/net").mount_auto);

        let custom = HostRecord {
            name: "sandbox".into(),
            mount_point: "/srv/sandbox".into(),
            ..Default::default()
        };
        let f = FormState::from_record(&custom, "/net");
        assert!(!f.mount_auto);
        assert_eq!(f.effective_mount("/net"), "/srv/sandbox");
    }

    #[test]
    fn cycling_type_swaps_the_default_port_but_keeps_a_custom_one() {
        let mut f = FormState::new("/net");
        assert_eq!(f.proto, "SSH");
        assert_eq!(f.port.value(), "22");
        f.cycle_type(1);
        assert_eq!(f.proto, "FTP");
        assert_eq!(f.port.value(), "21");
        f.cycle_type(-1);
        assert_eq!(f.proto, "SSH");
        assert_eq!(f.port.value(), "22");

        f.port = Input::new("2222".to_string());
        f.cycle_type(1);
        assert_eq!(
            f.port.value(),
            "2222",
            "a hand-typed port survives a type change"
        );
    }

    #[test]
    fn the_port_field_rejects_non_digits() {
        let mut f = FormState::new("/net");
        f.focus = FormField::Port;
        f.port.reset();
        typed(&mut f, "2a2!2");
        assert_eq!(f.port.value(), "222");
    }

    fn ctrl(f: &mut FormState, c: char) {
        f.handle_key(
            KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL),
            "/net",
        );
    }

    /// The fields were append-only: typing appended, Backspace took the last
    /// character, and nothing reached the middle of a value. Fixing the third
    /// character of an address meant deleting everything after it.
    #[test]
    fn a_field_can_be_edited_anywhere_not_just_at_the_end() {
        let mut f = FormState::new("/net");
        f.focus = FormField::Addr;
        typed(&mut f, "10.0.0.1");

        press(&mut f, KeyCode::Home);
        typed(&mut f, "  ");
        assert_eq!(f.addr.value(), "  10.0.0.1", "Home reaches the front");

        press(&mut f, KeyCode::End);
        press(&mut f, KeyCode::Left);
        press(&mut f, KeyCode::Backspace);
        assert_eq!(
            f.addr.value(),
            "  10.0.01",
            "and the caret deletes beside it"
        );

        ctrl(&mut f, 'a');
        press(&mut f, KeyCode::Delete);
        press(&mut f, KeyCode::Delete);
        assert_eq!(f.addr.value(), "10.0.01", "^A and Delete work too");

        ctrl(&mut f, 'k');
        assert_eq!(f.addr.value(), "", "^K cuts to the end");
    }

    /// The rules that predate line editing hold for every way of changing the
    /// text, not just for typing at the end.
    #[test]
    fn the_field_rules_survive_caret_editing() {
        let mut f = FormState::new("/net");
        f.focus = FormField::Port;
        f.port.reset();
        typed(&mut f, "2222");
        press(&mut f, KeyCode::Home);
        typed(&mut f, "x9");
        assert_eq!(
            f.port.value(),
            "92222",
            "a non-digit is refused mid-value too"
        );

        // The mount latch releases however the field is emptied, including by
        // a word kill rather than a run of backspaces.
        let mut f = FormState::new("/net");
        typed(&mut f, "nas");
        f.focus = FormField::Mount;
        typed(&mut f, "Z");
        assert!(!f.mount_auto);
        ctrl(&mut f, 'u');
        assert!(f.mount_auto, "^U to empty hands it back to the host name");
        assert_eq!(f.effective_mount("/net"), "/net/nas");
    }

    /// Tab walks the inputs, then the buttons as they are drawn, and wraps.
    #[test]
    fn focus_wraps_in_both_directions() {
        let mut f = FormState::new("/net");
        assert_eq!(f.focus, FormField::Name);
        f.focus_prev();
        assert_eq!(
            f.focus,
            FormField::Cancel,
            "back from the first is the last"
        );
        f.focus_next();
        assert_eq!(f.focus, FormField::Name);
    }

    /// Every button is in the order; Bulk add only where it is drawn.
    #[test]
    fn the_buttons_follow_the_inputs_in_the_tab_order() {
        use FormField::*;
        let add = FormState::new("/net");
        assert_eq!(
            add.focus_order(),
            [
                Name, Type, Addr, Port, Mount, Login, Pass, KeyButton, BulkAdd, Save, Cancel
            ]
        );
        let mut edit = FormState::new("/net");
        edit.id = 7;
        assert_eq!(
            edit.focus_order(),
            [
                Name, Type, Addr, Port, Mount, Login, Pass, KeyButton, Save, Cancel
            ]
        );
    }

    #[test]
    fn validation_requires_a_name_and_address() {
        let mut f = FormState::new("/net");
        assert_eq!(f.validate().unwrap_err(), "Host name is required.");
        typed(&mut f, "web");
        assert_eq!(f.validate().unwrap_err(), "Address is required.");
        f.focus = FormField::Addr;
        typed(&mut f, "10.0.0.1");
        assert!(f.validate().is_ok());

        f.port = Input::new("99999".to_string());
        assert!(f.validate().is_err(), "a port outside u16 is rejected");
    }

    #[test]
    fn to_record_lowercases_the_protocol_and_fills_the_mount() {
        let mut f = FormState::new("/net");
        typed(&mut f, "Web 01");
        f.focus = FormField::Addr;
        typed(&mut f, " 10.0.0.1 ");
        let rec = f.to_record("/net");
        assert_eq!(rec.proto, "ssh", "qhostman stores the protocol lowercased");
        assert_eq!(rec.name, "Web 01");
        assert_eq!(rec.addr, "10.0.0.1", "address is trimmed");
        assert_eq!(rec.mount_point, "/net/web-01");
        assert_eq!(rec.port, 22);
    }

    #[test]
    fn key_fields_survive_a_form_roundtrip() {
        let rec = HostRecord {
            id: 7,
            name: "bastion".into(),
            addr: "edge".into(),
            key_name: "bastion".into(),
            key_value: "phrase".into(),
            proxy: true,
            ..Default::default()
        };
        let f = FormState::from_record(&rec, "/net");
        assert!(f.has_key());
        let back = f.to_record("/net");
        assert_eq!(back.key_name, "bastion");
        assert_eq!(back.key_value, "phrase");
        assert!(back.proxy);
        assert_eq!(back.id, 7);
    }
}
