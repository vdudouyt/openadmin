//! Running a confirmed plan.
//!
//! The only code in the program that runs a scriptlet or uploads a file, and it
//! takes a `ConfirmedPlan` — which only the dialog can build. See
//! `crate::app::approve`.
//!
//! **Nothing halts.** A step that fails on one host does not stop the other
//! hosts, and does not stop later steps: the whole plan runs and the report
//! names every failure, so the model can propose a follow-up scoped to exactly
//! the hosts that need one. Halting early would leave the fleet in a state
//! nobody asked for and nobody can see.

use super::exec::{Captured, run_capture};
use crate::agent::artifacts;
use crate::agent::plan::StepKind;
use crate::app::approve::ConfirmedPlan;
use crate::config::Config;
use crate::db::model::HostRecord;
use crate::ssh;
use crate::ui::widgets::sanitize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// What one (step, host) pair did.
#[derive(Debug, Clone)]
pub struct StepResult {
    pub step: usize,
    pub summary: String,
    pub host: String,
    pub exit: Option<i32>,
    pub timed_out: bool,
    pub output: Vec<String>,
    pub skipped: bool,
}

impl StepResult {
    pub fn ok(&self) -> bool {
        !self.skipped && !self.timed_out && self.exit == Some(0)
    }
}

#[derive(Debug, Clone)]
pub struct ExecReport {
    pub plan_id: u64,
    pub title: String,
    pub results: Vec<StepResult>,
}

impl ExecReport {
    pub fn failures(&self) -> Vec<&StepResult> {
        self.results.iter().filter(|r| !r.ok()).collect()
    }

    /// The text handed back to the model.
    ///
    /// Deliberately plain prose with the exit status of every pair, and an
    /// explicit instruction at the end, because this is the moment the model
    /// decides whether to remediate.
    pub fn to_text(&self) -> String {
        let failed = self.failures();
        let mut s = format!(
            "Execution report for plan #{} ({}). {} of {} host-runs failed.\n",
            self.plan_id,
            self.title,
            failed.len(),
            self.results.len()
        );
        for r in &self.results {
            let status = if r.skipped {
                "skipped (cancelled)".to_string()
            } else if r.timed_out {
                "timed out".to_string()
            } else {
                match r.exit {
                    Some(c) => format!("exit {c}"),
                    None => "killed".to_string(),
                }
            };
            s.push_str(&format!(
                "\nstep {} \"{}\" on {}: {}\n",
                r.step, r.summary, r.host, status
            ));
            for line in &r.output {
                s.push_str("  ");
                s.push_str(line);
                s.push('\n');
            }
        }
        if failed.is_empty() {
            s.push_str("\nEverything succeeded. Tell the operator what changed, briefly.");
        } else {
            let mut hosts: Vec<&str> = failed.iter().map(|r| r.host.as_str()).collect();
            hosts.sort_unstable();
            hosts.dedup();
            s.push_str(&format!(
                "\nFailed hosts: {}. Work out why, then propose a follow-up plan \
                 targeting only those hosts.",
                hosts.join(", ")
            ));
        }
        s
    }
}

/// Where uploads land: one directory per plan, so a step cannot clobber a file
/// something else is relying on at a predictable path.
fn upload_dir(plan_id: u64) -> String {
    format!("/tmp/openadmin-plan-{plan_id}")
}

/// Where one staged file lands: `(directory to create, full path)`.
///
/// The staged path is kept rather than flattened to a basename, because
/// `nginx/site.conf` and `apache/site.conf` are two different files and
/// flattening them would have the second silently overwrite the first. It also
/// lets a scriptlet in the same plan name an upload by the path it already knows
/// from `list_artifacts`.
fn upload_target(plan_id: u64, rel: &str) -> (String, String) {
    let base = upload_dir(plan_id);
    let dir = match rel.rsplit_once('/') {
        Some((parent, _)) => format!("{base}/{parent}"),
        None => base.clone(),
    };
    (dir, format!("{base}/{rel}"))
}

/// Run the plan, one step at a time, one host at a time.
///
/// `on_line` receives each output line as it arrives; `on_step` brackets each
/// pair so the UI can open and close a block.
#[allow(clippy::too_many_arguments)]
pub fn execute(
    plan: &ConfirmedPlan,
    hosts: &[HostRecord],
    datadir: &Path,
    cfg: &Config,
    cancel: &AtomicBool,
    mut on_step: impl FnMut(usize, &str, &str),
    on_line: &mut dyn FnMut(String),
    mut on_finish: impl FnMut(Option<i32>, bool),
) -> ExecReport {
    let mut results = Vec::new();
    let proxy = hosts.iter().find(|h| h.proxy).cloned();

    for (i, step) in plan.steps().iter().enumerate() {
        let step_no = i + 1;
        for host_id in &step.hosts {
            let Some(host) = hosts.iter().find(|h| h.id == *host_id) else {
                // The host was deleted between proposal and confirmation.
                results.push(StepResult {
                    step: step_no,
                    summary: step.summary.clone(),
                    host: format!("#{host_id}"),
                    exit: None,
                    timed_out: false,
                    output: vec!["host no longer exists".to_string()],
                    skipped: true,
                });
                continue;
            };

            if cancel.load(Ordering::Relaxed) {
                results.push(StepResult {
                    step: step_no,
                    summary: step.summary.clone(),
                    host: host.name.clone(),
                    exit: None,
                    timed_out: false,
                    output: Vec::new(),
                    skipped: true,
                });
                continue;
            }

            on_step(step_no, &host.name, &step.summary);
            let captured = run_one(
                plan,
                &step.kind,
                host,
                proxy.as_ref(),
                datadir,
                cfg,
                on_line,
            );
            let (exit, timed_out, output) = match captured {
                Ok(c) => {
                    let mut out: Vec<String> = c.stdout.lines().map(sanitize).collect();
                    out.extend(c.stderr.lines().map(sanitize));
                    (c.exit, c.timed_out, out)
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    on_line(msg.clone());
                    (None, false, vec![msg])
                }
            };
            on_finish(exit, timed_out);
            results.push(StepResult {
                step: step_no,
                summary: step.summary.clone(),
                host: host.name.clone(),
                exit,
                timed_out,
                output,
                skipped: false,
            });
        }
    }

    ExecReport {
        plan_id: plan.plan_id(),
        title: plan.title().to_string(),
        results,
    }
}

#[allow(clippy::too_many_arguments)] // a step needs all of it
fn run_one(
    plan: &ConfirmedPlan,
    kind: &StepKind,
    host: &HostRecord,
    proxy: Option<&HostRecord>,
    datadir: &Path,
    cfg: &Config,
    on_line: &mut dyn FnMut(String),
) -> anyhow::Result<Captured> {
    let timeout = Duration::from_secs(cfg.agent.command_timeout_secs.max(300));
    let cap = cfg.agent.output_cap_bytes;
    let (tx, rx) = std::sync::mpsc::channel();

    let captured = match kind {
        StepKind::Scriptlet { script } => {
            // The script goes over stdin to `bash -s`, never as an argument, so
            // no quoting question arises at all. `-n` must not be passed here or
            // bash would read an empty script and exit 0.
            let launch = ssh::exec_command(host, datadir, cfg, "bash -s", ssh::Stdin::Pipe, proxy);
            run_capture(&launch, Some(script.as_bytes()), timeout, cap, Some(&tx))
        }
        StepKind::Upload { artifact } => {
            // Resolved and containment-checked: a name cannot escape the
            // artifacts directory, by `..`, by symlink, or by a subdirectory.
            // `rel` comes back from the canonicalized path, so it is a clean
            // relative path whatever the model wrote.
            let (local, rel) = artifacts::staged(datadir, artifact)?;
            let (dir, dest) = upload_target(plan.plan_id(), &rel);
            let mkdir = ssh::exec_command(
                host,
                datadir,
                cfg,
                &ssh::quote_command("mkdir", &["-p".into(), dir.clone()]),
                ssh::Stdin::Null,
                proxy,
            );
            let made = run_capture(&mkdir, None, timeout, cap, Some(&tx))?;
            if !made.success() {
                return Ok(made);
            }
            let launch = ssh::scp_command(host, datadir, cfg, &local, &dir, proxy);
            let out = run_capture(&launch, None, timeout, cap, Some(&tx));
            if out.as_ref().is_ok_and(|c| c.success()) {
                on_line(format!("uploaded {artifact} to {dest}"));
            }
            out
        }
    };

    drop(tx);
    for (is_err, line) in rx {
        let line = sanitize(&line);
        on_line(if is_err { format!("! {line}") } else { line });
    }
    captured
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(results: Vec<StepResult>) -> ExecReport {
        ExecReport {
            plan_id: 2,
            title: "tidy".into(),
            results,
        }
    }

    fn r(step: usize, host: &str, exit: Option<i32>) -> StepResult {
        StepResult {
            step,
            summary: "do a thing".into(),
            host: host.into(),
            exit,
            timed_out: false,
            output: vec!["some output".into()],
            skipped: false,
        }
    }

    #[test]
    fn a_clean_report_says_so_and_asks_for_a_summary() {
        let t = report(vec![r(1, "web-01", Some(0)), r(1, "web-02", Some(0))]).to_text();
        assert!(t.contains("0 of 2 host-runs failed"), "{t}");
        assert!(t.contains("Everything succeeded"), "{t}");
        assert!(!t.contains("Failed hosts"), "{t}");
    }

    /// The report is what turns a failure into a follow-up plan, so it has to
    /// name the hosts and ask for exactly that.
    #[test]
    fn failures_are_named_and_a_follow_up_is_requested() {
        let t = report(vec![
            r(1, "web-01", Some(0)),
            r(1, "web-02", Some(1)),
            r(2, "db-main", Some(127)),
        ])
        .to_text();
        assert!(t.contains("2 of 3 host-runs failed"), "{t}");
        assert!(t.contains("exit 1"), "{t}");
        assert!(t.contains("exit 127"), "{t}");
        assert!(t.contains("Failed hosts: db-main, web-02"), "{t}");
        assert!(t.contains("only those hosts"), "{t}");
        // The successful host is still reported — the model needs the whole
        // picture, not just what broke.
        assert!(t.contains("web-01"), "{t}");
    }

    #[test]
    fn a_timeout_and_a_skip_read_differently_from_an_exit_code() {
        let mut a = r(1, "slow", None);
        a.timed_out = true;
        let mut b = r(1, "cancelled", None);
        b.skipped = true;
        let t = report(vec![a, b]).to_text();
        assert!(t.contains("timed out"), "{t}");
        assert!(t.contains("skipped (cancelled)"), "{t}");
        assert_eq!(report(vec![r(1, "x", Some(0))]).failures().len(), 0);
    }

    #[test]
    fn uploads_land_in_a_directory_of_their_own() {
        assert_eq!(upload_dir(7), "/tmp/openadmin-plan-7");
    }

    /// A staged subdirectory survives the upload, so two files of the same name
    /// from different directories stay two files.
    #[test]
    fn a_staged_path_keeps_its_shape_on_the_far_side() {
        assert_eq!(
            upload_target(3, "hotfix.sh"),
            (
                "/tmp/openadmin-plan-3".to_string(),
                "/tmp/openadmin-plan-3/hotfix.sh".to_string()
            )
        );
        assert_eq!(
            upload_target(3, "nginx/site.conf"),
            (
                "/tmp/openadmin-plan-3/nginx".to_string(),
                "/tmp/openadmin-plan-3/nginx/site.conf".to_string()
            )
        );
        assert_eq!(
            upload_target(3, "a/b/c/deep.conf").1,
            "/tmp/openadmin-plan-3/a/b/c/deep.conf"
        );
        // The collision this exists to prevent.
        assert_ne!(
            upload_target(3, "nginx/site.conf").1,
            upload_target(3, "apache/site.conf").1
        );
    }

    #[test]
    fn ok_is_exactly_exit_zero() {
        assert!(r(1, "h", Some(0)).ok());
        assert!(!r(1, "h", Some(1)).ok());
        assert!(!r(1, "h", None).ok());
        let mut s = r(1, "h", Some(0));
        s.skipped = true;
        assert!(!s.ok(), "a skipped pair did not succeed");
    }
}
