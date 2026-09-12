//! Host-record writes the model may request.
//!
//! Write-only by construction. `HostFields` carries values *in*; nothing here
//! returns a field value, and no message built here names one. The model can
//! set a password and can never read one back — not the one it set, and not the
//! one it replaced. That is the same reason `list_hosts` returns names only:
//! the transcript must not become an inventory of somebody's infrastructure.
//!
//! The agent never touches the database. A tool validates a request and hands
//! it back as data; `crate::app` owns `DataBase` and is the only thing that
//! writes. Same shape as `Plan` — `crate::agent` describes, something outside
//! it performs.

use crate::db::model::{HOST_TYPES, HostRecord, default_port, mount_for};
use serde::{Deserialize, Deserializer};

/// Fields a tool may set. `None` means "leave it alone" on an edit and "take
/// the default" on a create, which is what makes a partial edit expressible
/// without the model having to know the value it is not changing.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostFields {
    pub name: Option<String>,
    /// Wire name `type`, to match what the schema shows the model.
    #[serde(rename = "type")]
    pub proto: Option<String>,
    #[serde(rename = "address")]
    pub addr: Option<String>,
    /// Accepts 22 and "22" alike: models send both, and a refusal the model has
    /// to guess its way out of costs a round trip.
    #[serde(default, deserialize_with = "lenient_port")]
    pub port: Option<i64>,
    pub login: Option<String>,
    #[serde(rename = "password")]
    pub pass: Option<String>,
    pub mount_point: Option<String>,
}

fn lenient_port<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    use serde::de::Error as _;
    match Option::<serde_json::Value>::deserialize(d)? {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| D::Error::custom("port must be a whole number")),
        Some(serde_json::Value::String(s)) => s
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| D::Error::custom("port must be a number")),
        Some(_) => Err(D::Error::custom("port must be a number")),
    }
}

impl HostFields {
    /// The names of the fields this request sets — never their values, so a
    /// receipt can say what changed without disclosing anything.
    pub fn names(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.name.is_some() {
            out.push("name");
        }
        if self.proto.is_some() {
            out.push("type");
        }
        if self.addr.is_some() {
            out.push("address");
        }
        if self.port.is_some() {
            out.push("port");
        }
        if self.login.is_some() {
            out.push("login");
        }
        if self.pass.is_some() {
            out.push("password");
        }
        if self.mount_point.is_some() {
            out.push("mount point");
        }
        out
    }

    fn is_empty(&self) -> bool {
        self.names().is_empty()
    }
}

/// A validated request to write one host record.
#[derive(Debug, Clone, PartialEq)]
pub enum HostWrite {
    Create(HostFields),
    /// `name` identifies the existing record; `fields` may rename it.
    Edit {
        name: String,
        fields: HostFields,
    },
}

impl HostWrite {
    /// The record to persist. `existing` is the live row for an edit — the app
    /// passes what the database holds now, not the turn's snapshot, so a write
    /// never silently reverts a field the operator changed mid-turn.
    pub fn apply(&self, existing: Option<&HostRecord>, mount_prefix: &str) -> HostRecord {
        let (mut rec, fields) = match self {
            HostWrite::Create(f) => (HostRecord::default(), f),
            HostWrite::Edit { fields, .. } => (existing.cloned().unwrap_or_default(), fields),
        };

        if let Some(v) = &fields.name {
            rec.name = v.trim().to_string();
        }
        if let Some(v) = &fields.proto {
            rec.proto = v.trim().to_lowercase();
        }
        if let Some(v) = &fields.addr {
            rec.addr = v.trim().to_string();
        }
        if let Some(v) = fields.port {
            rec.port = v;
        }
        if let Some(v) = &fields.login {
            rec.login = v.trim().to_string();
        }
        // Not trimmed: a password is whatever the operator was given.
        if let Some(v) = &fields.pass {
            rec.pass = v.clone();
        }
        if let Some(v) = &fields.mount_point {
            rec.mount_point = v.trim().to_string();
        }

        if matches!(self, HostWrite::Create(_)) {
            if rec.proto.is_empty() {
                rec.proto = HOST_TYPES[0].to_lowercase();
            }
            if fields.port.is_none() {
                rec.port = default_port(&rec.proto);
            }
            if rec.mount_point.is_empty() {
                rec.mount_point = mount_for(mount_prefix, &rec.name);
            }
        }
        rec
    }
}

/// Check a create request against the names already in use.
///
/// `taken` is every existing host name. Errors name the fix, not just the rule.
pub fn validate_create(fields: &HostFields, taken: &[&str]) -> Result<(), String> {
    if fields.is_empty() {
        return Err("a new host needs at least a name and an address.".to_string());
    }
    let name = fields.name.as_deref().unwrap_or("").trim();
    if name.is_empty() {
        return Err("name is required, and must not be blank.".to_string());
    }
    if taken.iter().any(|t| t.eq_ignore_ascii_case(name)) {
        // Deliberately does not repeat the name or confirm which one collided:
        // that would make this tool an oracle for "is this host in the
        // database?", which is the one thing a write-only tool must not become.
        return Err(
            "that name is already in use. Pick another, or use edit_host to change \
             the record that has it."
                .to_string(),
        );
    }
    if fields.addr.as_deref().unwrap_or("").trim().is_empty() {
        return Err("address is required, and must not be blank.".to_string());
    }
    check_common(fields)
}

/// Check an edit against the existing names. `target` is the host being edited,
/// so renaming it to its own name is not a collision.
pub fn validate_edit(fields: &HostFields, target: &str, taken: &[&str]) -> Result<(), String> {
    if fields.is_empty() {
        return Err(
            "nothing to change: give at least one field to set. The fields you may set are \
             name, type, address, port, login, password and mount_point."
                .to_string(),
        );
    }
    if let Some(new) = &fields.name {
        let new = new.trim();
        if new.is_empty() {
            return Err("name must not be blank.".to_string());
        }
        if !new.eq_ignore_ascii_case(target) && taken.iter().any(|t| t.eq_ignore_ascii_case(new)) {
            return Err("that new name is already in use by another host.".to_string());
        }
    }
    if let Some(addr) = &fields.addr
        && addr.trim().is_empty()
    {
        return Err("address must not be blank.".to_string());
    }
    check_common(fields)
}

/// The rules that do not depend on which names are in use.
fn check_common(fields: &HostFields) -> Result<(), String> {
    if let Some(proto) = &fields.proto {
        let p = proto.trim();
        if !HOST_TYPES.iter().any(|t| t.eq_ignore_ascii_case(p)) {
            return Err(format!(
                "{p:?} is not a host type. Use one of: {}.",
                HOST_TYPES.join(", ")
            ));
        }
    }
    if let Some(port) = fields.port
        && !(0..=65535).contains(&port)
    {
        return Err(format!("port {port} is out of range; it must be 0-65535."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn existing() -> HostRecord {
        HostRecord {
            id: 7,
            name: "web-01".into(),
            proto: "ssh".into(),
            addr: "10.0.4.11".into(),
            mount_point: "/net/web-01".into(),
            port: 22,
            login: "deploy".into(),
            pass: "hunter2".into(),
            key_name: "web-01".into(),
            key_value: "passphrase".into(),
            proxy: true,
            mounted: true,
        }
    }

    fn fields(json: serde_json::Value) -> HostFields {
        serde_json::from_value(json).expect("fields should parse")
    }

    /// The property that makes a partial edit expressible: everything the
    /// request leaves out keeps the value it had — including the fields no tool
    /// can set at all, which must survive a write they never mention.
    #[test]
    fn an_edit_changes_only_the_fields_it_names() {
        let before = existing();
        let write = HostWrite::Edit {
            name: "web-01".into(),
            fields: fields(serde_json::json!({"port": 2222})),
        };
        let after = write.apply(Some(&before), "/net");

        assert_eq!(after.port, 2222);
        // Untouched, field by field, because a silent revert here would be a
        // lost password or a broken key with nothing in the UI to show why.
        assert_eq!(after.id, before.id);
        assert_eq!(after.name, before.name);
        assert_eq!(after.addr, before.addr);
        assert_eq!(after.login, before.login);
        assert_eq!(after.pass, before.pass);
        assert_eq!(after.key_name, before.key_name);
        assert_eq!(after.key_value, before.key_value);
        assert_eq!(after.mount_point, before.mount_point);
        assert_eq!(after.proxy, before.proxy);
    }

    #[test]
    fn a_create_fills_in_the_defaults_the_form_would_have() {
        let write = HostWrite::Create(fields(
            serde_json::json!({"name": "Web 03", "address": "10.0.4.13"}),
        ));
        let rec = write.apply(None, "/net");
        assert_eq!(rec.id, 0, "id 0 is what makes DataBase::save insert");
        assert_eq!(rec.name, "Web 03");
        assert_eq!(rec.proto, "ssh");
        assert_eq!(rec.port, 22);
        assert_eq!(rec.mount_point, "/net/web-03");
        assert!(rec.pass.is_empty());
        assert!(rec.key_name.is_empty());
        assert!(!rec.proxy);
    }

    #[test]
    fn an_ftp_create_gets_the_ftp_port() {
        let write = HostWrite::Create(fields(
            serde_json::json!({"name": "files", "address": "10.0.9.2", "type": "FTP"}),
        ));
        let rec = write.apply(None, "/net");
        assert_eq!(rec.proto, "ftp");
        assert_eq!(rec.port, 21);
    }

    /// A password is whatever the operator was handed, so it is stored verbatim
    /// while the fields that become paths or arguments are trimmed.
    #[test]
    fn a_password_is_not_trimmed_but_the_other_fields_are() {
        let write = HostWrite::Create(fields(serde_json::json!({
            "name": "  pad  ", "address": " 10.0.0.1 ", "login": " root ", "password": " s p "
        })));
        let rec = write.apply(None, "/net");
        assert_eq!(rec.name, "pad");
        assert_eq!(rec.addr, "10.0.0.1");
        assert_eq!(rec.login, "root");
        assert_eq!(rec.pass, " s p ");
    }

    #[test]
    fn ports_arrive_as_numbers_or_strings() {
        assert_eq!(fields(serde_json::json!({"port": 2222})).port, Some(2222));
        assert_eq!(fields(serde_json::json!({"port": "2222"})).port, Some(2222));
        assert_eq!(fields(serde_json::json!({})).port, None);
        assert!(serde_json::from_value::<HostFields>(serde_json::json!({"port": "http"})).is_err());
    }

    /// A misspelled field must be a refusal, not a write that quietly does
    /// nothing — the operator would have no way to tell the difference.
    #[test]
    fn an_unknown_field_is_rejected_rather_than_ignored() {
        let err = serde_json::from_value::<HostFields>(
            serde_json::json!({"name": "x", "passwrod": "oops"}),
        )
        .expect_err("unknown field should not parse");
        assert!(err.to_string().contains("passwrod"), "{err}");
    }

    /// Keys and the proxy flag are the operator's alone: a key is a file only
    /// they can put in the data directory, and the proxy flag rewrites other
    /// records. Neither is expressible here, and that is checked rather than
    /// just documented.
    #[test]
    fn keys_and_the_proxy_flag_are_not_expressible() {
        for unsettable in ["key_name", "key_value", "proxy", "id", "mounted"] {
            let attempt = serde_json::from_value::<HostFields>(
                serde_json::json!({"name": "x", unsettable: "whatever"}),
            );
            assert!(attempt.is_err(), "{unsettable} should not be settable");
        }
    }

    #[test]
    fn a_create_needs_a_name_and_an_address() {
        assert!(validate_create(&fields(serde_json::json!({})), &[]).is_err());
        assert!(validate_create(&fields(serde_json::json!({"name": "a"})), &[]).is_err());
        assert!(validate_create(&fields(serde_json::json!({"address": "a"})), &[]).is_err());
        assert!(
            validate_create(
                &fields(serde_json::json!({"name": " ", "address": "a"})),
                &[]
            )
            .is_err()
        );
        assert!(
            validate_create(
                &fields(serde_json::json!({"name": "a", "address": "b"})),
                &[]
            )
            .is_ok()
        );
    }

    /// The refusal must not confirm which name collided: that would turn a
    /// write-only tool into an oracle for "is this host in the database?".
    #[test]
    fn a_duplicate_name_is_refused_without_repeating_it() {
        let err = validate_create(
            &fields(serde_json::json!({"name": "web-01", "address": "1.2.3.4"})),
            &["web-01", "db-main"],
        )
        .expect_err("duplicate should be refused");
        assert!(!err.contains("web-01"), "leaked the name: {err}");
        assert!(!err.contains("db-main"), "leaked another name: {err}");
        assert!(err.contains("already in use"), "{err}");
        // Case matters for the check, not for the collision.
        assert!(
            validate_create(
                &fields(serde_json::json!({"name": "WEB-01", "address": "1.2.3.4"})),
                &["web-01"],
            )
            .is_err()
        );
    }

    #[test]
    fn an_edit_must_name_at_least_one_field() {
        let err = validate_edit(&fields(serde_json::json!({})), "web-01", &["web-01"])
            .expect_err("an empty edit should be refused");
        // The refusal names the fix: which fields there are to set.
        assert!(err.contains("address"), "{err}");
        assert!(err.contains("password"), "{err}");
    }

    #[test]
    fn an_edit_may_rename_but_not_onto_another_host() {
        let taken = ["web-01", "db-main"];
        assert!(
            validate_edit(
                &fields(serde_json::json!({"name": "web-02"})),
                "web-01",
                &taken
            )
            .is_ok()
        );
        // Renaming to its own name is not a collision.
        assert!(
            validate_edit(
                &fields(serde_json::json!({"name": "web-01"})),
                "web-01",
                &taken
            )
            .is_ok()
        );
        let err = validate_edit(
            &fields(serde_json::json!({"name": "db-main"})),
            "web-01",
            &taken,
        )
        .expect_err("renaming onto another host should be refused");
        assert!(!err.contains("db-main"), "leaked the name: {err}");
    }

    #[test]
    fn the_type_and_port_are_checked_against_what_a_host_can_hold() {
        let bad_type = validate_create(
            &fields(serde_json::json!({"name": "a", "address": "b", "type": "telnet"})),
            &[],
        )
        .expect_err("telnet is not a host type");
        // Names the fix: the types that do work.
        assert!(
            bad_type.contains("SSH") && bad_type.contains("FTP"),
            "{bad_type}"
        );

        for port in [-1, 65536, 99999] {
            let err = validate_edit(
                &fields(serde_json::json!({"port": port})),
                "web-01",
                &["web-01"],
            )
            .expect_err("out of range port should be refused");
            assert!(err.contains("0-65535"), "{err}");
        }
        assert!(
            validate_edit(
                &fields(serde_json::json!({"port": 0})),
                "web-01",
                &["web-01"]
            )
            .is_ok()
        );
        assert!(
            validate_edit(
                &fields(serde_json::json!({"port": 65535})),
                "web-01",
                &["web-01"]
            )
            .is_ok()
        );
    }

    /// Blanking a login or password is a real intention; blanking a name or an
    /// address would leave a record nothing could use.
    #[test]
    fn a_field_may_be_cleared_unless_the_record_needs_it() {
        assert!(
            validate_edit(
                &fields(serde_json::json!({"password": ""})),
                "web-01",
                &["web-01"]
            )
            .is_ok()
        );
        assert!(
            validate_edit(
                &fields(serde_json::json!({"login": ""})),
                "web-01",
                &["web-01"]
            )
            .is_ok()
        );
        assert!(
            validate_edit(
                &fields(serde_json::json!({"address": ""})),
                "web-01",
                &["web-01"]
            )
            .is_err()
        );
        assert!(
            validate_edit(
                &fields(serde_json::json!({"name": ""})),
                "web-01",
                &["web-01"]
            )
            .is_err()
        );
    }

    #[test]
    fn the_receipt_lists_field_names_and_never_their_values() {
        let f = fields(serde_json::json!({
            "address": "10.9.9.9", "password": "hunter2", "port": 2222
        }));
        let names = f.names();
        assert_eq!(names, vec!["address", "port", "password"]);
        let joined = names.join(" ");
        for value in ["10.9.9.9", "hunter2", "2222"] {
            assert!(!joined.contains(value), "{value} leaked: {joined}");
        }
    }
}
