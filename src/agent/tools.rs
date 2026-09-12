//! The six tools, and the dispatch for the five that need no confirmation.
//!
//! `propose_plan` is the sixth and it executes nothing: it returns a `Plan`
//! for the UI to show. Nothing in this module can run a scriptlet or upload a
//! file — `ToolCtx` carries no executor, no channel and no handle that reaches
//! one, so the only outbound effects reachable from here are a validated
//! read-only command and reading the artifacts directory.
//!
//! `create_host` and `edit_host` write the operator's local host database, and
//! they reach it the same indirect way a plan reaches the executor: `ToolCtx`
//! holds no `DataBase` either. They validate a request and return it as data
//! for `crate::app` to perform. Both are write-only — they return a receipt and
//! never a field value, so no host detail can come back through them.

use super::exec::run_capture;
use super::hosts::{HostFields, HostWrite, validate_create, validate_edit};
use super::plan::{Plan, PlanRequest, resolve};
use super::proto::ToolDef;
use super::{artifacts, readonly};
use crate::config::Config;
// Note `HostRecord` is never serialised: it carries `pass` and `key_value`,
// and a `derive(Serialize)` on it would put both on the wire the first time
// anyone listed a host. Tools name their fields one at a time, or — as
// `list_hosts` now does — send nothing but the name.
use crate::db::model::{HOST_TYPES, HostRecord};
use crate::ssh;
use crate::ui::widgets::sanitize;
use serde::Deserialize;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// Everything a tool may touch. No executor, by construction.
pub struct ToolCtx<'a> {
    pub hosts: &'a [HostRecord],
    pub datadir: &'a Path,
    pub cfg: &'a Config,
    /// Raised when the operator cancels; the plan executor polls it between
    /// steps. None of the tools here run long enough to need it yet.
    #[allow(dead_code)]
    pub cancel: &'a AtomicBool,
    /// The id the next proposal receives.
    pub next_plan_id: u64,
    /// Host names written earlier in this same turn. `hosts` is a snapshot from
    /// when the turn began, so without this a host the model just created would
    /// look absent to `edit_host` and free to `create_host` — which matters
    /// when the operator pastes a list and the model enters it one call at a
    /// time. The app applies writes in the order they arrive, so an edit naming
    /// one of these resolves against a record that exists by then.
    pub written_this_turn: &'a [String],
}

/// What a dispatched tool produced.
pub enum ToolOutcome {
    /// Text to hand straight back as the tool result.
    Text(String),
    /// A proposal. The tool result is only a receipt; the plan goes to the UI
    /// and waits for a human.
    Proposal { receipt: String, plan: Box<Plan> },
    /// A validated host-record write. As with `Proposal`, the tool result is
    /// only a receipt: the write travels to `crate::app`, which owns the
    /// database. The receipt names the fields set, never their values.
    HostWrite {
        receipt: String,
        write: Box<HostWrite>,
    },
}

impl ToolOutcome {
    pub fn text(&self) -> &str {
        match self {
            ToolOutcome::Text(t) => t,
            ToolOutcome::Proposal { receipt, .. } => receipt,
            ToolOutcome::HostWrite { receipt, .. } => receipt,
        }
    }
}

pub const RUN_READONLY: &str = "run_readonly";
pub const LIST_HOSTS: &str = "list_hosts";
pub const LIST_ARTIFACTS: &str = "list_artifacts";
pub const PROPOSE_PLAN: &str = "propose_plan";
pub const CREATE_HOST: &str = "create_host";
pub const EDIT_HOST: &str = "edit_host";

/// The sentence both host tools carry, so the restriction is stated at the one
/// place the model reads at call time rather than only in the system prompt.
const ONLY_WHEN_ASKED: &str = "Use it ONLY when the operator has asked you, in so many \
    words, to add or change a host in the host database. Never on your own initiative, \
    never to tidy up what you found, and never as a side effect of some other task — if a \
    host record looks wrong to you, say so and let the operator decide.";

/// What these tools cannot reach, stated so the model does not spend a turn
/// discovering it.
const NOT_SETTABLE: &str = "It cannot set an SSH key or the SOCKS-proxy flag, and cannot \
    delete a host: the operator does those on the Hosts screen (F2 add, F3 edit, F7 \
    generate a key, F6 proxy, F8 delete).";

/// The schemas sent with every request.
pub fn definitions(cfg: &Config) -> Vec<ToolDef> {
    let allow = &cfg.agent.readonly_commands;
    vec![
        ToolDef::function(
            LIST_HOSTS,
            "List the machines this installation knows about, by name. A name is \
             the only identifier you need: it is what every other tool takes, and \
             OpenAdmin supplies the address, port, login and credentials itself. \
             They are not available to you, and you do not need them — never ask \
             the operator for them, and never write one into a script. The list is \
             a snapshot taken when the turn began; a host added mid-turn will not \
             appear.",
            serde_json::json!({"type": "object", "properties": {}}),
        ),
        ToolDef::function(
            RUN_READONLY,
            format!(
                "Run one read-only command on one host and return its output and exit \
                 status. Runs unattended, with no confirmation. Each call opens its own \
                 SSH connection and calls run one at a time, so it is seconds of the \
                 operator's time: send the command that answers the whole question \
                 rather than several that narrow it. It reads machine state — services, \
                 configuration, disks, logs, processes. It is not a file browser and not \
                 a code reader: diagnose a program from what it leaves behind — unit \
                 status, exit codes, logs, the files it writes — rather than paging \
                 through its source. {}",
                readonly::describe(allow)
            ),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "host": {"type": "string", "description": "Host name from list_hosts."},
                    "command": {"type": "string", "description": "Program name, no path."},
                    "args": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Arguments, one per element. Never a shell string."
                    }
                },
                "required": ["host", "command"]
            }),
        ),
        ToolDef::function(
            LIST_ARTIFACTS,
            "List files staged for upload. Use the exact name in an upload step.",
            serde_json::json!({"type": "object", "properties": {}}),
        ),
        ToolDef::function(
            PROPOSE_PLAN,
            "Propose changes for the operator to review. This does NOT run anything: it \
             shows the operator a dialog where they approve or reject each step and each \
             host. You cannot execute anything yourself and there is no tool that will. \
             Put everything the task needs into ONE plan — uploads and scripts, every \
             host — rather than proposing repeatedly; each proposal costs the operator a \
             decision, and a plan they can read end to end is one they can actually judge. \
             Scripts run non-interactively under `bash -s` and must never prompt.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Short description of the whole plan."},
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "summary": {"type": "string", "description": "One line: what this step does."},
                                "kind": {"type": "string", "enum": ["scriptlet", "upload"]},
                                "script": {"type": "string", "description": "For kind=scriptlet: the bash to run."},
                                "artifact": {"type": "string", "description": "For kind=upload: a name from list_artifacts."},
                                "hosts": {
                                    "type": "array",
                                    "items": {"type": "string"},
                                    "description": "Host names from list_hosts."
                                }
                            },
                            "required": ["kind", "hosts"]
                        }
                    }
                },
                "required": ["steps"]
            }),
        ),
        ToolDef::function(
            CREATE_HOST,
            format!(
                "Add one host to the operator's local host database, so it appears on the \
                 Hosts screen and can be used by name afterwards. {asked}\n\
                 \n\
                 Write-only: it returns a receipt naming the fields it set and nothing \
                 else. It cannot read a host back, and no tool can — so never call it to \
                 find out what a field currently holds. {cannot}\n\
                 \n\
                 Omitted fields take a default: type ssh, port 22 for ssh and 21 for ftp, \
                 and a mount point derived from the name. `name` and `address` are \
                 required, and `name` must not already be in use.",
                asked = ONLY_WHEN_ASKED,
                cannot = NOT_SETTABLE,
            ),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Short label for the machine, and the handle every \
                                        other tool takes. Must not already exist."
                    },
                    "type": {
                        "type": "string",
                        "enum": HOST_TYPES,
                        "description": "Protocol. Defaults to SSH."
                    },
                    "address": {
                        "type": "string",
                        "description": "Hostname or IP address. May be host:/remote/path, \
                                        which is the form sshfs mounts use."
                    },
                    "port": {
                        "type": "integer",
                        "description": "0-65535. Defaults to 22 for SSH, 21 for FTP."
                    },
                    "login": {"type": "string", "description": "Remote user name."},
                    "password": {
                        "type": "string",
                        "description": "Remote password. Omit it for a key-authenticated \
                                        host; you cannot attach a key here."
                    },
                    "mount_point": {
                        "type": "string",
                        "description": "Local path for the sshfs mount. Defaults to the \
                                        configured prefix plus the name."
                    }
                },
                "required": ["name", "address"]
            }),
        ),
        ToolDef::function(
            EDIT_HOST,
            format!(
                "Change fields on one host that is already in the operator's local host \
                 database. {asked}\n\
                 \n\
                 Write-only: it returns a receipt naming the fields it set and nothing \
                 else. It cannot read a field back — not even the one it is about to \
                 overwrite — and no tool can, so never call it to discover a current \
                 value. {cannot}\n\
                 \n\
                 Pass `host` plus only the fields you are changing; every field you leave \
                 out keeps the value it has. Setting `name` renames the host, which \
                 changes the handle the other tools take; it does not re-derive the mount \
                 point, so set `mount_point` too if that should follow.",
                asked = ONLY_WHEN_ASKED,
                cannot = NOT_SETTABLE,
            ),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "host": {
                        "type": "string",
                        "description": "Name of the existing host to change, from list_hosts."
                    },
                    "name": {"type": "string", "description": "New name. Renames the host."},
                    "type": {"type": "string", "enum": HOST_TYPES, "description": "Protocol."},
                    "address": {
                        "type": "string",
                        "description": "Hostname or IP address, or host:/remote/path."
                    },
                    "port": {"type": "integer", "description": "0-65535."},
                    "login": {"type": "string", "description": "Remote user name."},
                    "password": {"type": "string", "description": "Remote password."},
                    "mount_point": {
                        "type": "string",
                        "description": "Local path for the sshfs mount."
                    }
                },
                "required": ["host"]
            }),
        ),
    ]
}

#[derive(Deserialize)]
struct ReadonlyArgs {
    host: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
}

/// Run one tool. A refusal or a failure is a *result*, not an error — the model
/// should see why and correct itself rather than the turn collapsing.
pub fn dispatch(ctx: &ToolCtx, name: &str, arguments: &str) -> ToolOutcome {
    match name {
        // Names and nothing else. A name is the whole of what the model needs:
        // it is the handle every other tool takes, and OpenAdmin fills in the
        // address, port, login and credentials itself when it runs something.
        // Anything more would be detail the model cannot use and an operator's
        // infrastructure on somebody else's server.
        LIST_HOSTS => {
            let names: Vec<&str> = ctx.hosts.iter().map(|h| h.name.as_str()).collect();
            if names.is_empty() {
                return ToolOutcome::Text(
                    "No hosts are configured. The operator adds them on the Hosts screen."
                        .to_string(),
                );
            }
            ToolOutcome::Text(
                serde_json::to_string(&names)
                    .unwrap_or_else(|e| format!("could not list hosts: {e}")),
            )
        }
        LIST_ARTIFACTS => match artifacts::list(ctx.datadir) {
            Ok(list) if list.is_empty() => ToolOutcome::Text(
                "No artifacts staged. The operator puts files in ~/.openadmin/artifacts/."
                    .to_string(),
            ),
            Ok(list) => ToolOutcome::Text(
                serde_json::to_string_pretty(&serde_json::json!(
                    list.iter()
                        .map(|a| serde_json::json!({"name": a.name, "bytes": a.size}))
                        .collect::<Vec<_>>()
                ))
                .unwrap_or_default(),
            ),
            Err(e) => ToolOutcome::Text(format!("could not list artifacts: {e}")),
        },
        RUN_READONLY => ToolOutcome::Text(run_readonly(ctx, arguments)),
        PROPOSE_PLAN => propose(ctx, arguments),
        CREATE_HOST => create_host(ctx, arguments),
        EDIT_HOST => edit_host(ctx, arguments),
        other => ToolOutcome::Text(format!(
            "there is no tool called {other:?}. Available: {LIST_HOSTS}, {RUN_READONLY}, \
             {LIST_ARTIFACTS}, {PROPOSE_PLAN}, {CREATE_HOST}, {EDIT_HOST}."
        )),
    }
}

fn run_readonly(ctx: &ToolCtx, arguments: &str) -> String {
    let args: ReadonlyArgs = match serde_json::from_str(arguments) {
        Ok(a) => a,
        Err(e) => return format!("could not read the arguments: {e}"),
    };
    let Some(host) = ctx.hosts.iter().find(|h| h.name == args.host) else {
        return format!(
            "unknown host {:?}. Known hosts: {}.",
            args.host,
            ctx.hosts
                .iter()
                .map(|h| h.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    };
    if let Err(e) = readonly::validate(&args.command, &args.args, &ctx.cfg.agent.readonly_commands)
    {
        return format!("refused: {e}");
    }

    let remote = ssh::quote_command(&args.command, &args.args);
    let proxy = ctx.hosts.iter().find(|h| h.proxy);
    let launch = ssh::exec_command(host, ctx.datadir, ctx.cfg, &remote, ssh::Stdin::Null, proxy);
    match run_capture(
        &launch,
        None,
        Duration::from_secs(ctx.cfg.agent.command_timeout_secs),
        ctx.cfg.agent.output_cap_bytes,
        None,
    ) {
        // Sanitized here so escape sequences reach neither the transcript nor
        // the model's context, where they are only noise.
        Ok(c) => sanitize_block(&c.summarize()),
        Err(e) => format!("could not run the command: {e}"),
    }
}

/// Sanitize a multi-line block, keeping the line structure.
fn sanitize_block(text: &str) -> String {
    text.lines().map(sanitize).collect::<Vec<_>>().join("\n")
}

fn propose(ctx: &ToolCtx, arguments: &str) -> ToolOutcome {
    let req: PlanRequest = match serde_json::from_str(arguments) {
        Ok(r) => r,
        Err(e) => return ToolOutcome::Text(format!("could not read the plan: {e}")),
    };
    let known: Vec<(i64, String)> = ctx.hosts.iter().map(|h| (h.id, h.name.clone())).collect();
    match resolve(ctx.next_plan_id, req, &known) {
        Ok(plan) => {
            let receipt = format!(
                "Plan #{} recorded: {} step(s) across {} host(s). It is now waiting for the \
                 operator to approve or reject it, step by step and host by host. Nothing \
                 has run. Wait for the execution report before proposing anything else.",
                plan.id,
                plan.steps.len(),
                plan.host_count()
            );
            ToolOutcome::Proposal {
                receipt,
                plan: Box::new(plan),
            }
        }
        Err(e) => ToolOutcome::Text(format!("the plan was not accepted: {e}")),
    }
}

/// Every host name the model may refer to: the turn's snapshot plus anything
/// written since it began.
fn known_names<'a>(ctx: &'a ToolCtx<'a>) -> Vec<&'a str> {
    ctx.hosts
        .iter()
        .map(|h| h.name.as_str())
        .chain(ctx.written_this_turn.iter().map(String::as_str))
        .collect()
}

/// Join field names for a receipt. Names only: a receipt never carries a value,
/// which is what keeps both host tools write-only.
fn field_list(names: &[&str]) -> String {
    match names {
        [] => "nothing".to_string(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn create_host(ctx: &ToolCtx, arguments: &str) -> ToolOutcome {
    let fields: HostFields = match serde_json::from_str(arguments) {
        Ok(f) => f,
        Err(e) => return ToolOutcome::Text(format!("could not read the arguments: {e}")),
    };
    let taken = known_names(ctx);
    if let Err(e) = validate_create(&fields, &taken) {
        return ToolOutcome::Text(format!("refused: {e}"));
    }
    // validate_create rejects a missing or blank name, so this is present.
    let name = fields
        .name
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_string();
    let receipt = format!(
        "Created host {name:?}, setting {}. It is on the operator's Hosts screen now; \
         use {name:?} as the host name from here on.",
        field_list(&fields.names())
    );
    ToolOutcome::HostWrite {
        receipt,
        write: Box::new(HostWrite::Create(fields)),
    }
}

fn edit_host(ctx: &ToolCtx, arguments: &str) -> ToolOutcome {
    let mut value: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => return ToolOutcome::Text(format!("could not read the arguments: {e}")),
    };
    let Some(obj) = value.as_object_mut() else {
        return ToolOutcome::Text("could not read the arguments: expected an object.".to_string());
    };
    // `host` names the record; the rest are the fields to set. Taking it out
    // first lets `HostFields` keep rejecting unknown keys, so a misspelled
    // field name is a refusal rather than a change that silently does nothing.
    let host = match obj.remove("host") {
        Some(serde_json::Value::String(s)) => s,
        Some(_) => {
            return ToolOutcome::Text(
                "could not read the arguments: host must be a string.".to_string(),
            );
        }
        None => {
            return ToolOutcome::Text(format!(
                "could not read the arguments: host is required — the name of the host to \
                 change, from {LIST_HOSTS}."
            ));
        }
    };
    let fields: HostFields = match serde_json::from_value(value) {
        Ok(f) => f,
        Err(e) => return ToolOutcome::Text(format!("could not read the arguments: {e}")),
    };
    // `any`, not `find`: nothing here needs to look at the record, and not
    // binding it is what makes "this tool reads no field" plain to read.
    let taken = known_names(ctx);
    if !taken.iter().any(|n| *n == host) {
        // No name, and no list: the point of a write-only tool is that you
        // cannot learn from it which hosts exist.
        return ToolOutcome::Text(format!(
            "refused: no host by that name. This tool changes a record that already \
             exists; {CREATE_HOST} adds a new one."
        ));
    }
    if let Err(e) = validate_edit(&fields, &host, &taken) {
        return ToolOutcome::Text(format!("refused: {e}"));
    }
    let receipt = format!(
        "Updated host {host:?}, setting {}. Every other field is unchanged.",
        field_list(&fields.names())
    );
    ToolOutcome::HostWrite {
        receipt,
        write: Box::new(HostWrite::Edit { name: host, fields }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts() -> Vec<HostRecord> {
        vec![
            HostRecord {
                id: 1,
                name: "web-01".into(),
                proto: "ssh".into(),
                addr: "10.0.4.11".into(),
                port: 22,
                login: "deploy".into(),
                pass: "hunter2".into(),
                key_name: "web-01".into(),
                key_value: "passphrase".into(),
                mount_point: "/net/web-01".into(),
                ..Default::default()
            },
            HostRecord {
                id: 2,
                name: "db-main".into(),
                proto: "ssh".into(),
                addr: "10.0.8.3".into(),
                port: 22,
                login: "postgres".into(),
                pass: "s3cret".into(),
                ..Default::default()
            },
        ]
    }

    fn ctx<'a>(
        hosts: &'a [HostRecord],
        cfg: &'a Config,
        dir: &'a Path,
        cancel: &'a AtomicBool,
    ) -> ToolCtx<'a> {
        ToolCtx {
            hosts,
            datadir: dir,
            cfg,
            cancel,
            next_plan_id: 1,
            written_this_turn: &[],
        }
    }

    /// The single most important property of `list_hosts`: a name is all it
    /// gives, so there is no field a future edit could widen into a leak.
    #[test]
    fn listing_hosts_gives_names_and_nothing_else() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(&ctx(&h, &cfg, Path::new("/tmp"), &cancel), LIST_HOSTS, "{}");
        let text = out.text();
        assert_eq!(text, r#"["web-01","db-main"]"#, "{text}");
        for secret in ["hunter2", "passphrase", "s3cret"] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
        // Not secrets, but not the model's business either — and an address
        // is the part that turns a transcript into somebody's inventory.
        for detail in ["10.0.4.11", "10.0.8.3", "deploy", "postgres", "/net/"] {
            assert!(!text.contains(detail), "{detail} leaked: {text}");
        }
    }

    #[test]
    fn with_no_hosts_it_says_so_rather_than_returning_an_empty_list() {
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(
            &ctx(&[], &cfg, Path::new("/tmp"), &cancel),
            LIST_HOSTS,
            "{}",
        );
        assert!(
            out.text().contains("No hosts are configured"),
            "{}",
            out.text()
        );
    }

    #[test]
    fn an_unknown_tool_says_what_exists() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(&ctx(&h, &cfg, Path::new("/tmp"), &cancel), "rm_rf", "{}");
        assert!(out.text().contains("no tool called"), "{}", out.text());
        assert!(out.text().contains(PROPOSE_PLAN));
    }

    #[test]
    fn a_refused_command_explains_itself_rather_than_failing_the_turn() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let c = ctx(&h, &cfg, Path::new("/tmp"), &cancel);

        let out = dispatch(
            &c,
            RUN_READONLY,
            r#"{"host":"web-01","command":"rm","args":["-rf","/"]}"#,
        );
        assert!(out.text().starts_with("refused:"), "{}", out.text());

        let out = dispatch(
            &c,
            RUN_READONLY,
            r#"{"host":"nowhere","command":"ls","args":[]}"#,
        );
        assert!(out.text().contains("unknown host"), "{}", out.text());

        let out = dispatch(&c, RUN_READONLY, "not json");
        assert!(out.text().contains("could not read"), "{}", out.text());
    }

    #[test]
    fn a_proposal_is_recorded_and_nothing_runs() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(
            &ctx(&h, &cfg, Path::new("/tmp"), &cancel),
            PROPOSE_PLAN,
            r#"{"title":"fix","steps":[
                {"summary":"restart","kind":"scriptlet","script":"systemctl restart nginx",
                 "hosts":["web-01","db-main"]}]}"#,
        );
        match out {
            ToolOutcome::Proposal { receipt, plan } => {
                assert_eq!(plan.id, 1);
                assert_eq!(plan.steps[0].hosts, vec![1, 2]);
                assert!(receipt.contains("Nothing has run"), "{receipt}");
                assert!(receipt.contains("waiting for the operator"), "{receipt}");
            }
            other => panic!("expected a proposal, got: {}", other.text()),
        }
    }

    #[test]
    fn a_bad_proposal_comes_back_as_advice() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(
            &ctx(&h, &cfg, Path::new("/tmp"), &cancel),
            PROPOSE_PLAN,
            r#"{"steps":[{"kind":"scriptlet","script":"id","hosts":["ghost"]}]}"#,
        );
        assert!(matches!(out, ToolOutcome::Text(_)));
        assert!(out.text().contains("unknown host"), "{}", out.text());
    }

    #[test]
    fn the_tool_schemas_name_the_boundary() {
        let cfg = Config::default();
        let defs = definitions(&cfg);
        assert_eq!(defs.len(), 6);
        let plan = defs
            .iter()
            .find(|d| d.function.name == PROPOSE_PLAN)
            .unwrap();
        // The model is told the gate is structural, so it does not waste turns.
        assert!(plan.function.description.contains("does NOT run anything"));
        assert!(plan.function.description.contains("no tool that will"));
        // And that one big plan is wanted.
        assert!(plan.function.description.contains("ONE plan"));

        let ro = defs
            .iter()
            .find(|d| d.function.name == RUN_READONLY)
            .unwrap();
        assert!(ro.function.description.contains("no confirmation"));
        // The cost, and what it is not for, stated where the call is
        // constructed rather than only in the system prompt.
        assert!(ro.function.description.contains("own SSH connection"));
        assert!(ro.function.description.contains("not a code reader"));
        // The cost and the two things it is not for are stated where the call
        // is constructed, not only in the system prompt.
        assert!(ro.function.description.contains("own SSH connection"));
        assert!(ro.function.description.contains("not a code reader"));
        // The schema carries the whole grammar, not a summary of it: this is
        // the copy the model reads while filling in the arguments, and every
        // gap in it is a refusal and a round trip.
        let d = &ro.function.description;
        assert!(d.contains("exhaustive"), "{d}");
        assert!(d.contains("first word one of: status show cat"), "{d}");
        assert!(d.contains("-maxdepth"), "{d}");
    }

    #[test]
    fn artifacts_are_listed_by_name_and_size() {
        let dir = std::env::temp_dir().join(format!("openadmin-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = artifacts::ensure_dir(&dir).unwrap();
        std::fs::write(a.join("hotfix.sh"), "#!/bin/sh\n").unwrap();

        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(&ctx(&h, &cfg, &dir, &cancel), LIST_ARTIFACTS, "{}");
        assert!(out.text().contains("hotfix.sh"), "{}", out.text());
        assert!(out.text().contains("10"), "size reported: {}", out.text());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_artifacts_says_where_to_put_them() {
        let dir = std::env::temp_dir().join(format!("openadmin-tools-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(&ctx(&h, &cfg, &dir, &cancel), LIST_ARTIFACTS, "{}");
        assert!(out.text().contains("artifacts/"), "{}", out.text());
    }
    /// Every value the fixture hosts hold. No host tool may put any of these
    /// into its result, because the result is the model's context.
    const EXISTING_DETAIL: [&str; 7] = [
        "10.0.4.11",
        "10.0.8.3",
        "deploy",
        "postgres",
        "hunter2",
        "s3cret",
        "passphrase",
    ];

    #[test]
    fn creating_a_host_returns_a_receipt_and_asks_the_app_to_write_it() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(
            &ctx(&h, &cfg, Path::new("/tmp"), &cancel),
            CREATE_HOST,
            r#"{"name":"web-03","address":"10.0.4.13","login":"deploy","password":"n3w"}"#,
        );
        let text = out.text().to_string();
        // The write is data for `crate::app`: the tool cannot reach the database.
        match out {
            ToolOutcome::HostWrite { write, .. } => match *write {
                crate::agent::hosts::HostWrite::Create(f) => {
                    assert_eq!(f.name.as_deref(), Some("web-03"));
                    assert_eq!(f.pass.as_deref(), Some("n3w"));
                }
                other => panic!("expected a create, got {other:?}"),
            },
            other => panic!("expected a host write, got: {}", other.text()),
        }
        assert!(text.contains("web-03"), "{text}");
        assert!(
            text.contains("password"),
            "the receipt names the fields set: {text}"
        );
        // Names the fields, never the values.
        assert!(!text.contains("n3w"), "the new password leaked: {text}");
        assert!(
            !text.contains("10.0.4.13"),
            "the new address leaked: {text}"
        );
    }

    /// The write-only property, as a test: an edit cannot disclose what it is
    /// about to overwrite, nor anything else the database holds.
    #[test]
    fn editing_a_host_discloses_nothing_it_overwrote() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(
            &ctx(&h, &cfg, Path::new("/tmp"), &cancel),
            EDIT_HOST,
            r#"{"host":"web-01","password":"rotated","port":2222}"#,
        );
        let text = out.text().to_string();
        assert!(matches!(out, ToolOutcome::HostWrite { .. }), "{text}");
        assert!(text.contains("web-01"), "{text}");
        assert!(text.contains("port") && text.contains("password"), "{text}");
        for detail in EXISTING_DETAIL {
            assert!(!text.contains(detail), "{detail} leaked: {text}");
        }
        // Not even the values it was just handed.
        assert!(!text.contains("rotated"), "the new password leaked: {text}");
        assert!(!text.contains("2222"), "the new port leaked: {text}");
    }

    /// A refusal must not become a way to ask "does this host exist?".
    #[test]
    fn a_missing_host_is_refused_without_saying_which_hosts_exist() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(
            &ctx(&h, &cfg, Path::new("/tmp"), &cancel),
            EDIT_HOST,
            r#"{"host":"web-99","port":22}"#,
        );
        let text = out.text();
        assert!(text.starts_with("refused:"), "{text}");
        for name in ["web-01", "db-main"] {
            assert!(!text.contains(name), "{name} leaked: {text}");
        }
        for detail in EXISTING_DETAIL {
            assert!(!text.contains(detail), "{detail} leaked: {text}");
        }
    }

    #[test]
    fn a_duplicate_create_is_refused_without_confirming_the_name() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(
            &ctx(&h, &cfg, Path::new("/tmp"), &cancel),
            CREATE_HOST,
            r#"{"name":"web-01","address":"1.2.3.4"}"#,
        );
        let text = out.text();
        assert!(text.starts_with("refused:"), "{text}");
        assert!(
            !text.contains("web-01"),
            "confirmed the name exists: {text}"
        );
    }

    /// The operator pastes a list and the model enters it one call at a time.
    /// `hosts` is the turn's snapshot, so without `written_this_turn` the second
    /// call would both be allowed to duplicate a name and unable to edit it.
    #[test]
    fn a_host_written_earlier_in_the_turn_is_already_known() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let written = vec!["web-03".to_string()];
        let c = ToolCtx {
            hosts: &h,
            datadir: Path::new("/tmp"),
            cfg: &cfg,
            cancel: &cancel,
            next_plan_id: 1,
            written_this_turn: &written,
        };
        // Editable, because the app will have applied the create by then.
        let edit = dispatch(&c, EDIT_HOST, r#"{"host":"web-03","port":2022}"#);
        assert!(
            matches!(edit, ToolOutcome::HostWrite { .. }),
            "{}",
            edit.text()
        );
        // And not creatable twice.
        let again = dispatch(&c, CREATE_HOST, r#"{"name":"web-03","address":"1.2.3.4"}"#);
        assert!(again.text().starts_with("refused:"), "{}", again.text());
    }

    #[test]
    fn a_misspelled_field_is_refused_rather_than_silently_dropped() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(
            &ctx(&h, &cfg, Path::new("/tmp"), &cancel),
            EDIT_HOST,
            r#"{"host":"web-01","passwrod":"oops"}"#,
        );
        assert!(
            out.text().starts_with("could not read the arguments"),
            "{}",
            out.text()
        );
        assert!(out.text().contains("passwrod"), "{}", out.text());
    }

    #[test]
    fn the_host_tool_schemas_state_the_restriction_and_the_write_only_property() {
        let cfg = Config::default();
        let defs = definitions(&cfg);
        for name in [CREATE_HOST, EDIT_HOST] {
            let d = defs.iter().find(|d| d.function.name == name).unwrap();
            let desc = &d.function.description;
            // Only on an explicit request, stated where it is read at call time.
            assert!(
                desc.contains("ONLY when the operator has asked"),
                "{name}: {desc}"
            );
            assert!(
                desc.contains("Never on your own initiative"),
                "{name}: {desc}"
            );
            // Write-only, so the model does not try to read through it.
            assert!(desc.contains("Write-only"), "{name}: {desc}");
            assert!(desc.contains("cannot read"), "{name}: {desc}");
            // And what it cannot reach, so that is not learned by refusal.
            assert!(desc.contains("SSH key"), "{name}: {desc}");
            assert!(
                desc.contains("cannot \ndelete") || desc.contains("cannot delete"),
                "{name}: {desc}"
            );
        }
    }
}
