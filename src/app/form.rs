//! Add/Edit Host form state (`design/ui_kits/openadmin/HostDialogs.jsx`).
//!
//! Modeled on `/root/cfdns/src/app/form.rs`: an explicit field enum drives both
//! the tab order and the renderer, input is filtered per field, and `validate`
//! returns the exact message the status bar flashes while the dialog stays
//! open.

use crate::db::model::{HOST_TYPES, HostRecord, default_port, mount_for};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Name,
    Type,
    Addr,
    Port,
    Mount,
    Login,
    Pass,
}

/// Tab order, matching `FORM_FIELDS` in HostDialogs.jsx:15.
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
    pub name: String,
    pub proto: String,
    pub addr: String,
    pub port: String,
    pub mount: String,
    /// While true the mount point tracks the host name. Cleared the moment the
    /// user types in the mount field, restored when they empty it.
    pub mount_auto: bool,
    pub login: String,
    pub pass: String,
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
            name: String::new(),
            proto: HOST_TYPES[0].to_string(),
            addr: String::new(),
            port: default_port(HOST_TYPES[0]).to_string(),
            mount: String::new(),
            mount_auto: true,
            login: String::new(),
            pass: String::new(),
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
            name: rec.name.clone(),
            proto: rec.proto.to_ascii_uppercase(),
            addr: rec.addr.clone(),
            port: rec.port.to_string(),
            mount: rec.mount_point.clone(),
            mount_auto: auto,
            login: rec.login.clone(),
            pass: rec.pass.clone(),
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

    pub fn focus_next(&mut self) {
        let i = FORM_FIELDS
            .iter()
            .position(|f| *f == self.focus)
            .unwrap_or(0);
        self.focus = FORM_FIELDS[(i + 1) % FORM_FIELDS.len()];
    }

    pub fn focus_prev(&mut self) {
        let i = FORM_FIELDS
            .iter()
            .position(|f| *f == self.focus)
            .unwrap_or(0);
        self.focus = FORM_FIELDS[(i + FORM_FIELDS.len() - 1) % FORM_FIELDS.len()];
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
        if self.port.is_empty() || self.port == old_default {
            self.port = default_port(&self.proto).to_string();
        }
    }

    /// The mount point shown for the current name, honoring the override latch.
    pub fn effective_mount(&self, prefix: &str) -> String {
        if self.mount_auto {
            mount_for(prefix, &self.name)
        } else {
            self.mount.clone()
        }
    }

    pub fn type_char(&mut self, c: char, prefix: &str) {
        match self.focus {
            FormField::Name => {
                self.name.push(c);
                self.sync_mount(prefix);
            }
            FormField::Type => {}
            FormField::Addr => self.addr.push(c),
            // The port field only accepts digits.
            FormField::Port => {
                if c.is_ascii_digit() {
                    self.port.push(c);
                }
            }
            FormField::Mount => {
                self.mount.push(c);
                self.mount_auto = false;
            }
            FormField::Login => self.login.push(c),
            FormField::Pass => self.pass.push(c),
        }
    }

    pub fn backspace(&mut self, prefix: &str) {
        match self.focus {
            FormField::Name => {
                self.name.pop();
                self.sync_mount(prefix);
            }
            FormField::Type => {}
            FormField::Addr => {
                self.addr.pop();
            }
            FormField::Port => {
                self.port.pop();
            }
            FormField::Mount => {
                self.mount.pop();
                // Emptying the field hands control back to the host name. The
                // buffer stays empty; `effective_mount` supplies the display,
                // so further backspaces cannot chew into the automatic value.
                self.mount_auto = self.mount.trim().is_empty();
            }
            FormField::Login => {
                self.login.pop();
            }
            FormField::Pass => {
                self.pass.pop();
            }
        }
    }

    fn sync_mount(&mut self, prefix: &str) {
        if self.mount_auto {
            self.mount = mount_for(prefix, &self.name);
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Host name is required.".to_string());
        }
        if self.addr.trim().is_empty() {
            return Err("Address is required.".to_string());
        }
        if !self.port.is_empty() && self.port.parse::<u16>().is_err() {
            return Err("Port must be a number between 0 and 65535.".to_string());
        }
        Ok(())
    }

    pub fn to_record(&self, prefix: &str) -> HostRecord {
        let mount = {
            let m = self.effective_mount(prefix);
            if m.trim().is_empty() {
                mount_for(prefix, &self.name)
            } else {
                m
            }
        };
        HostRecord {
            id: self.id,
            name: self.name.trim().to_string(),
            // qhostman stores the protocol lowercased and displays it upper.
            proto: self.proto.to_lowercase(),
            addr: self.addr.trim().to_string(),
            mount_point: mount,
            port: self
                .port
                .parse::<i64>()
                .unwrap_or_else(|_| default_port(&self.proto)),
            login: self.login.clone(),
            pass: self.pass.clone(),
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

    fn typed(f: &mut FormState, s: &str) {
        for c in s.chars() {
            f.type_char(c, "/net");
        }
    }

    #[test]
    fn mount_point_follows_the_name_until_overridden() {
        let mut f = FormState::new("/net");
        typed(&mut f, "web-01");
        assert_eq!(f.mount, "/net/web-01");

        // Typing in the mount field latches the override off.
        f.focus = FormField::Mount;
        typed(&mut f, "!");
        assert!(!f.mount_auto);
        let overridden = f.mount.clone();

        // Further name edits no longer move it.
        f.focus = FormField::Name;
        typed(&mut f, "x");
        assert_eq!(f.mount, overridden);
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
            f.backspace("/net");
        }
        assert!(f.mount_auto, "an emptied mount field goes back to auto");
        assert_eq!(f.effective_mount("/net"), "/net/nas");
        // Backspacing past empty must not start eating the automatic value.
        f.backspace("/net");
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
        assert_eq!(f.port, "22");
        f.cycle_type(1);
        assert_eq!(f.proto, "FTP");
        assert_eq!(f.port, "21");
        f.cycle_type(-1);
        assert_eq!(f.proto, "SSH");
        assert_eq!(f.port, "22");

        f.port = "2222".to_string();
        f.cycle_type(1);
        assert_eq!(f.port, "2222", "a hand-typed port survives a type change");
    }

    #[test]
    fn the_port_field_rejects_non_digits() {
        let mut f = FormState::new("/net");
        f.focus = FormField::Port;
        f.port.clear();
        typed(&mut f, "2a2!2");
        assert_eq!(f.port, "222");
    }

    #[test]
    fn focus_wraps_in_both_directions() {
        let mut f = FormState::new("/net");
        assert_eq!(f.focus, FormField::Name);
        f.focus_prev();
        assert_eq!(f.focus, FormField::Pass);
        f.focus_next();
        assert_eq!(f.focus, FormField::Name);
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

        f.port = "99999".to_string();
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
