//! The agentic chat loop.
//!
//! One worker thread owns the conversation, the model client and the read-only
//! tools. That keeps the turn — send, stream, dispatch tools, send again —
//! as straight-line blocking code, so the worker's stack *is* the state
//! machine and `App` holds no "which phase of the turn is this" flag.
//!
//! The one thing the worker cannot do is change a remote machine. Its tools
//! reach a validated read-only command and the artifacts directory, and nothing
//! else; a plan it proposes is data that goes to the UI and waits for a person.

pub mod artifacts;
pub mod client;
pub mod exec;
pub mod exec_plan;
pub mod hosts;
pub mod manuals;
pub mod plan;
pub mod probe;
pub mod proto;
pub mod readonly;
pub mod store;
pub mod tools;

use crate::config::Config;
use crate::db::model::HostRecord;
use anyhow::{Context, Result};
use client::LlmClient;
use plan::Plan;
use proto::{Message, TurnAccumulator};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use tools::{ToolCtx, ToolOutcome};

/// How many model round-trips one user message may take before we stop. A model
/// that keeps calling tools without concluding would otherwise run until the
/// context window or the bill ran out.
const MAX_STEPS: usize = 24;

/// The hosts the model may see and act on, as the operator has scoped them —
/// a snapshot, because `DataBase` is not `Sync` and lives on the UI thread.
#[derive(Debug, Clone, Default)]
pub struct HostScope {
    /// SSH hosts the Hosts screen lists: all of them, or those its filter
    /// matches. Every tool reads this and nothing wider.
    pub hosts: Vec<HostRecord>,
    /// Whether a filter narrowed `hosts`. The model is told so; never what the
    /// filter is, which can hold part of an address.
    pub filtered: bool,
    /// The proxy host, from *all* hosts: hiding it must not route around it.
    /// Its record only switches the SOCKS route on; the model never sees it.
    pub proxy: Option<HostRecord>,
}

/// What the UI asks the worker to do.
pub enum AgentCommand {
    /// Run a plan the operator confirmed. The payload can only be built by
    /// `crate::app::approve`, which is what makes execution unreachable from
    /// here.
    Execute {
        plan: crate::app::approve::ConfirmedPlan,
        /// The hosts the plan names, as the operator approved them — looked
        /// up among all hosts, so a filter changed since cannot skip one.
        run_on: Vec<HostRecord>,
        /// What the model may see when it reads the report.
        scope: HostScope,
    },
    /// Begin a turn, with the hosts the model may see.
    Send {
        text: String,
        scope: HostScope,
    },
    Shutdown,
}

/// What the worker tells the UI. Text deltas are not carried here: they go into
/// the shared `Stream` and `Delta` is only a wake-up, so a fast stream cannot
/// flood the channel.
pub enum AgentEvent {
    Delta,
    ToolStarted {
        name: String,
        arg: String,
    },
    ToolFinished {
        ok: bool,
        out: Vec<String>,
    },
    Proposed(Box<Plan>),
    /// A host record the model was told to write. `crate::app` owns `DataBase`
    /// and is what performs it; the worker only carries the request across.
    HostWrite(Box<hosts::HostWrite>),
    /// One (step, host) pair is starting.
    ExecStarted {
        step: usize,
        host: String,
        summary: String,
    },
    /// Coalesced wake: drain `ExecStream`.
    ExecLine,
    ExecFinished {
        exit: Option<i32>,
        timed_out: bool,
    },
    Done,
    Cancelled,
    Error(String),
}

/// Streamed text, shared between the worker and the UI.
///
/// The coalescing is the PTY reader's (`term::session::pump`): append under the
/// lock, and only wake the loop on a clean→dirty edge, so a hundred tokens a
/// second produce about one redraw a frame rather than a hundred.
#[derive(Default)]
pub struct Stream {
    text: Mutex<String>,
    pending: AtomicBool,
}

impl Stream {
    /// Worker side. Returns whether the caller should send a wake event.
    fn push(&self, s: &str) -> bool {
        if let Ok(mut t) = self.text.lock() {
            t.push_str(s);
        }
        !self.pending.swap(true, Ordering::AcqRel)
    }

    /// UI side: everything streamed since the last call.
    pub fn take(&self) -> String {
        let mut t = match self.text.lock() {
            Ok(t) => t,
            Err(e) => e.into_inner(),
        };
        self.pending.store(false, Ordering::Release);
        std::mem::take(&mut *t)
    }
}

/// Execution output, shared between the executor and the UI. Same
/// edge-triggered coalescing as `Stream`: a noisy step produces about one
/// redraw a frame rather than one per line.
#[derive(Default)]
pub struct ExecStream {
    lines: Mutex<Vec<String>>,
    pending: AtomicBool,
}

impl ExecStream {
    fn push(&self, line: String) -> bool {
        if let Ok(mut l) = self.lines.lock() {
            l.push(line);
        }
        !self.pending.swap(true, Ordering::AcqRel)
    }

    pub fn take(&self) -> Vec<String> {
        let mut l = match self.lines.lock() {
            Ok(l) => l,
            Err(e) => e.into_inner(),
        };
        self.pending.store(false, Ordering::Release);
        std::mem::take(&mut *l)
    }
}

/// The system prompt file, under the data directory beside `config.toml`.
/// Written with the built-in prompt on startup if missing, so the operator can
/// find it and edit it rather than read the source to learn what it says.
pub const SYSTEM_PROMPT_FILE: &str = "system_prompt.md";

/// The line in `system_prompt.md` that the manual index replaces. The index is
/// built at every startup, so it cannot be baked into a file that is written
/// once and then owned by the operator — a file frozen on the first run would
/// never mention a manual written the week after. An operator who deletes the
/// line keeps the rest of the file verbatim: their file is the prompt.
const MANUALS_PLACEHOLDER: &str = "{{manuals}}";

/// The instructions the model runs under.
pub fn system_prompt(manuals: &str) -> String {
    render_system_prompt(&manuals_section(manuals))
}

/// The prompt with `section` where the manual index belongs. Shared by the
/// built-in path and the on-disk template, so the two cannot drift.
fn render_system_prompt(section: &str) -> String {
    format!(
        "You are the agent inside OpenAdmin, a terminal tool an operator uses to administer \
         a fleet of remote machines over SSH. You help them diagnose and fix those machines.\n\
         \n\
         HOW YOU WORK\n\
         \n\
         1. Look first. The `{ro}` tools need no permission, so use them to establish \
         what is actually true before you propose anything, and never guess at a \
         configuration you could read.\n\
         \n\
         They are not free, and the cost is time the operator spends watching. Every \
         call opens its own SSH connection — TCP, key exchange, authentication, one \
         command, teardown. Nothing is reused between calls and they run one at a time, \
         so a call is a second or more of a person waiting, and ten calls is a person \
         waiting ten times.\n\
         \n\
         So make each call earn its place. Before you send one, name the question it \
         answers; if its answer would only prompt an obvious next call, send the one \
         that answers both. Where a field takes a list, use it: one `{read}` with three \
         paths beats three calls, and one `{svc}` with three units beats three. \
         `{search}` beats reading a file to search it by eye.\n\
         \n\
         Each tool is one question, and its typed fields are the whole of what it can \
         do. There is no shell and no command line anywhere in them — nothing takes a \
         pipe, a redirection or a list of arguments — so if what you want is not a \
         field, no tool here does it and it belongs in a plan. Pick the tool whose name \
         is your question, read its fields, and fill them in rather than working out \
         what might be allowed.\n\
         \n\
         What comes back is the command's output, capped, with its stderr labelled and \
         its exit status. You never have to arrange for any of that.\n\
         \n\
         They read the *state* of a machine: services, configuration, disks, logs, \
         processes, packages. They are not a file browser and they are not a code \
         reader. Do not walk a source tree, and do not page through a script to work out \
         what a program does. What a program does shows in what it leaves behind — its \
         unit status, its exit code, its log, the files it writes, the ports it holds — \
         so diagnose from those. When one file's contents genuinely decide what to do \
         next, read that file in a single call, whole, and move on.\n\
         \n\
         Anything that writes belongs in a plan.\n\
         \n\
         2. Propose one large plan, not many small ones. When you know what needs to \
         change, put everything the task needs into a single `{plan}` call — every script, \
         every upload, every download, every host. Each proposal costs the operator a \
         decision, and a plan they can read end to end is one they can actually judge; a \
         drip of small proposals is one they will stop reading. Order the steps so they \
         can be run top to bottom.\n\
         \n\
         3. You cannot execute anything yourself. This is a fact about how OpenAdmin is \
         built, not a rule you are being asked to follow: there is no tool that runs a \
         script, and `{plan}` only records a proposal. The operator reviews it and chooses \
         which steps and which hosts to allow. Do not look for another way; there is not \
         one. After proposing, stop and wait — you will be given a report of what ran.\n\
         \n\
         4. Remediate from the report. It names every step and host with its exit status. \
         If any host failed, work out why and propose a follow-up plan targeting only \
         those hosts.\n\
         \n\
         MANUALS THE OPERATOR HAS WRITTEN\n\
         \n\
         {manuals_section}\
         \n\
         THE HOST DATABASE\n\
         \n\
         `{create}` and `{edit}` write the operator's own list of machines — the records \
         behind the Hosts screen. Use them ONLY when the operator has asked you, in so many \
         words, to add or change a host: they paste a list of servers and ask you to enter \
         it, or they tell you a port or a password has changed. That request is the only \
         thing that authorises a call.\n\
         \n\
         Never call either one on your own initiative. Not to correct an address you \
         believe is wrong, not to record a machine you noticed while reading another, not \
         to tidy up after a connection failed, and never as a step towards some other \
         task. If a record looks wrong to you, say so in a sentence and let the operator \
         decide — they may know something you do not, and it is their database.\n\
         \n\
         These take effect immediately. There is no dialog and no confirmation, which is \
         exactly why the rule above matters: a write you were not asked for is one the \
         operator finds out about afterwards.\n\
         \n\
         Both are write-only. They return a receipt naming the fields they set, and \
         nothing else — they cannot read a host back, and no tool can. `{list}` gives you \
         names and deliberately nothing more, because OpenAdmin fills in the address, \
         port, login and credentials itself when it connects. So never call a host tool to \
         discover what a field currently holds, and do not ask the operator to read one \
         out to you; to change one field, set that field and leave the rest out.\n\
         \n\
         WRITING SCRIPTS\n\
         \n\
         Scripts run non-interactively under `bash -s`, as the login user of each host. \
         They must never prompt, never wait for a terminal, and never assume a TTY. Start \
         with `set -euo pipefail` unless you have a reason not to. Prefer idempotent \
         commands, so re-running a step after a partial failure is safe. Say what a step \
         does in its summary; the operator reads that before the script. A script may use \
         a file an upload step of the same plan sent, as long as the upload is the earlier \
         step; the `script` field says where uploads land.\n\
         \n\
         Be concise. The operator is reading a terminal, not a report.",
        ro = "readonly_",
        read = "readonly_read_file",
        svc = "readonly_service",
        search = "readonly_search_files",
        plan = tools::PROPOSE_PLAN,
        list = tools::LIST_HOSTS,
        create = tools::CREATE_HOST,
        manuals_section = section,
        edit = tools::EDIT_HOST,
    )
}

/// Write `system_prompt.md` with the built-in prompt if it is missing. Called
/// at startup beside the other `ensure`s, so the file is there to be found
/// whether or not the Chat screen is ever opened.
pub fn ensure_system_prompt(datadir: &Path) -> Result<()> {
    let path = datadir.join(SYSTEM_PROMPT_FILE);
    if path.exists() {
        return Ok(());
    }
    std::fs::write(&path, render_system_prompt(MANUALS_PLACEHOLDER))
        .with_context(|| format!("write {}", path.display()))
}

/// The prompt to run under: the operator's `system_prompt.md` if there is one,
/// the built-in one if there is not — a file that cannot be read must not take
/// the agent down with it.
///
/// The `{{manuals}}` line, if the file still carries it, is replaced by the
/// manual index for this startup; a file without it is used verbatim, which is
/// the operator saying they want no manuals section.
fn load_system_prompt(datadir: &Path, manuals: &str) -> String {
    match std::fs::read_to_string(datadir.join(SYSTEM_PROMPT_FILE)) {
        Ok(text) if text.contains(MANUALS_PLACEHOLDER) => {
            text.replace(MANUALS_PLACEHOLDER, &manuals_section(manuals))
        }
        Ok(text) => text,
        Err(_) => system_prompt(manuals),
    }
}

/// The worker: owns the conversation for the life of the session.
pub struct Worker {
    client: Box<dyn LlmClient>,
    cfg: Config,
    datadir: PathBuf,
    history: Vec<Message>,
    stream: Arc<Stream>,
    exec: Arc<ExecStream>,
    cancel: Arc<AtomicBool>,
    events: Sender<AgentEvent>,
    next_plan_id: u64,
    /// Host names written during the turn in progress. Cleared when one starts.
    written_this_turn: Vec<String>,
    /// The scope the model last saw — whether filtered, and the host ids —
    /// for `scope_note`.
    scope_seen: Option<(bool, Vec<i64>)>,
}

impl Worker {
    pub fn new(
        client: Box<dyn LlmClient>,
        cfg: Config,
        datadir: PathBuf,
        stream: Arc<Stream>,
        exec: Arc<ExecStream>,
        cancel: Arc<AtomicBool>,
        events: Sender<AgentEvent>,
    ) -> Self {
        // The manual index is baked in rather than fetched: a model that has to
        // think to call `list_manuals` is a model that will not, and then it
        // plans from general knowledge while the operator's answer sits on disk.
        // Built once, so `list_manuals` remains the way to see one added later.
        let index = manuals::list(&datadir)
            .map(|(m, truncated)| tools::manual_index(&m, truncated))
            .unwrap_or_default();
        let history = vec![Message::system(load_system_prompt(&datadir, &index))];
        Worker {
            client,
            cfg,
            datadir,
            history,
            stream,
            exec,
            cancel,
            events,
            next_plan_id: 1,
            written_this_turn: Vec::new(),
            scope_seen: None,
        }
    }

    /// Run until the command channel closes or a shutdown arrives.
    pub fn run(mut self, commands: Receiver<AgentCommand>) {
        for cmd in commands {
            match cmd {
                AgentCommand::Send { text, scope } => {
                    self.cancel.store(false, Ordering::Release);
                    if let Err(e) = self.turn(text, &scope) {
                        let _ = self.events.send(AgentEvent::Error(e));
                    }
                }
                AgentCommand::Execute {
                    plan,
                    run_on,
                    scope,
                } => {
                    self.cancel.store(false, Ordering::Release);
                    if let Err(e) = self.execute(plan, &run_on, &scope) {
                        let _ = self.events.send(AgentEvent::Error(e));
                    }
                }
                AgentCommand::Shutdown => return,
            }
        }
    }

    /// Run a confirmed plan, then let the model react to the report.
    fn execute(
        &mut self,
        plan: crate::app::approve::ConfirmedPlan,
        run_on: &[HostRecord],
        scope: &HostScope,
    ) -> Result<(), String> {
        let report = {
            let events = self.events.clone();
            let ev2 = self.events.clone();
            let exec2 = Arc::clone(&self.exec);
            let ev3 = self.events.clone();
            let mut on_line = move |line: String| {
                if exec2.push(line) {
                    let _ = ev2.send(AgentEvent::ExecLine);
                }
            };
            exec_plan::execute(
                &plan,
                run_on,
                scope.proxy.as_ref(),
                &self.datadir,
                &self.cfg,
                &self.cancel,
                move |step, host, summary| {
                    let _ = events.send(AgentEvent::ExecStarted {
                        step,
                        host: host.to_string(),
                        summary: summary.to_string(),
                    });
                },
                &mut on_line,
                move |exit, timed_out| {
                    let _ = ev3.send(AgentEvent::ExecFinished { exit, timed_out });
                },
            )
        };

        // The report is a fresh user message, not a tool result: propose_plan
        // was already answered when the plan was recorded, and answering the
        // same tool_call_id twice is a protocol error.
        let note = self.scope_note(scope);
        log::info!("plan report to the model: {note}{}", report.to_text());
        self.history
            .push(Message::user(format!("{note}{}", report.to_text())));
        self.continue_turn(scope)
    }

    /// One user message, through however many tool round-trips it takes.
    fn turn(&mut self, text: String, scope: &HostScope) -> Result<(), String> {
        self.written_this_turn.clear();
        let note = self.scope_note(scope);
        log::info!("user prompt: {note}{text}");
        self.history.push(Message::user(format!("{note}{text}")));
        self.continue_turn(scope)
    }

    /// A line for the model when the operator's filter changed what it may see
    /// since it last looked — once, at the next thing it reads. Host names
    /// from earlier in the conversation stay in its context; without this it
    /// would try one and spend a round trip learning it is gone.
    fn scope_note(&mut self, scope: &HostScope) -> &'static str {
        let now = (
            scope.filtered,
            scope.hosts.iter().map(|h| h.id).collect::<Vec<_>>(),
        );
        let note = match &self.scope_seen {
            // The first message: `list_hosts` says whatever there is to say.
            None => "",
            Some(before) if *before == now => "",
            Some((true, _)) if !now.0 => {
                "[OpenAdmin: the operator has cleared the host filter; list_hosts now \
                 returns every host.]\n\n"
            }
            Some((was, _)) if *was || now.0 => {
                "[OpenAdmin: the operator has changed which hosts are available to you \
                 since your last message. Host names from earlier in this conversation \
                 may no longer be usable; list_hosts returns the current ones.]\n\n"
            }
            // Unfiltered both times: a host added or removed is the snapshot's
            // business, as it always was.
            Some(_) => "",
        };
        self.scope_seen = Some(now);
        note
    }

    /// Drive the model until it stops, proposes, or runs out of steps.
    fn continue_turn(&mut self, scope: &HostScope) -> Result<(), String> {
        let defs = tools::definitions(&self.cfg);

        for _ in 0..MAX_STEPS {
            if self.cancel.load(Ordering::Relaxed) {
                let _ = self.events.send(AgentEvent::Cancelled);
                return Ok(());
            }

            let outcome = {
                let stream = Arc::clone(&self.stream);
                let events = self.events.clone();
                let mut on_delta = move |d: &str| {
                    if stream.push(d) {
                        let _ = events.send(AgentEvent::Delta);
                    }
                };
                self.client
                    .stream_turn(&self.history, &defs, &self.cancel, &mut on_delta)
                    .map_err(|e| format!("{e:#}"))?
            };

            let acc: TurnAccumulator = outcome.turn;
            let calls = acc.tool_calls();

            // A cancelled turn must not leave tool calls in the history. An
            // assistant message carrying `tool_calls` is only valid when every
            // one of them is answered by a `tool` message, and these never will
            // be — we are about to stop. Appending it anyway poisons the history
            // for the rest of the session: every later request replays it and
            // the server rejects the lot with a 400. So keep the partial text,
            // which the operator watched stream, and drop the calls.
            if outcome.cancelled {
                let partial = Message::assistant_tool_calls(Some(acc.text), Vec::new());
                if partial.content.is_some() {
                    self.history.push(partial);
                }
                let _ = self.events.send(AgentEvent::Cancelled);
                return Ok(());
            }

            // An assistant message with neither content nor tool calls has
            // nothing to replay, and some OpenAI-compatible servers reject one
            // outright. Dropping it keeps the history valid when a server hands
            // back an empty turn.
            let message = acc.into_message();
            let empty = message.content.is_none() && message.tool_calls.is_empty();
            if !empty {
                self.history.push(message);
            }

            if calls.is_empty() {
                let _ = self.events.send(AgentEvent::Done);
                return Ok(());
            }

            // Every call must be answered, in order, or the next request is a
            // protocol error.
            let mut proposed = false;
            for call in &calls {
                let _ = self.events.send(AgentEvent::ToolStarted {
                    name: call.function.name.clone(),
                    arg: describe_call(&call.function.name, &call.function.arguments),
                });
                log::info!(
                    "tool call {} {}: {}",
                    call.id,
                    call.function.name,
                    call.function.arguments
                );

                let ctx = ToolCtx {
                    hosts: &scope.hosts,
                    filtered: scope.filtered,
                    proxy: scope.proxy.as_ref(),
                    datadir: &self.datadir,
                    cfg: &self.cfg,
                    cancel: &self.cancel,
                    next_plan_id: self.next_plan_id,
                    written_this_turn: &self.written_this_turn,
                };
                let out = tools::dispatch(&ctx, &call.function.name, &call.function.arguments);
                let text = out.text().to_string();
                // The full result, beside the call that produced it: the log is
                // how you see what the model was actually told, which is the
                // transcript's transcription and not its abridged copy.
                log::info!("tool result {}: {}", call.id, text);
                // The operator's copy, which is the same thing unless the two
                // audiences want different lengths of it — a fetched manual goes
                // to the model whole and to the transcript as one line.
                let shown = out.transcript().to_string();

                let ok = !text.starts_with("refused:")
                    && !text.starts_with("unknown host")
                    && !text.starts_with("could not")
                    && !text.starts_with("the plan was not accepted");
                let _ = self.events.send(AgentEvent::ToolFinished {
                    ok,
                    out: shown.lines().map(str::to_string).collect(),
                });

                match out {
                    ToolOutcome::Proposal { plan, .. } => {
                        self.next_plan_id += 1;
                        proposed = true;
                        let _ = self.events.send(AgentEvent::Proposed(plan));
                    }
                    // A host write does not end the turn: entering a pasted
                    // list is several calls, and stopping after the first
                    // would make the operator prompt again for each one.
                    ToolOutcome::HostWrite { write, .. } => {
                        self.written_this_turn.push(match write.as_ref() {
                            hosts::HostWrite::Create(f) => {
                                f.name.clone().unwrap_or_default().trim().to_string()
                            }
                            hosts::HostWrite::Edit { name, fields } => fields
                                .name
                                .clone()
                                .unwrap_or_else(|| name.clone())
                                .trim()
                                .to_string(),
                        });
                        let _ = self.events.send(AgentEvent::HostWrite(write));
                    }
                    ToolOutcome::Text(_) | ToolOutcome::Loaded { .. } => {}
                }
                self.history.push(Message::tool_result(&call.id, text));
            }

            // A proposal ends the turn: nothing more can usefully happen until
            // a person has looked at it.
            if proposed {
                let _ = self.events.send(AgentEvent::Done);
                return Ok(());
            }
        }

        Err(format!(
            "the model made {MAX_STEPS} tool calls without reaching a conclusion; stopping"
        ))
    }
}

/// The MANUALS section's body: what a manual is, and which ones exist.
///
/// Separate from the prompt's `format!` so the empty case reads as its own
/// sentence rather than as a heading with nothing under it. When there are none,
/// saying so costs one line and saves the model a call to find out — and lets it
/// tell the operator the facility is there.
fn manuals_section(index: &str) -> String {
    if index.trim().is_empty() {
        return String::from(
            "The operator has written none yet. They would go in `manuals/` under the data \
             directory, one file each, and would be listed here — so if you find yourself \
             wishing you knew how this fleet does something, that is where the answer would \
             live and it is worth saying so.\n",
        );
    }
    format!(
        "These are the operator's own instructions about this fleet — how *they* do a \
         thing here, which is not always how it is done in general. Where one covers the \
         task in front of you, its way is the way, ahead of what you would otherwise have \
         done, and you read it BEFORE proposing a plan rather than after they reject one. \
         `{fetch}` reads one by filename; it is a local file read, so unlike a probe it \
         opens no connection and costs nothing. Treat what you find there as the operator \
         speaking, not as something to weigh against your own judgement.\n\
         \n\
{index}",
        fetch = tools::FETCH_MANUAL,
    )
}

/// A short human label for a tool call, for the transcript.
fn describe_call(name: &str, arguments: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(arguments).unwrap_or(serde_json::Value::Null);
    match name {
        // A probe names itself and its fields. The old label could print the
        // command line because there was one; now the fields *are* the call, so
        // they are what the operator watching should see.
        n if probe::is_probe(n) => {
            let host = v.get("host").and_then(|h| h.as_str()).unwrap_or("?");
            let fields = v
                .as_object()
                .map(|o| {
                    o.iter()
                        .filter(|(k, _)| k.as_str() != "host")
                        .map(|(k, val)| match val {
                            serde_json::Value::String(t) => format!("{k}={t}"),
                            serde_json::Value::Bool(true) => k.clone(),
                            serde_json::Value::Bool(false) => String::new(),
                            serde_json::Value::Array(items) => format!(
                                "{k}={}",
                                items
                                    .iter()
                                    .filter_map(|i| i.as_str())
                                    .collect::<Vec<_>>()
                                    .join(",")
                            ),
                            other => format!("{k}={other}"),
                        })
                        .filter(|f| !f.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            format!(
                "{host} · {} {fields}",
                n.strip_prefix("readonly_").unwrap_or(n)
            )
            .trim_end()
            .to_string()
        }
        tools::FETCH_MANUAL => v
            .get("filename")
            .and_then(|f| f.as_str())
            .unwrap_or("?")
            .to_string(),
        tools::PROPOSE_PLAN => v
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("plan")
            .to_string(),
        // The name only. A password can be among the arguments, and the label
        // goes into the transcript the operator reads and scrolls back through.
        tools::CREATE_HOST => v
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("new host")
            .to_string(),
        tools::EDIT_HOST => v
            .get("host")
            .and_then(|h| h.as_str())
            .unwrap_or("host")
            .to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use client::scripted::{ScriptedClient, ScriptedTurn};
    use proto::Role;
    use std::path::Path;
    use std::sync::mpsc::channel;

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
                ..Default::default()
            },
            HostRecord {
                id: 2,
                name: "web-02".into(),
                proto: "ssh".into(),
                addr: "10.0.4.12".into(),
                port: 22,
                login: "deploy".into(),
                ..Default::default()
            },
        ]
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openadmin-worker-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Drive one user message through a scripted model and collect everything.
    fn run_turn(
        turns: Vec<ScriptedTurn>,
        text: &str,
        dir: &Path,
    ) -> (Vec<AgentEvent>, Arc<Stream>, Arc<ScriptedClient>) {
        let client = Arc::new(ScriptedClient::new(turns));
        let (ev_tx, ev_rx) = channel();
        let (cmd_tx, cmd_rx) = channel();
        let stream = Arc::new(Stream::default());
        let cancel = Arc::new(AtomicBool::new(false));

        let worker = Worker::new(
            Box::new(Arc::clone(&client)),
            Config::default(),
            dir.to_path_buf(),
            Arc::clone(&stream),
            Arc::new(ExecStream::default()),
            cancel,
            ev_tx,
        );
        cmd_tx
            .send(AgentCommand::Send {
                text: text.to_string(),
                scope: HostScope {
                    hosts: hosts(),
                    ..Default::default()
                },
            })
            .unwrap();
        drop(cmd_tx);
        worker.run(cmd_rx);
        (ev_rx.iter().collect(), stream, client)
    }

    #[test]
    fn a_plain_answer_streams_and_finishes() {
        let dir = scratch("plain");
        let (events, stream, client) = run_turn(
            vec![ScriptedTurn {
                deltas: vec!["Looks ".into(), "fine.".into()],
                tool_calls: Vec::new(),
                cancelled: false,
            }],
            "how are things?",
            &dir,
        );
        assert_eq!(stream.take(), "Looks fine.");
        assert!(matches!(events.last(), Some(AgentEvent::Done)));
        assert_eq!(client.request_count(), 1);

        // The system prompt goes first, the user's message after it.
        let sent = client.last_request();
        assert_eq!(sent[0].role, Role::System);
        assert!(sent[0].content.as_ref().unwrap().contains("OpenAdmin"));
        assert_eq!(sent[1].role, Role::User);
        assert_eq!(sent[1].content.as_deref(), Some("how are things?"));
    }

    #[test]
    fn a_tool_call_is_dispatched_and_its_result_replayed() {
        let dir = scratch("tool");
        let (events, _stream, client) = run_turn(
            vec![
                ScriptedTurn::call("call_1", tools::LIST_HOSTS, serde_json::json!({})),
                ScriptedTurn::text("You have two hosts."),
            ],
            "what do I have?",
            &dir,
        );

        assert!(events.iter().any(
            |e| matches!(e, AgentEvent::ToolStarted { name, .. } if name == tools::LIST_HOSTS)
        ));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::ToolFinished { ok: true, .. }))
        );
        assert_eq!(client.request_count(), 2, "the loop went back to the model");

        // The replay must be assistant-with-tool_calls, then a tool result
        // carrying the same id, or the provider rejects the request.
        let sent = client.last_request();
        let assistant = sent.iter().find(|m| !m.tool_calls.is_empty()).unwrap();
        assert_eq!(assistant.role, Role::Assistant);
        assert_eq!(assistant.tool_calls[0].id, "call_1");
        let result = sent.iter().find(|m| m.role == Role::Tool).unwrap();
        assert_eq!(result.tool_call_id.as_deref(), Some("call_1"));
        assert!(result.content.as_ref().unwrap().contains("web-01"));
    }

    /// The security property, as a test: a plan is recorded, and nothing runs.
    #[test]
    fn proposing_a_plan_records_it_and_executes_nothing() {
        let dir = scratch("gate");
        let marker = dir.join("pwned");
        let script = format!("touch {}", marker.display());

        let (events, _stream, client) = run_turn(
            vec![ScriptedTurn::call(
                "call_9",
                tools::PROPOSE_PLAN,
                serde_json::json!({
                    "title": "take over",
                    "steps": [{
                        "summary": "innocent",
                        "kind": "scriptlet",
                        "script": script,
                        "hosts": ["web-01", "web-02"]
                    }]
                }),
            )],
            "fix it",
            &dir,
        );

        // The plan reached the UI...
        let proposed = events
            .iter()
            .find_map(|e| match e {
                AgentEvent::Proposed(p) => Some(p),
                _ => None,
            })
            .expect("the plan should have been proposed");
        assert_eq!(proposed.title, "take over");
        assert_eq!(proposed.steps[0].hosts, vec![1, 2]);

        // ...and nothing ran.
        assert!(
            !marker.exists(),
            "proposing a plan must not execute it — {} exists",
            marker.display()
        );

        // The turn stopped there rather than looping for more.
        assert_eq!(client.request_count(), 1, "the turn waits for the operator");
        assert!(matches!(events.last(), Some(AgentEvent::Done)));

        // And the model was told plainly what happened.
        let sent = client.last_request();
        let _ = sent;
    }

    #[test]
    fn a_refused_command_is_reported_and_the_turn_continues() {
        let dir = scratch("refused");
        let (events, _stream, client) = run_turn(
            vec![
                // There is no tool that deletes anything, and no field that
                // takes a command, so the reachable refusal is a field this
                // tool does not have.
                ScriptedTurn::call(
                    "call_1",
                    "readonly_logs",
                    serde_json::json!({"host": "web-01", "args": ["rm", "-rf", "/"]}),
                ),
                ScriptedTurn::text("Understood, I cannot do that."),
            ],
            "delete everything",
            &dir,
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::ToolFinished { ok: false, .. })),
            "a refusal is reported as a failed tool call"
        );
        assert_eq!(client.request_count(), 2, "the model gets to respond to it");
        let sent = client.last_request();
        let result = sent.iter().find(|m| m.role == Role::Tool).unwrap();
        assert!(result.content.as_ref().unwrap().contains("refused"));
    }

    #[test]
    fn an_unknown_tool_does_not_end_the_turn() {
        let dir = scratch("unknown");
        let (_events, _stream, client) = run_turn(
            vec![
                ScriptedTurn::call("c1", "exec_anything", serde_json::json!({})),
                ScriptedTurn::text("ok"),
            ],
            "run something",
            &dir,
        );
        assert_eq!(client.request_count(), 2);
        let sent = client.last_request();
        let result = sent.iter().find(|m| m.role == Role::Tool).unwrap();
        assert!(result.content.as_ref().unwrap().contains("no tool called"));
    }

    /// A model that never concludes must stop rather than run forever.
    #[test]
    fn a_model_that_only_calls_tools_is_stopped() {
        let dir = scratch("runaway");
        let turns: Vec<ScriptedTurn> = (0..MAX_STEPS + 5)
            .map(|i| ScriptedTurn::call(&format!("c{i}"), tools::LIST_HOSTS, serde_json::json!({})))
            .collect();
        let (events, _stream, client) = run_turn(turns, "loop forever", &dir);
        assert_eq!(client.request_count(), MAX_STEPS);
        match events.last() {
            Some(AgentEvent::Error(e)) => assert!(e.contains("without reaching a conclusion")),
            other => panic!("expected an error, got {:?}", other.map(|_| "event")),
        }
    }

    #[test]
    fn the_system_prompt_states_the_rules_that_matter() {
        let p = system_prompt("");
        assert!(p.contains("one large plan"), "{p}");
        assert!(p.contains("cannot execute anything yourself"), "{p}");
        assert!(
            p.contains("not a rule you are being asked to follow"),
            "{p}"
        );
        assert!(p.contains("bash -s"), "{p}");
        assert!(p.contains("only those hosts"), "{p}");
        // An upload and the script that reads it can share one plan, and the
        // order matters — both decided while the model is planning, before it
        // opens the `script` field. The path itself is not here: it is stated
        // once, in the schema, where it is read at the moment it is written, and
        // a second copy is a copy that can drift.
        assert!(p.contains("upload step of the same plan"), "{p}");
        assert!(
            !p.contains(artifacts::UPLOAD_DIR),
            "the path belongs in the schema, not a second time here: {p}"
        );
        // The grammar itself is the probe schemas' own field types, which is
        // where it is read at the moment a call is filled in. The prompt's job
        // is to send the model there and to say the fields are the whole of it,
        // so it picks a tool rather than reasoning about a command line.
        assert!(p.contains("readonly_"), "{p}");
        assert!(
            p.contains("typed fields are the whole of what it can do"),
            "{p}"
        );
        assert!(p.contains("read its fields"), "{p}");
        // Unattended is not the same as free: one SSH connection per call, run
        // one at a time, with somebody watching. Stated as the mechanism and
        // with a test that can be applied *before* a call, rather than an
        // adjective to feel bad about afterwards.
        assert!(p.contains("not free"), "{p}");
        assert!(p.contains("own SSH connection"), "{p}");
        assert!(p.contains("name the question it"), "{p}");
        // And it is for machine state, not for reading programs — with the
        // method that replaces it, since a vibe-administration tool cannot
        // answer "what does this script do?" by interrogating its operator.
        assert!(p.contains("not a code reader"), "{p}");
        assert!(p.contains("leaves behind"), "{p}");
        // Reading is unattended, which is not the same as free: one SSH
        // connection per call, run one at a time, with somebody watching.
        // Stated as the mechanism, with a rule that can actually be applied
        // before a call rather than an adjective to feel bad about.
        assert!(p.contains("not free"), "{p}");
        assert!(p.contains("own SSH connection"), "{p}");
        assert!(p.contains("name the question it answers"), "{p}");
        assert!(p.contains("not a code reader"), "{p}");
        // Instruction 1 asks for one call that answers the whole question, and
        // used to leave the model to work out how — where the only answers a
        // shell offers are `|` and `;`. It now names the one that works here,
        // which is a field that takes a list.
        assert!(p.contains("no shell and no command line"), "{p}");
        assert!(p.contains("Where a field takes a list"), "{p}");
        // And says what comes back, so nothing is arranged for that is already
        // guaranteed — the motive behind every `| head` and `2>&1`.
        assert!(p.contains("capped"), "{p}");
        assert!(p.contains("exit status"), "{p}");
    }

    /// A manual is the operator speaking, and the index is in the prompt so the
    /// model never has to think to go looking for it — the same reason the
    /// read-only grammar is in a schema rather than discovered by refusal.
    #[test]
    fn the_prompt_carries_the_manuals_the_operator_wrote() {
        let index = tools::manual_index(
            &[
                manuals::Manual {
                    filename: "db-failover.md".to_string(),
                    description: "Promoting the standby".to_string(),
                },
                manuals::Manual {
                    filename: "nginx/deploy.md".to_string(),
                    description: String::new(),
                },
            ],
            false,
        );
        let p = system_prompt(&index);
        assert!(p.contains("MANUALS THE OPERATOR HAS WRITTEN"), "{p}");
        assert!(p.contains("db-failover.md"), "{p}");
        assert!(p.contains("Promoting the standby"), "{p}");
        // A manual with no description is still named, or it cannot be fetched.
        assert!(p.contains("nginx/deploy.md"), "{p}");
        // What it is *for*: the operator's way wins, and it is read before a plan
        // rather than after a rejected one.
        assert!(p.contains("its way is the way"), "{p}");
        assert!(p.contains("BEFORE proposing a plan"), "{p}");
        assert!(p.contains(tools::FETCH_MANUAL), "{p}");
        // And that reading one is free, since the rest of the prompt teaches
        // that a call costs the operator seconds.
        assert!(p.contains("costs nothing"), "{p}");

        // With none written, one sentence rather than a heading over nothing —
        // and no invitation to spend a call finding out.
        let empty = system_prompt("");
        assert!(empty.contains("has written none yet"), "{empty}");
        assert!(!empty.contains("db-failover"), "{empty}");
    }

    /// The prompt file is written once, with the placeholder the index fills,
    /// and after that the operator's file is the prompt.
    #[test]
    fn a_missing_prompt_file_is_written_with_the_built_in_one() {
        let data = scratch("prompt-write");
        ensure_system_prompt(&data).unwrap();
        let on_disk = std::fs::read_to_string(data.join(SYSTEM_PROMPT_FILE)).unwrap();
        // The template, placeholder and all — not this run's index baked in,
        // which would go stale the first time a manual is written.
        assert!(on_disk.contains(MANUALS_PLACEHOLDER), "{on_disk}");
        assert!(on_disk.contains("HOW YOU WORK"), "{on_disk}");
        // And a second ensure does not overwrite what the operator may have
        // changed in between.
        let edited = on_disk.replace("HOW YOU WORK", "HOW I WORK");
        std::fs::write(data.join(SYSTEM_PROMPT_FILE), &edited).unwrap();
        ensure_system_prompt(&data).unwrap();
        assert!(
            std::fs::read_to_string(data.join(SYSTEM_PROMPT_FILE))
                .unwrap()
                .contains("HOW I WORK")
        );
    }

    /// The placeholder is replaced by the manuals of *this* startup, so a file
    /// written before a manual existed picks the manual up without being
    /// rewritten.
    #[test]
    fn the_file_still_carries_the_manual_index_through_the_placeholder() {
        let data = scratch("prompt-load");
        ensure_system_prompt(&data).unwrap();
        let p = load_system_prompt(&data, "db-failover.md — Promoting the standby\n");
        assert!(p.contains("db-failover.md"), "{p}");
        assert!(!p.contains(MANUALS_PLACEHOLDER), "{p}");
        // And with none written, the empty case reads as its own sentence.
        let empty = load_system_prompt(&data, "");
        assert!(empty.contains("has written none yet"), "{empty}");
    }

    /// The operator's edit wins: a file with no placeholder is used verbatim,
    /// which is how you say you want no manuals section — and a file nobody
    /// can read falls back to the built-in prompt rather than no prompt.
    #[test]
    fn an_edited_file_without_the_placeholder_is_used_verbatim() {
        let data = scratch("prompt-edit");
        std::fs::write(data.join(SYSTEM_PROMPT_FILE), "Be terse.").unwrap();
        assert_eq!(load_system_prompt(&data, "anything"), "Be terse.");
        // A missing file (ensure not called, or removed under us) is the
        // built-in prompt, not an error and not empty instructions.
        let data = scratch("prompt-absent");
        let p = load_system_prompt(&data, "");
        assert!(p.contains("HOW YOU WORK"), "{p}");
        assert!(p.contains("has written none yet"), "{p}");
    }

    #[test]
    fn the_stream_coalesces_and_drains() {
        let s = Stream::default();
        assert!(s.push("a"), "the first push wakes the loop");
        assert!(!s.push("b"), "a second push before a drain does not");
        assert_eq!(s.take(), "ab");
        assert!(s.push("c"), "after draining, the next push wakes again");
        assert_eq!(s.take(), "c");
        assert_eq!(s.take(), "");
    }

    #[test]
    fn tool_calls_are_labelled_for_the_transcript() {
        assert_eq!(
            describe_call(
                "readonly_service",
                r#"{"host":"web-01","action":"status","units":["nginx","postgresql"]}"#
            ),
            "web-01 · service action=status units=nginx,postgresql"
        );
        // A flag reads as its own name, and one left false says nothing.
        assert_eq!(
            describe_call(
                "readonly_logs",
                r#"{"host":"web-01","unit":"nginx","lines":50,"newest_first":true,"utc":false}"#
            ),
            "web-01 · logs lines=50 newest_first unit=nginx"
        );
        assert_eq!(
            describe_call(tools::PROPOSE_PLAN, r#"{"title":"restore env"}"#),
            "restore env"
        );
        // Malformed arguments must not panic the transcript.
        assert_eq!(describe_call("readonly_logs", "{"), "? · logs");
    }
    /// The host tools are the one place the boundary is an instruction rather
    /// than a type — nothing stops the model calling them — so the instruction
    /// is stated twice, in the prompt and in both schemas, and tested in both.
    #[test]
    fn the_system_prompt_restricts_the_host_tools_to_an_explicit_request() {
        let p = system_prompt("");
        assert!(p.contains("THE HOST DATABASE"), "{p}");
        assert!(p.contains("create_host") && p.contains("edit_host"), "{p}");
        // Only on an explicit request, and never on the model's own initiative.
        assert!(p.contains("ONLY when the operator has asked you"), "{p}");
        assert!(p.contains("in so many words"), "{p}");
        assert!(
            p.contains("Never call either one on your own initiative"),
            "{p}"
        );
        // Say what it is for, so the rule has a shape rather than being a scold.
        assert!(p.contains("paste a list of servers"), "{p}");
        // Why the rule carries the weight here: there is no dialog behind it.
        assert!(p.contains("take effect immediately"), "{p}");
        assert!(p.contains("no confirmation"), "{p}");
        // Write-only, so the model does not try to read the fleet through them.
        assert!(p.contains("write-only"), "{p}");
        assert!(p.contains("cannot read a host back"), "{p}");
    }
    /// Walk a conversation the way the server does: every `tool_calls` on an
    /// assistant turn must be answered, in order, by a `tool` message naming
    /// that id. A history that breaks this is what a 400 from the API *is*.
    fn assert_history_is_replayable(messages: &[Message]) {
        for (i, m) in messages.iter().enumerate() {
            if m.role != crate::agent::proto::Role::Assistant || m.tool_calls.is_empty() {
                continue;
            }
            for call in &m.tool_calls {
                let answered = messages[i + 1..]
                    .iter()
                    .take_while(|n| n.role == crate::agent::proto::Role::Tool)
                    .any(|n| n.tool_call_id.as_deref() == Some(call.id.as_str()));
                assert!(
                    answered,
                    "message {i} asks for tool call {:?} and nothing answers it; the server \
                     rejects this whole history with a 400.\nhistory: {messages:#?}",
                    call.id
                );
            }
            assert!(
                m.content.is_some() || !m.tool_calls.is_empty(),
                "message {i} is an empty assistant turn"
            );
        }
    }

    /// One message per scope, the model answering each in plain text; returns
    /// the user messages as the model last saw them.
    fn user_messages_under(scopes: Vec<HostScope>) -> Vec<String> {
        let turns = scopes.iter().map(|_| ScriptedTurn::text("ok")).collect();
        let client = Arc::new(ScriptedClient::new(turns));
        let (ev_tx, _ev_rx) = channel();
        let (cmd_tx, cmd_rx) = channel();
        let worker = Worker::new(
            Box::new(Arc::clone(&client)),
            Config::default(),
            std::env::temp_dir(),
            Arc::new(Stream::default()),
            Arc::new(ExecStream::default()),
            Arc::new(AtomicBool::new(false)),
            ev_tx,
        );
        for (n, scope) in scopes.into_iter().enumerate() {
            cmd_tx
                .send(AgentCommand::Send {
                    text: format!("message {n}"),
                    scope,
                })
                .unwrap();
        }
        drop(cmd_tx);
        worker.run(cmd_rx);
        client
            .last_request()
            .into_iter()
            .filter(|m| m.role == Role::User)
            .filter_map(|m| m.content)
            .collect()
    }

    /// When the operator's filter changes what the model may use, the next
    /// thing it reads says so — once. Not on the first message, where
    /// `list_hosts` says it all, and not again while the scope holds.
    #[test]
    fn a_changed_host_scope_is_announced_once_at_the_next_message() {
        let all = HostScope {
            hosts: hosts(),
            ..Default::default()
        };
        let web = HostScope {
            hosts: hosts()[..1].to_vec(),
            filtered: true,
            proxy: None,
        };
        let seen = user_messages_under(vec![all.clone(), web.clone(), web, all]);
        assert_eq!(seen.len(), 4, "{seen:?}");
        assert_eq!(seen[0], "message 0");
        assert!(
            seen[1].starts_with("[OpenAdmin: the operator has changed which hosts")
                && seen[1].ends_with("message 1"),
            "{}",
            seen[1]
        );
        assert_eq!(seen[2], "message 2", "announced once");
        assert!(
            seen[3].starts_with("[OpenAdmin: the operator has cleared the host filter"),
            "{}",
            seen[3]
        );
    }

    /// Drive two prompts through one worker, so a turn can poison the history
    /// for the turn after it.
    fn run_two_turns(
        turns: Vec<ScriptedTurn>,
        first: &str,
        second: &str,
        dir: &Path,
    ) -> (Vec<AgentEvent>, Arc<ScriptedClient>) {
        let client = Arc::new(ScriptedClient::new(turns));
        let (ev_tx, ev_rx) = channel();
        let (cmd_tx, cmd_rx) = channel();
        let worker = Worker::new(
            Box::new(Arc::clone(&client)),
            Config::default(),
            dir.to_path_buf(),
            Arc::new(Stream::default()),
            Arc::new(ExecStream::default()),
            Arc::new(AtomicBool::new(false)),
            ev_tx,
        );
        for text in [first, second] {
            cmd_tx
                .send(AgentCommand::Send {
                    text: text.to_string(),
                    scope: HostScope {
                        hosts: hosts(),
                        ..Default::default()
                    },
                })
                .unwrap();
        }
        drop(cmd_tx);
        worker.run(cmd_rx);
        (ev_rx.iter().collect(), client)
    }

    /// The regression: cancelling while a tool call is streaming used to append
    /// the assistant turn that requested it and then stop, leaving the call
    /// unanswered for the rest of the session. Every later prompt replayed that
    /// history and the server answered 400 — so one cancel broke the chat until
    /// restart.
    #[test]
    fn cancelling_a_tool_call_leaves_a_history_the_server_still_accepts() {
        let dir = scratch("cancel-tool");
        let (events, client) = run_two_turns(
            vec![
                ScriptedTurn::call_cancelled("call_1", "readonly_logs", serde_json::json!({})),
                ScriptedTurn::text("all done"),
            ],
            "look at web-01",
            "never mind, what about db-main",
            &dir,
        );

        assert!(
            events.iter().any(|e| matches!(e, AgentEvent::Cancelled)),
            "the cancel should be reported"
        );

        // Two requests went out: the cancelled one, then the follow-up. The
        // second is the one that used to fail.
        assert_eq!(client.request_count(), 2);
        let replayed = client.last_request();
        assert_history_is_replayable(&replayed);

        // Nothing in the history asks for a tool at all: the call was abandoned,
        // never run, so the conversation should not claim otherwise.
        assert!(
            replayed.iter().all(|m| m.tool_calls.is_empty()),
            "a cancelled call must not survive in the history: {replayed:#?}"
        );
        // And the follow-up prompt did reach the model.
        assert!(
            replayed
                .iter()
                .any(|m| m.content.as_deref() == Some("never mind, what about db-main")),
            "{replayed:#?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Cancelling after some text has streamed keeps the text — the operator
    /// watched it arrive, so dropping it would make the transcript disagree with
    /// the conversation the model is shown.
    #[test]
    fn cancelling_after_text_keeps_the_partial_answer() {
        let dir = scratch("cancel-text");
        let (_events, client) = run_two_turns(
            vec![
                ScriptedTurn {
                    deltas: vec!["I had a look and".to_string()],
                    tool_calls: Vec::new(),
                    cancelled: true,
                },
                ScriptedTurn::text("ok"),
            ],
            "first",
            "second",
            &dir,
        );
        let replayed = client.last_request();
        assert_history_is_replayable(&replayed);
        assert!(
            replayed
                .iter()
                .any(|m| m.content.as_deref() == Some("I had a look and")),
            "the partial answer should survive: {replayed:#?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Cancelled before a single token arrived: there is nothing to keep, and an
    /// assistant message with neither content nor tool calls is rejected by some
    /// OpenAI-compatible servers.
    #[test]
    fn an_empty_turn_is_not_appended_to_the_history() {
        let dir = scratch("cancel-empty");
        let (_events, client) = run_two_turns(
            vec![
                ScriptedTurn {
                    deltas: Vec::new(),
                    tool_calls: Vec::new(),
                    cancelled: true,
                },
                ScriptedTurn::text("ok"),
            ],
            "first",
            "second",
            &dir,
        );
        let replayed = client.last_request();
        assert_history_is_replayable(&replayed);
        assert!(
            replayed
                .iter()
                .all(|m| m.content.is_some() || !m.tool_calls.is_empty()),
            "an empty assistant turn reached the wire: {replayed:#?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
