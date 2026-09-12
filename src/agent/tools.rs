//! The four tools, and the dispatch for the three that need no confirmation.
//!
//! `propose_plan` is the fourth and it executes nothing: it returns a `Plan`
//! for the UI to show. Nothing in this module can run a scriptlet or upload a
//! file — `ToolCtx` carries no executor, no channel and no handle that reaches
//! one, so the only outbound effects reachable from here are a validated
//! read-only command and reading the artifacts directory.

use super::exec::run_capture;
use super::plan::{Plan, PlanRequest, resolve};
use super::proto::ToolDef;
use super::{artifacts, readonly};
use crate::config::Config;
use crate::db::model::HostRecord;
use crate::ssh;
use crate::ui::widgets::sanitize;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// What the model is told about a host.
///
/// Built field by field, and deliberately *not* a `Serialize` derive on
/// `HostRecord`: that would put `pass` and `key_value` on the wire the first
/// time anyone called `list_hosts`.
#[derive(Debug, Serialize)]
struct HostView<'a> {
    name: &'a str,
    proto: String,
    addr: &'a str,
    port: i64,
    login: &'a str,
    mount_point: &'a str,
    mounted: bool,
    /// Whether a key is installed — never the key, and never the passphrase.
    has_key: bool,
    is_proxy: bool,
}

impl<'a> From<&'a HostRecord> for HostView<'a> {
    fn from(h: &'a HostRecord) -> Self {
        HostView {
            name: &h.name,
            proto: h.proto.to_uppercase(),
            addr: &h.addr,
            port: h.port,
            login: &h.login,
            mount_point: &h.mount_point,
            mounted: h.mounted,
            has_key: !h.key_name.is_empty(),
            is_proxy: h.proxy,
        }
    }
}

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
}

/// What a dispatched tool produced.
pub enum ToolOutcome {
    /// Text to hand straight back as the tool result.
    Text(String),
    /// A proposal. The tool result is only a receipt; the plan goes to the UI
    /// and waits for a human.
    Proposal { receipt: String, plan: Box<Plan> },
}

impl ToolOutcome {
    pub fn text(&self) -> &str {
        match self {
            ToolOutcome::Text(t) => t,
            ToolOutcome::Proposal { receipt, .. } => receipt,
        }
    }
}

pub const RUN_READONLY: &str = "run_readonly";
pub const LIST_HOSTS: &str = "list_hosts";
pub const LIST_ARTIFACTS: &str = "list_artifacts";
pub const PROPOSE_PLAN: &str = "propose_plan";

/// The schemas sent with every request.
pub fn definitions(cfg: &Config) -> Vec<ToolDef> {
    let allow = &cfg.agent.readonly_commands;
    vec![
        ToolDef::function(
            LIST_HOSTS,
            "List the machines this installation knows about. Returns a snapshot \
             taken when the turn began; adding a host mid-turn will not appear here. \
             Passwords are never returned.",
            serde_json::json!({"type": "object", "properties": {}}),
        ),
        ToolDef::function(
            RUN_READONLY,
            format!(
                "Run one read-only command on one host and return its output and exit \
                 status. Runs unattended, with no confirmation, so use it freely to find \
                 out what is actually true before proposing any change. {}",
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
        LIST_HOSTS => {
            let views: Vec<HostView> = ctx.hosts.iter().map(HostView::from).collect();
            ToolOutcome::Text(
                serde_json::to_string_pretty(&views)
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
        other => ToolOutcome::Text(format!(
            "there is no tool called {other:?}. Available: {LIST_HOSTS}, {RUN_READONLY}, \
             {LIST_ARTIFACTS}, {PROPOSE_PLAN}."
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
        }
    }

    /// The single most important property of `list_hosts`.
    #[test]
    fn listing_hosts_never_leaks_a_secret() {
        let h = hosts();
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let out = dispatch(&ctx(&h, &cfg, Path::new("/tmp"), &cancel), LIST_HOSTS, "{}");
        let text = out.text();
        assert!(text.contains("web-01"));
        assert!(text.contains("10.0.4.11"));
        assert!(text.contains("\"has_key\": true"));
        assert!(!text.contains("hunter2"), "password leaked: {text}");
        assert!(
            !text.contains("passphrase"),
            "key passphrase leaked: {text}"
        );
        assert!(!text.contains("s3cret"), "password leaked: {text}");
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
            ToolOutcome::Text(t) => panic!("expected a proposal, got: {t}"),
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
        assert_eq!(defs.len(), 4);
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
        assert!(ro.function.description.contains("whitelisted"));
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
}
