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
use super::plan::{Plan, PlanRequest, StepKind, hosts_available, resolve};
use super::proto::ToolDef;
use super::{artifacts, manuals, probe, readonly};
use crate::config::Config;
// Note `HostRecord` is never serialised: it carries `pass` and `key_value`,
// and a `derive(Serialize)` on it would put both on the wire the first time
// anyone listed a host. Tools name their fields one at a time, or — as
// `list_hosts` now does — send nothing but the name.
use crate::db::model::{HOST_TYPES, HostRecord};
use crate::ssh;
use crate::ui::widgets::sanitize;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// Everything a tool may touch. No executor, by construction.
pub struct ToolCtx<'a> {
    /// The hosts the model may see and act on — the operator's filter applied.
    pub hosts: &'a [HostRecord],
    /// Whether a filter narrowed `hosts`, so the answers can say so.
    pub filtered: bool,
    /// The proxy host, from all hosts: not looked up in `hosts`, where a filter
    /// could hide it.
    pub proxy: Option<&'a HostRecord>,
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
    /// Text for the model, and a line for the operator.
    ///
    /// The inverse of `Proposal` and `HostWrite`, where the receipt is what the
    /// *model* gets. A fetched manual is the one result the operator does not
    /// need to see: they wrote it, and the transcript renders every line of a
    /// tool result and keeps it, so sixteen kilobytes of their own prose would
    /// bury the conversation it is part of.
    Loaded { text: String, receipt: String },
}

impl ToolOutcome {
    /// What the model is told.
    pub fn text(&self) -> &str {
        match self {
            ToolOutcome::Text(t) => t,
            ToolOutcome::Proposal { receipt, .. } => receipt,
            ToolOutcome::HostWrite { receipt, .. } => receipt,
            ToolOutcome::Loaded { text, .. } => text,
        }
    }

    /// What the operator sees in the transcript. The same thing, unless the two
    /// audiences want different lengths of it.
    pub fn transcript(&self) -> &str {
        match self {
            ToolOutcome::Loaded { receipt, .. } => receipt,
            other => other.text(),
        }
    }
}

pub const LIST_HOSTS: &str = "list_hosts";
pub const LIST_ARTIFACTS: &str = "list_artifacts";
pub const LIST_MANUALS: &str = "list_manuals";
pub const FETCH_MANUAL: &str = "fetch_manual";
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
    let mut defs = vec![
        ToolDef::function(
            LIST_HOSTS,
            "List the machines this installation knows about, by name. A name is \
             the only identifier you need: it is what every other tool takes, and \
             OpenAdmin supplies the address, port, login and credentials itself. \
             They are not available to you, and you do not need them — never ask \
             the operator for them, and never write one into a script. The list is \
             a snapshot taken when the turn began: a host added, or a filter \
             changed, mid-turn takes effect with the operator's next message.\n\
             \n\
             The operator can narrow this list with a filter on the Hosts screen. \
             When they have, the result says so, and only the hosts it names are \
             available to any tool: others may exist, and naming one gets the same \
             unknown-host answer as a name that does not. What the filter matches is \
             not given to you, and you do not need it. If the task needs a host that \
             is not listed, ask the operator to clear the filter (Esc on the Hosts \
             screen) — do not guess names.",
            serde_json::json!({"type": "object", "properties": {}}),
        ),
        ToolDef::function(
            LIST_ARTIFACTS,
            "List the files the operator has staged for upload, with their sizes. Scans \
             subdirectories, so a name may be a path like `nginx/site.conf` — use it \
             exactly as given in an upload step; the propose_plan schema and the \
             receipt name the exact path it lands at on each host. Every file under \
             the artifacts directory can be uploaded, including any this list was too \
             long to name. Files a confirmed plan downloaded from a host land here \
             too, under `downloads/plan-N/host/…`, and are uploadable like any other.",
            serde_json::json!({"type": "object", "properties": {}}),
        ),
        ToolDef::function(
            LIST_MANUALS,
            "List the manuals the operator has written about this fleet: a filename and \
             the first line of each. Their filenames and descriptions are already in your \
             instructions, so call this only to see one added since this conversation \
             began. Reading a manual is a local file read — it opens no SSH connection and \
             costs the operator nothing.",
            serde_json::json!({"type": "object", "properties": {}, "additionalProperties": false}),
        ),
        ToolDef::function(
            FETCH_MANUAL,
            "Read one manual in full. A manual is the operator's own instructions about \
             their fleet — how they do a thing here, which is not always how it is done in \
             general. Where a manual covers the task in front of you, its way is the way, \
             ahead of what you would otherwise have done; read it BEFORE proposing a plan \
             rather than after the operator rejects one. Local, so it costs no SSH \
             connection.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "filename": {
                        "type": "string",
                        "description": "A filename exactly as list_manuals or your \
                            instructions give it, which may be a path like `linux/tuning.md`."
                    }
                },
                "required": ["filename"],
                "additionalProperties": false,
            }),
        ),
        ToolDef::function(
            PROPOSE_PLAN,
            "Propose changes for the operator to review. This does NOT run anything: it \
             shows the operator a dialog where they approve or reject each step and each \
             host. You cannot execute anything yourself and there is no tool that will. \
             Put everything the task needs into ONE plan — scripts, uploads and downloads, \
             every host — rather than proposing repeatedly; each proposal costs the \
             operator a decision, and a plan they can read end to end is one they can \
             actually judge. Scripts run non-interactively under `bash -s` and must never \
             prompt.",
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
                                "kind": {"type": "string", "enum": ["scriptlet", "upload", "download"]},
                                "script": {"type": "string", "description": artifacts::describe_script_field()},
                                "artifact": {"type": "string", "description": artifacts::describe_artifact_field()},
                                "path": {"type": "string", "description": "For kind=download: an absolute path on the host, like /var/log/nginx/error.log. Letters, digits and ._-+=@/ only — no spaces or shell characters, because scp runs the remote side through a shell. The file lands under artifacts/ and appears in list_artifacts, so a later plan can upload it to another host."},
                                "dest": {"type": "string", "description": "For kind=download: optional. Where to put the file inside the artifacts directory, as a relative path like `logs/web-01-error.log`. Names exactly one host — two hosts writing one dest would overwrite each other; leave it out and each host gets its own copy under downloads/plan-N/<host>/…, keeping the remote path's shape."},
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
    ];
    // One tool per read-only question rather than one tool taking a command name
    // and an argv: an argv is a token stream, and a token stream is where a weak
    // model puts a pipe. Appended last so the host tools keep their position.
    defs.extend(probe::definitions(&cfg.agent));
    defs
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
            // Under a filter, said so — what the filter is never is: it can
            // hold part of an address.
            match (names.is_empty(), ctx.filtered) {
                (true, false) => ToolOutcome::Text(
                    "No hosts are configured. The operator adds them on the Hosts screen."
                        .to_string(),
                ),
                (true, true) => ToolOutcome::Text(
                    "No host you can use matches the operator's current filter on the \
                     Hosts screen, so none is available to you. If the task needs one, ask \
                     them to change or clear it (Esc on the Hosts screen)."
                        .to_string(),
                ),
                (false, filtered) => {
                    let list = serde_json::to_string(&names)
                        .unwrap_or_else(|e| format!("could not list hosts: {e}"));
                    ToolOutcome::Text(if filtered {
                        format!(
                            "{list}\nThe operator has filtered the host list: these are the \
                             only hosts available to you until they clear it. If the task \
                             needs another, ask them to clear the filter."
                        )
                    } else {
                        list
                    })
                }
            }
        }
        LIST_ARTIFACTS => match artifacts::list(ctx.datadir) {
            Ok(l) if l.items.is_empty() => ToolOutcome::Text(
                "No artifacts staged. The operator puts files in ~/.openadmin/artifacts/."
                    .to_string(),
            ),
            Ok(l) => {
                let body = serde_json::to_string_pretty(&serde_json::json!(
                    l.items
                        .iter()
                        .map(|a| serde_json::json!({"name": a.name, "bytes": a.size}))
                        .collect::<Vec<_>>()
                ))
                .unwrap_or_default();
                // A truncated list said out loud, because the alternative is a
                // model concluding a file is not staged when it is.
                ToolOutcome::Text(if l.truncated {
                    format!(
                        "{body}\nThere are more files than this list names. Anything under \
                         the artifacts directory can still be uploaded by its path, listed \
                         or not."
                    )
                } else {
                    body
                })
            }
            Err(e) => ToolOutcome::Text(format!("could not list artifacts: {e}")),
        },
        LIST_MANUALS => match manuals::list(ctx.datadir) {
            Ok((m, _)) if m.is_empty() => ToolOutcome::Text(format!(
                "No manuals written. The operator puts them in {}, one file each, and a \
                 manual's first line is its description.",
                manuals::dir(ctx.datadir).display()
            )),
            Ok((m, truncated)) => ToolOutcome::Text(manual_index(&m, truncated)),
            Err(e) => ToolOutcome::Text(format!("could not list manuals: {e}")),
        },
        FETCH_MANUAL => fetch_manual(ctx, arguments),
        PROPOSE_PLAN => propose(ctx, arguments),
        CREATE_HOST => create_host(ctx, arguments),
        EDIT_HOST => edit_host(ctx, arguments),
        name if probe::is_probe(name) => ToolOutcome::Text(run_probe(ctx, name, arguments)),
        // Named in full rather than gestured at: a model that invented a tool
        // name is one field away from a working call, and "the readonly_ tools"
        // would leave it guessing which.
        other => ToolOutcome::Text(format!(
            "there is no tool called {other:?}. Available: {}.",
            available(ctx.cfg).join(", ")
        )),
    }
}

/// Every tool name this configuration offers.
fn available(cfg: &Config) -> Vec<&'static str> {
    definitions(cfg)
        .into_iter()
        .map(|d| d.function.name)
        .collect()
}

/// Run one probe: build its argv from the typed fields, check it, send it.
///
/// Two layers, and the order matters. `probe::render` decides *what* to run from
/// fields the model filled in; `readonly::validate` then judges the argv that
/// came out. The second is the boundary and is unchanged from when the model
/// wrote the argv itself, so a mistake in the probe table cannot widen it — it
/// shows up as a refusal here and as a failing test in `probe`.
fn run_probe(ctx: &ToolCtx, name: &str, arguments: &str) -> String {
    let Some(p) = probe::find(name) else {
        return format!("there is no tool called {name:?}.");
    };
    let value: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => return format!("could not read the arguments: {e}"),
    };
    let host_name = value.get("host").and_then(|h| h.as_str()).unwrap_or("");
    let Some(host) = ctx.hosts.iter().find(|h| h.name == host_name) else {
        let names: Vec<&str> = ctx.hosts.iter().map(|h| h.name.as_str()).collect();
        return format!(
            "unknown host {host_name:?}. {}",
            hosts_available(&names, ctx.filtered)
        );
    };

    let allow = &ctx.cfg.agent.readonly_commands;
    let (program, args) = match probe::render(p, &value, allow) {
        Ok(built) => built,
        Err(e) => return format!("refused: {e}"),
    };
    // The whitelist still has the last word on what leaves this machine.
    if let Err(e) = readonly::validate(program, &args, allow) {
        return format!("refused: {e}");
    }

    let remote = ssh::quote_command(program, &args);
    let launch = probe_launch(ctx, host, &remote);
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

/// The ssh invocation for one probe. Through the proxy handed in from all
/// hosts — never one looked up among `ctx.hosts`, where the operator's filter
/// could hide it and send the probe around the proxy.
fn probe_launch(ctx: &ToolCtx, host: &HostRecord, remote: &str) -> ssh::Launch {
    ssh::exec_command(
        host,
        ctx.datadir,
        ctx.cfg,
        remote,
        ssh::Stdin::Null,
        ctx.proxy,
    )
}

/// Sanitize a multi-line block, keeping the line structure.
fn sanitize_block(text: &str) -> String {
    text.lines().map(sanitize).collect::<Vec<_>>().join("\n")
}

/// The index, one manual per line.
///
/// Shared with the system prompt, which carries the same text: a model that has
/// to call a tool to discover that guidance exists is a model that will not, and
/// then it plans from general knowledge while the operator's answer sits on disk.
pub fn manual_index(list: &[manuals::Manual], truncated: bool) -> String {
    let width = list
        .iter()
        .map(|m| m.filename.chars().count())
        .max()
        .unwrap_or(0)
        .min(40);
    let mut out = String::new();
    for m in list {
        if m.description.is_empty() {
            out.push_str(&format!("  {}\n", m.filename));
        } else {
            out.push_str(&format!(
                "  {:width$}  — {}\n",
                m.filename,
                m.description,
                width = width
            ));
        }
    }
    if truncated {
        out.push_str("  … and more not named here; list_manuals shows what it can.\n");
    }
    out
}

fn fetch_manual(ctx: &ToolCtx, arguments: &str) -> ToolOutcome {
    let value: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => return ToolOutcome::Text(format!("could not read the arguments: {e}")),
    };
    let Some(name) = value.get("filename").and_then(|f| f.as_str()) else {
        return ToolOutcome::Text(
            "refused: fetch_manual needs filename, a name from list_manuals.".to_string(),
        );
    };
    match manuals::read(ctx.datadir, name, ctx.cfg.agent.output_cap_bytes) {
        // The model gets the manual; the operator gets a line saying which one
        // was read. They wrote it, so the transcript does not repeat it back.
        Ok(text) => ToolOutcome::Loaded {
            receipt: format!("read {name} ({} bytes)", text.len()),
            text,
        },
        Err(e) => ToolOutcome::Text(format!("refused: {e}")),
    }
}

fn propose(ctx: &ToolCtx, arguments: &str) -> ToolOutcome {
    let req: PlanRequest = match serde_json::from_str(arguments) {
        Ok(r) => r,
        Err(e) => return ToolOutcome::Text(format!("could not read the plan: {e}")),
    };
    let known: Vec<(i64, String)> = ctx.hosts.iter().map(|h| (h.id, h.name.clone())).collect();
    match resolve(ctx.next_plan_id, req, &known, ctx.filtered) {
        Ok(plan) => {
            // Every upload names a file that can actually be staged, and every
            // explicit download dest names a path that can actually be written
            // — checked here, the one moment the model can fix a wrong name
            // without costing the operator anything. And the receipt states
            // each exact destination, so a scriptlet in a follow-up plan never
            // has to infer it.
            let mut destinations = Vec::new();
            for step in &plan.steps {
                match &step.kind {
                    StepKind::Upload { artifact } => {
                        match artifacts::staged(ctx.datadir, artifact) {
                            Ok((_, rel)) => destinations.push(format!(
                                "{rel} lands at {}",
                                artifacts::upload_target(&rel).1
                            )),
                            Err(e) => {
                                return ToolOutcome::Text(format!(
                                    "the plan was not accepted: step {:?} uploads \
                                     {artifact:?}, which cannot be staged: {e}",
                                    step.summary
                                ));
                            }
                        }
                    }
                    StepKind::Download { path, dest } if !dest.trim().is_empty() => {
                        if let Err(e) = crate::agent::store::ARTIFACTS.dest(ctx.datadir, dest) {
                            return ToolOutcome::Text(format!(
                                "the plan was not accepted: step {:?} downloads {path:?} to \
                                 {dest:?}, which cannot be a destination: {e}",
                                step.summary
                            ));
                        }
                        destinations.push(format!("{path} → artifacts/{}", dest.trim()));
                    }
                    _ => {}
                }
            }
            let upload_note = if destinations.is_empty() {
                String::new()
            } else {
                format!(" {} on each host.", destinations.join("; "))
            };
            let receipt = format!(
                "Plan #{} recorded: {} step(s) across {} host(s).{upload_note} It is now \
                 waiting for the operator to approve or reject it, step by step and host by \
                 host. Nothing has run. Wait for the execution report before proposing \
                 anything else.",
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
        // Under a filter a hidden host is refused the same way — and said why,
        // or the natural next step, `create_host`, would duplicate it.
        let filter = if ctx.filtered {
            " The operator has filtered the host list, so a host outside it cannot be \
             changed — if that is the one they meant, ask them to clear the filter."
        } else {
            ""
        };
        return ToolOutcome::Text(format!(
            "refused: no host by that name. This tool changes a record that already \
             exists; {CREATE_HOST} adds a new one.{filter}"
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
            filtered: false,
            proxy: None,
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
    fn a_refused_call_explains_itself_rather_than_failing_the_turn() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let c = ctx(&h, &cfg, Path::new("/tmp"), &cancel);

        // There is no tool that writes and no field that takes a command, so the
        // refusals a model can still reach are about its own fields.
        let out = dispatch(
            &c,
            "readonly_service",
            r#"{"host":"web-01","action":"restart","units":["nginx"]}"#,
        );
        assert!(out.text().starts_with("refused:"), "{}", out.text());
        assert!(out.text().contains("status"), "{}", out.text());

        let out = dispatch(
            &c,
            "readonly_logs",
            r#"{"host":"web-01","command":"journalctl -f"}"#,
        );
        assert!(out.text().contains("no field"), "{}", out.text());

        let out = dispatch(
            &c,
            "readonly_read_file",
            r#"{"host":"nowhere","paths":["/etc"]}"#,
        );
        assert!(out.text().contains("unknown host"), "{}", out.text());

        let out = dispatch(&c, "readonly_logs", "not json");
        assert!(out.text().contains("could not read"), "{}", out.text());

        // A tool the model invented is answered with the ones that exist.
        let out = dispatch(&c, "run_readonly", r#"{"host":"web-01","command":"ls"}"#);
        assert!(out.text().contains("no tool called"), "{}", out.text());
        assert!(out.text().contains("readonly_read_file"), "{}", out.text());
    }

    /// The model gets the manual; the operator gets a line. The transcript keeps
    /// and re-renders every line of a tool result, so a fetch that reported
    /// itself in full would bury the conversation in the operator's own prose.
    #[test]
    fn a_fetched_manual_goes_to_the_model_and_a_line_to_the_operator() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let dir = std::env::temp_dir().join(format!("openadmin-tools-man-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let m = manuals::ensure_dir(&dir).unwrap();
        std::fs::write(
            m.join("db-failover.md"),
            "# Promoting the standby\n\nStop the primary first.\n",
        )
        .unwrap();
        let c = ctx(&h, &cfg, &dir, &cancel);

        let out = dispatch(&c, FETCH_MANUAL, r#"{"filename":"db-failover.md"}"#);
        assert!(matches!(out, ToolOutcome::Loaded { .. }), "{}", out.text());
        assert!(
            out.text().contains("Stop the primary first."),
            "{}",
            out.text()
        );
        // The operator's copy names the manual and its size, not its contents.
        assert!(
            out.transcript().starts_with("read db-failover.md"),
            "{}",
            out.transcript()
        );
        assert!(
            !out.transcript().contains("Stop the primary"),
            "{}",
            out.transcript()
        );

        // The same index the prompt carries, for a manual added mid-session.
        let out = dispatch(&c, LIST_MANUALS, "{}");
        assert!(out.text().contains("db-failover.md"), "{}", out.text());
        assert!(
            out.text().contains("Promoting the standby"),
            "{}",
            out.text()
        );

        // A name that is not there is a refusal, spelled so the UI colours it one.
        let out = dispatch(&c, FETCH_MANUAL, r#"{"filename":"nope.md"}"#);
        assert!(out.text().starts_with("refused:"), "{}", out.text());
        let out = dispatch(&c, FETCH_MANUAL, r#"{"path":"nope.md"}"#);
        assert!(out.text().contains("needs filename"), "{}", out.text());

        // With none written, the tool says where they go rather than nothing.
        let empty =
            std::env::temp_dir().join(format!("openadmin-tools-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).unwrap();
        let c2 = ctx(&h, &cfg, &empty, &cancel);
        let out = dispatch(&c2, LIST_MANUALS, "{}");
        assert!(out.text().contains("No manuals written"), "{}", out.text());
        assert!(out.text().contains("manuals"), "{}", out.text());

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }

    /// The index is one line per manual and is what the prompt carries, so its
    /// shape is worth pinning: a name with no description still appears, or it
    /// cannot be fetched.
    #[test]
    fn the_index_names_every_manual_it_has() {
        let index = manual_index(
            &[
                manuals::Manual {
                    filename: "a.md".into(),
                    description: "First".into(),
                },
                manuals::Manual {
                    filename: "b/c.md".into(),
                    description: String::new(),
                },
            ],
            true,
        );
        assert!(index.contains("a.md") && index.contains("First"), "{index}");
        assert!(index.contains("b/c.md"), "{index}");
        assert!(
            index.contains("list_manuals"),
            "truncation is said: {index}"
        );
        assert_eq!(index.lines().count(), 3, "{index}");
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

    /// The chicken-and-egg this arrangement exists to remove: the model writes
    /// the path of a file the plan has not uploaded yet, in the same call that
    /// uploads it — so the destination named in the schema has to be the one the
    /// executor uses. Asserted against the constant rather than a string typed
    /// here, because a string typed here is exactly the drift being guarded
    /// against.
    #[test]
    fn the_plan_schema_names_where_an_upload_lands() {
        let defs = definitions(&Config::default());
        let plan = defs
            .iter()
            .find(|d| d.function.name == PROPOSE_PLAN)
            .unwrap();
        let props = &plan.function.parameters["properties"]["steps"]["items"]["properties"];
        let dest = artifacts::upload_target("nginx/site.conf").1;
        // Both fields: the model reads the one it is filling in, not the other.
        for field in ["artifact", "script"] {
            let text = props[field]["description"].as_str().unwrap_or_default();
            assert!(
                text.contains(artifacts::UPLOAD_DIR),
                "{field} does not say where an upload lands: {text}"
            );
            assert!(
                text.contains(&dest),
                "{field} names the directory but not the whole path: {text}"
            );
        }
    }

    /// The receipt names the exact destination of every upload — the model has
    /// just committed scripts that may reference it, and a description like
    /// "the plan's upload directory" is a thing to guess at, not a path.
    /// An artifact that cannot be staged is refused here, at the only moment
    /// the model can fix the name without costing the operator anything.
    #[test]
    fn the_receipt_names_each_uploads_exact_destination() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let dir = std::env::temp_dir().join(format!("openadmin-tools-up-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = artifacts::ensure_dir(&dir).unwrap();
        std::fs::write(a.join("hotfix.sh"), "#!/bin/sh\n").unwrap();
        std::fs::create_dir_all(a.join("nginx")).unwrap();
        std::fs::write(a.join("nginx/site.conf"), "server {}\n").unwrap();
        let c = ctx(&h, &cfg, &dir, &cancel);

        let out = dispatch(
            &c,
            PROPOSE_PLAN,
            r#"{"steps":[
                {"kind":"upload","artifact":"hotfix.sh","hosts":["web-01"]},
                {"kind":"upload","artifact":"nginx/site.conf","hosts":["db-main"]}]}"#,
        );
        match out {
            ToolOutcome::Proposal { receipt, .. } => {
                assert!(
                    receipt.contains("hotfix.sh lands at /tmp/openadmin/hotfix.sh"),
                    "{receipt}"
                );
                assert!(
                    receipt.contains("nginx/site.conf lands at /tmp/openadmin/nginx/site.conf"),
                    "{receipt}"
                );
            }
            other => panic!("expected a proposal, got: {}", other.text()),
        }

        // A name that is not staged is a refusal the model can act on, not a
        // plan that fails on the operator halfway through.
        let out = dispatch(
            &c,
            PROPOSE_PLAN,
            r#"{"steps":[{"kind":"upload","artifact":"nope.sh","hosts":["web-01"]}]}"#,
        );
        assert!(matches!(out, ToolOutcome::Text(_)));
        assert!(
            out.text().contains("the plan was not accepted"),
            "{}",
            out.text()
        );
        assert!(out.text().contains("nope.sh"), "{}", out.text());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A download with an explicit dest names where it lands in the receipt,
    /// and a dest that cannot be written is refused at the one moment the
    /// model can fix it for free.
    #[test]
    fn the_receipt_names_an_explicit_download_dest() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let dir = std::env::temp_dir().join(format!("openadmin-tools-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        artifacts::ensure_dir(&dir).unwrap();
        let c = ctx(&h, &cfg, &dir, &cancel);

        let out = dispatch(
            &c,
            PROPOSE_PLAN,
            r#"{"steps":[{"kind":"download","path":"/var/log/pg.log",
                     "dest":"logs/db.log","hosts":["db-main"]}]}"#,
        );
        match out {
            ToolOutcome::Proposal { receipt, .. } => {
                assert!(
                    receipt.contains("/var/log/pg.log → artifacts/logs/db.log"),
                    "{receipt}"
                );
            }
            other => panic!("expected a proposal, got: {}", other.text()),
        }

        // A dest that escapes the artifacts directory is not a destination.
        let out = dispatch(
            &c,
            PROPOSE_PLAN,
            r#"{"steps":[{"kind":"download","path":"/var/log/pg.log",
                     "dest":"../keys/x","hosts":["db-main"]}]}"#,
        );
        assert!(matches!(out, ToolOutcome::Text(_)));
        assert!(
            out.text().contains("the plan was not accepted"),
            "{}",
            out.text()
        );
        assert!(out.text().contains("keys"), "{}", out.text());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_tool_schemas_name_the_boundary() {
        let cfg = Config::default();
        let defs = definitions(&cfg);
        // Seven tools that are not read-only probes, plus one probe per
        // read-only question. The probe count is asserted loosely on purpose:
        // adding a probe is a normal change, and a test that fails on it teaches
        // nothing.
        assert_eq!(
            defs.iter()
                .filter(|d| !d.function.name.starts_with("readonly_"))
                .count(),
            7
        );
        assert!(defs.len() > 15, "the probes are there too: {}", defs.len());
        let plan = defs
            .iter()
            .find(|d| d.function.name == PROPOSE_PLAN)
            .unwrap();
        // The model is told the gate is structural, so it does not waste turns.
        assert!(plan.function.description.contains("does NOT run anything"));
        assert!(plan.function.description.contains("no tool that will"));
        // And that one big plan is wanted.
        assert!(plan.function.description.contains("ONE plan"));

        // One tool per read-only question, each with typed fields and no field
        // that holds a command line. This is what stopped a weak model reaching
        // for a shell: there is nowhere left to put one.
        let names: Vec<&str> = defs.iter().map(|d| d.function.name).collect();
        for expected in [
            "readonly_logs",
            "readonly_service",
            "readonly_network",
            "readonly_read_file",
            "readonly_search_files",
        ] {
            assert!(names.contains(&expected), "{names:?}");
        }
        assert!(
            !names.contains(&"run_readonly"),
            "the argv tool is gone: {names:?}"
        );
        assert!(names.contains(&LIST_MANUALS), "{names:?}");
        assert!(names.contains(&FETCH_MANUAL), "{names:?}");
        let fetch = defs
            .iter()
            .find(|d| d.function.name == FETCH_MANUAL)
            .unwrap();
        assert_eq!(
            fetch.function.parameters["additionalProperties"],
            serde_json::json!(false)
        );
        assert_eq!(
            fetch.function.parameters["required"],
            serde_json::json!(["filename"])
        );
        // What a manual *is*, said where the call is constructed.
        assert!(
            fetch
                .function
                .description
                .contains("operator's own instructions")
        );
        assert!(
            fetch
                .function
                .description
                .contains("BEFORE proposing a plan")
        );

        for d in defs
            .iter()
            .filter(|d| d.function.name.starts_with("readonly_"))
        {
            let props = d.function.parameters["properties"].as_object().unwrap();
            assert_eq!(
                d.function.parameters["additionalProperties"],
                serde_json::json!(false),
                "{} is open",
                d.function.name
            );
            for f in ["command", "args", "argv", "options", "shell"] {
                assert!(
                    !props.contains_key(f),
                    "{} still has a {f} field",
                    d.function.name
                );
            }
        }

        // The operator's narrowing reaches the tool list itself.
        let narrow = Config {
            agent: crate::config::AgentConfig {
                readonly_commands: vec!["cat".into()],
                ..Config::default().agent
            },
            ..Config::default()
        };
        let names: Vec<&str> = definitions(&narrow)
            .iter()
            .map(|d| d.function.name)
            .collect();
        assert!(names.contains(&"readonly_read_file"), "{names:?}");
        assert!(!names.contains(&"readonly_logs"), "{names:?}");
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

    /// A context the operator's filter has narrowed to `hosts`.
    fn filtered<'a>(
        hosts: &'a [HostRecord],
        cfg: &'a Config,
        cancel: &'a AtomicBool,
    ) -> ToolCtx<'a> {
        ToolCtx {
            filtered: true,
            ..ctx(hosts, cfg, Path::new("/tmp"), cancel)
        }
    }

    /// Under a filter the list says so — these are all the model may use — and
    /// never what the filter is.
    #[test]
    fn a_filtered_list_says_so() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(&filtered(&h[..1], &cfg, &cancel), LIST_HOSTS, "{}");
        assert_eq!(
            out.text(),
            "[\"web-01\"]\nThe operator has filtered the host list: these are the only hosts \
             available to you until they clear it. If the task needs another, ask them to \
             clear the filter."
        );
    }

    /// A filter matching none of the model's hosts is not "no hosts configured":
    /// the operator has hosts, and the model is told how to get one back.
    #[test]
    fn a_filter_that_matches_nothing_is_not_no_hosts_configured() {
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let text = dispatch(&filtered(&[], &cfg, &cancel), LIST_HOSTS, "{}")
            .text()
            .to_string();
        assert!(!text.contains("configured"), "{text}");
        assert!(text.contains("filter") && text.contains("Esc"), "{text}");
    }

    /// Every unknown name gets the same answer under a filter — a host it hides
    /// and one that never existed alike — so no refusal says which is which.
    #[test]
    fn unknown_hosts_under_a_filter_all_get_the_same_answer() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let c = filtered(&h[..1], &cfg, &cancel);
        let calls = [
            (
                "readonly_read_file",
                r#"{"host":"HOST","paths":["/etc/hostname"]}"#,
            ),
            (
                PROPOSE_PLAN,
                r#"{"steps":[{"kind":"scriptlet","script":"id","hosts":["HOST"]}]}"#,
            ),
            (EDIT_HOST, r#"{"host":"HOST","port":2222}"#),
        ];
        for (tool, args) in calls {
            let hidden = dispatch(&c, tool, &args.replace("HOST", "db-main"))
                .text()
                .replace("db-main", "X");
            let absent = dispatch(&c, tool, &args.replace("HOST", "nowhere"))
                .text()
                .replace("nowhere", "X");
            assert_eq!(hidden, absent, "{tool}");
            assert!(hidden.contains("filtered"), "{tool}: {hidden}");
        }
    }

    /// The proxy is handed in from all hosts: a probe goes through it though the
    /// filter hides the proxy host — and a proxy flag on a host in scope no longer
    /// decides anything.
    #[test]
    fn a_hidden_proxy_still_carries_probes() {
        let mut h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let proxy = HostRecord {
            proxy: true,
            ..h[1].clone()
        };
        let via = |c: &ToolCtx| {
            probe_launch(c, &c.hosts[0], "id")
                .args
                .iter()
                .any(|a| a.starts_with("ProxyCommand="))
        };
        let mut c = filtered(&h[..1], &cfg, &cancel);
        c.proxy = Some(&proxy);
        assert!(via(&c), "the hidden proxy carries it");

        h[0].proxy = false;
        h[1].proxy = true;
        let c = ctx(&h, &cfg, Path::new("/tmp"), &cancel);
        assert!(!via(&c), "only the proxy handed in counts");
    }

    /// The contract is in the schema, where it is read — and the schema stays the
    /// same whatever the filter, as the cached prompt prefix must.
    #[test]
    fn the_list_hosts_schema_states_the_filter_contract() {
        let defs = definitions(&Config::default());
        let list = defs.iter().find(|d| d.function.name == LIST_HOSTS).unwrap();
        let d = &list.function.description;
        assert!(d.contains("filter on the Hosts screen"), "{d}");
        assert!(d.contains("not given to you"), "{d}");
        assert!(d.contains("same unknown-host answer"), "{d}");
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
            filtered: false,
            proxy: None,
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
