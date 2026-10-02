//! Application state and event routing.
//!
//! Structure follows `/root/cfdns/src/app/mod.rs`: a flat `Mode` enum (no
//! screen stack), a `Regions` hit-test table repopulated by the render pass,
//! and a `Click` enum whose dominant variant synthesizes a key event so the
//! mouse reuses the keyboard handlers verbatim.

pub mod approve;
pub mod bulk;
pub mod chat;
pub mod form;
pub mod mount_job;

use crate::agent::hosts::HostWrite;
use crate::agent::plan::Plan;
use crate::agent::{AgentCommand, AgentEvent, ExecStream, HostScope, Stream, Worker};
use crate::config::Config;
use crate::db::DataBase;
use crate::db::model::{HostRecord, mount_for};
use crate::term::manager::TerminalManager;
use crate::term::session::{SessionId, Spawn, TermEvent};
use crate::ui::widgets::ScrollGeometry;
use crate::{keys, mount, mtab, ssh};
use approve::{ConfirmedPlan, PlanSelection};
use bulk::{BulkImport, BulkStep};
use chat::{ChatState, PlanState};
use form::{FormField, FormState};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};
use tui_input::Input;
use tui_input::backend::crossterm::to_input_request;

/// Whether `h` is one the filter `needle_lc` (already lowercased) lists: its
/// name or its address contains it, in any case.
fn host_matches(h: &HostRecord, needle_lc: &str) -> bool {
    h.name.to_lowercase().contains(needle_lc) || h.addr.to_lowercase().contains(needle_lc)
}

/// How long a transient status message stays before reverting.
const STATUS_REVERT: Duration = Duration::from_millis(3400);
const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const SPINNER_TICK: Duration = Duration::from_millis(80);
/// The function-bar code for the Shell cap.
///
/// Shell moved from F5 to Enter, and Enter has no F-number for the bar to
/// dispatch. Outside the range any real key produces — F-keys are 1…24 and
/// `Alt`+digit 0…9 — so it can only ever arrive from a click on that cap.
pub const BAR_SHELL: u8 = 0xfe;

const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// What the body shows. `Shells` shows the manager's active shell tab, so
/// there is one `Shells` screen however many shells are open — the header
/// strip is where they are told apart (`Tab`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Hosts,
    Chat,
    Shells,
}

/// One entry of the header's tab strip: Hosts, Chat, then one per open shell
/// tab, in that order. `Alt`+digit and `Alt+←/→` count along this, so a shell
/// is reached the same way as a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Hosts,
    Chat,
    /// An index into `TerminalManager::tabs`.
    Shell(usize),
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::Hosts => "Hosts",
            Tab::Chat => "Agent",
            Tab::Shell(_) => "Shell",
        }
    }
}

/// Modal state, orthogonal to which screen is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// Reviewing a proposed plan. The only mode that can authorize execution.
    ConfirmPlan,
    HostForm,
    ConfirmDelete,
    ShowKey,
    Help,
    /// sshfs is connecting. Modal: the only thing to do is wait or cancel.
    Mounting,
    /// Pasting a qhostman host list, then reviewing what it would add. Opened
    /// over the Add dialog, which stays in `form` to come back to.
    BulkImport,
    /// Typing the Hosts screen's filter (Ctrl+F).
    Filter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Idle,
    Ok,
    Warn,
    Loading,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub text: String,
    pub kind: StatusKind,
    pub set_at: Instant,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            text: "Alt+1…9 switch tabs · Insert marks hosts · F1 help".to_string(),
            kind: StatusKind::Idle,
            set_at: Instant::now(),
        }
    }
}

/// A dialog control's click action. Most route through `on_key` so buttons need
/// no duplicate logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    Key(KeyCode),
    /// Indices into the plan dialog. `Click` is `Copy`, so hitboxes carry
    /// positions rather than owned data.
    ToggleStep(u16),
    ToggleHost(u16, u16),
    FocusField(FormField),
    CycleType(i32),
    /// One of the host form's buttons. Named rather than sent as its key: ↵
    /// presses whichever button has focus, so a click on Add Host sent as ↵
    /// would press Cancel if Cancel had it.
    FormButton(FormField),
    /// The error dialog's button.
    Dismiss,
}

/// Which list a scrollbar scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    /// The Known Hosts list.
    Hosts,
    /// A shell pane's history.
    Pane(SessionId),
}

/// A scrollbar held down: which one, and where on its thumb it was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScrollDrag {
    target: ScrollTarget,
    /// Rows from the thumb's top to the pointer, kept while dragging so the
    /// thumb does not jump to meet it.
    grab: i32,
    /// The thumb row last applied. A drag within the same row does nothing:
    /// a row stands for many positions, and re-resolving it would nudge the
    /// view while the pointer has not moved a row.
    row: i32,
}

/// Clickable screen regions, recaptured every render.
#[derive(Default, Clone)]
pub struct Regions {
    /// Host table data rows, and the host index drawn on the first of them.
    pub rows: Rect,
    pub row_start: usize,
    /// The header's tabs: Hosts, Chat and every shell tab drawn.
    pub screen_tabs: Vec<(Rect, Tab)>,
    /// Function-bar row and the `(x_start, x_end, fkey)` of each cap.
    pub fn_bar_y: u16,
    pub fkeys: Vec<(u16, u16, u8)>,
    /// Shell tabs' `×` buttons, by shell tab index.
    pub shell_closes: Vec<(Rect, usize)>,
    /// Terminal panes of the active tab, by pane index.
    pub panes: Vec<(Rect, usize)>,
    /// The pending plan's card in the transcript, while it is on screen.
    pub plan_card: Option<Rect>,
    /// The transcript's frame — the Agent box, composer excluded. The plan
    /// dialog covers exactly this, so the draft stays in sight beneath it.
    pub chat_log: Option<Rect>,
    /// Dialog controls, captured per render.
    pub clicks: Vec<(Rect, Click)>,
    /// Scrollbars drawn this frame: the bar's band, and what it scrolls.
    pub scrollbars: Vec<(Rect, ScrollTarget)>,
}

impl Regions {
    /// Cleared at the top of every frame; the renderers refill it.
    pub fn clear(&mut self) {
        self.screen_tabs.clear();
        // u16::MAX is off-screen, so a screen that draws no function bar
        // cannot inherit the previous one's row.
        self.fn_bar_y = u16::MAX;
        self.fkeys.clear();
        self.shell_closes.clear();
        self.panes.clear();
        self.plan_card = None;
        self.chat_log = None;
        self.clicks.clear();
        self.scrollbars.clear();
    }
}

/// A running agent worker, when a model is configured.
pub struct AgentHandle {
    tx: Sender<AgentCommand>,
    /// Streamed text, drained on every `Delta`.
    stream: Arc<Stream>,
    /// Execution output, drained on every `ExecLine`.
    exec: Arc<ExecStream>,
    /// Raised to stop an in-flight turn; the worker polls it.
    cancel: Arc<AtomicBool>,
}

pub struct App {
    pub screen: Screen,
    /// The screen that was showing before the Shells screen was, so closing
    /// the last shell can go back there. Never `Shells` itself.
    pub prev_screen: Screen,
    pub mode: Mode,

    pub hosts: Vec<HostRecord>,
    pub cursor: usize,
    pub scroll: usize,
    pub marked: HashSet<i64>,
    /// The Hosts screen's filter: only hosts whose name or address contains it
    /// are listed, acted on — and available to the agent. `None` shows all.
    /// Only Ctrl+F then Enter, and Esc, change it: it is the agent's scope, so
    /// nothing clears it as a side effect. Never saved.
    pub filter: Option<String>,
    /// The filter being typed, while `Mode::Filter` shows it.
    pub filter_edit: Option<Input>,

    pub form: Option<FormState>,
    pub pending_delete: Vec<i64>,
    /// The mount in progress, while `Mode::Mounting` shows it.
    pub mounting: Option<mount_job::MountJob>,
    /// `(host name, public key)` for the SSH Public Key dialog.
    pub key_dialog: Option<(String, String)>,
    pub alert: Option<String>,
    /// The bulk import in progress, while `Mode::BulkImport` shows it.
    pub bulk: Option<BulkImport>,

    pub chat: ChatState,
    /// `None` until a model is configured.
    pub agent: Option<AgentHandle>,
    /// Where agent events are sent, so a worker can be started later.
    agent_events: Sender<AgentEvent>,
    /// A proposal waiting for the operator. It suspends the turn, so the
    /// dialog shows itself as soon as that is free (`maybe_auto_open_plan`).
    pub pending_plan: Option<Plan>,
    /// The dialog's state. Outlives `Mode::ConfirmPlan`, so a plan put away
    /// with `F2` comes back with its checkboxes intact.
    pub plan: Option<PlanSelection>,
    /// Set by `F2` while the dialog is up: the operator has seen it and wants
    /// it out of the way, so it must not spring back on the next frame.
    pub plan_hidden: bool,
    pub term: TerminalManager,

    pub status: Status,
    pub spinner_frame: usize,
    pub last_spin: Instant,
    pub busy: bool,

    pub hover: Option<Position>,
    pub regions: Regions,
    last_click: Option<(Instant, u16)>,
    /// A scrollbar being dragged, from the press on it to the release.
    scroll_drag: Option<ScrollDrag>,

    /// The `ssh -D` tunnel backing the host flagged as proxy. Without it the
    /// injected ProxyCommand would point at a port nobody is listening on.
    proxy_tunnel: Option<std::process::Child>,

    pub db: DataBase,
    pub cfg: Config,
    pub datadir: PathBuf,
    pub term_tx: Sender<TermEvent>,
    pub should_quit: bool,
}

impl App {
    pub fn new(
        db: DataBase,
        cfg: Config,
        datadir: PathBuf,
        term_tx: Sender<TermEvent>,
        agent_events: Sender<AgentEvent>,
    ) -> Self {
        let mut app = App {
            screen: Screen::Hosts,
            prev_screen: Screen::Hosts,
            mode: Mode::Normal,
            hosts: Vec::new(),
            cursor: 0,
            scroll: 0,
            marked: HashSet::new(),
            filter: None,
            filter_edit: None,
            form: None,
            pending_delete: Vec::new(),
            mounting: None,
            key_dialog: None,
            alert: None,
            bulk: None,
            chat: ChatState::default(),
            agent: None,
            agent_events,
            pending_plan: None,
            plan_hidden: false,
            plan: None,
            term: TerminalManager::new(),
            status: Status::default(),
            spinner_frame: 0,
            last_spin: Instant::now(),
            busy: false,
            hover: None,
            regions: Regions::default(),
            scroll_drag: None,
            last_click: None,
            proxy_tunnel: None,
            db,
            cfg,
            datadir,
            term_tx,
            should_quit: false,
        };
        app.reload();
        app.start_agent();
        app
    }

    /// Start the worker if a model is configured. Idempotent.
    pub fn start_agent(&mut self) {
        if self.agent.is_some() || !self.cfg.agent.configured() {
            return;
        }
        let client = match crate::agent::client::HttpClient::new(&self.cfg.agent) {
            Ok(c) => c,
            Err(e) => {
                self.fail(format!("agent not started: {e}"));
                return;
            }
        };
        let (tx, rx) = channel();
        let stream = Arc::new(Stream::default());
        let exec = Arc::new(ExecStream::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let worker = Worker::new(
            Box::new(client),
            self.cfg.clone(),
            self.datadir.clone(),
            Arc::clone(&stream),
            Arc::clone(&exec),
            Arc::clone(&cancel),
            self.agent_events.clone(),
        );
        if std::thread::Builder::new()
            .name("openadmin-agent".into())
            .spawn(move || worker.run(rx))
            .is_err()
        {
            self.fail("could not start the agent worker");
            return;
        }
        self.agent = Some(AgentHandle {
            tx,
            stream,
            exec,
            cancel,
        });
    }

    /// The hosts the agent is allowed to see and act on.
    ///
    /// SSH only: every tool the model has — reading a file, running a
    /// scriptlet, uploading an artifact — goes over SSH, so an FTP entry is
    /// something it could name but never use. Filtering here rather than in
    /// each tool means `list_hosts`, `run_readonly` and plan resolution cannot
    /// disagree about what exists: to the model, an FTP host simply is not a
    /// host, and naming one gets the ordinary "unknown host" refusal.
    ///
    /// And only the hosts the Hosts screen lists: the operator's filter is the
    /// model's scope, for every tool at once, so a host it hides cannot be
    /// listed, probed, planned on or edited, and no refusal names one. The
    /// proxy comes from all hosts: hiding it must not route around it.
    pub(crate) fn agent_scope(&self) -> HostScope {
        HostScope {
            hosts: self
                .visible()
                .filter(|h| h.proto.eq_ignore_ascii_case("ssh"))
                .cloned()
                .collect(),
            filtered: self.filter.is_some(),
            proxy: self.proxy_host().cloned(),
        }
    }

    /// Hand the composed message to the worker.
    fn send_chat(&mut self) {
        if self.busy {
            return;
        }
        if self.agent.is_none() {
            self.start_agent();
        }
        let Some(handle) = self.agent.as_ref() else {
            self.flash(
                "No model configured — set agent.model in ~/.openadmin/config.toml.",
                StatusKind::Warn,
            );
            return;
        };
        let Some(text) = self.chat.take_draft() else {
            return;
        };
        handle.cancel.store(false, Ordering::Release);
        if handle
            .tx
            .send(AgentCommand::Send {
                text,
                scope: self.agent_scope(),
            })
            .is_err()
        {
            self.agent = None;
            self.fail("the agent worker has stopped");
            return;
        }
        self.busy = true;
        self.flash("Thinking…", StatusKind::Loading);
    }

    /// Hand a confirmed plan to the worker for execution.
    fn start_execution(&mut self, plan: ConfirmedPlan) {
        let Some(handle) = self.agent.as_ref() else {
            self.fail("the agent worker has stopped");
            return;
        };
        handle.cancel.store(false, Ordering::Release);
        if handle
            .tx
            .send(AgentCommand::Execute {
                // The hosts it names, from all of them: what was approved
                // runs, whatever the filter has become since.
                run_on: approve::approved_hosts(&self.hosts, &plan),
                plan,
                scope: self.agent_scope(),
            })
            .is_err()
        {
            self.agent = None;
            self.fail("the agent worker has stopped");
            return;
        }
        self.busy = true;
    }

    /// Stop an in-flight turn. The worker notices between stream lines.
    pub fn cancel_agent(&mut self) {
        if let Some(h) = self.agent.as_ref() {
            h.cancel.store(true, Ordering::Release);
        }
    }

    /// Fold one worker event into the transcript.
    pub fn on_agent_event(&mut self, ev: AgentEvent) {
        match ev {
            AgentEvent::Delta => {
                if let Some(h) = self.agent.as_ref() {
                    let text = h.stream.take();
                    if !text.is_empty() {
                        self.chat.push_delta(&text);
                    }
                }
            }
            AgentEvent::ToolStarted { name, arg } => {
                self.chat.finish_stream();
                self.chat.turns.push(chat::Turn::Tool {
                    name,
                    arg,
                    status: chat::ToolStatus::Running,
                    out: Vec::new(),
                });
            }
            AgentEvent::ToolFinished { ok, out } => {
                if let Some(chat::Turn::Tool { status, out: o, .. }) = self.chat.turns.last_mut() {
                    *status = if ok {
                        if out.is_empty() {
                            chat::ToolStatus::Empty
                        } else {
                            chat::ToolStatus::Ok
                        }
                    } else {
                        chat::ToolStatus::Fail
                    };
                    *o = out;
                }
            }
            AgentEvent::Proposed(plan) => {
                self.chat.finish_stream();
                self.chat.turns.push(chat::Turn::Plan {
                    id: plan.id,
                    title: plan.title.clone(),
                    steps: plan.steps.len(),
                    hosts: plan.host_count(),
                    state: PlanState::Proposed,
                });
                self.pending_plan = Some(*plan);
                self.plan_hidden = false;
                self.flash("Plan proposed — nothing has run.", StatusKind::Warn);
            }
            AgentEvent::HostWrite(write) => self.apply_host_write(*write),
            AgentEvent::ExecStarted {
                step,
                host,
                summary,
            } => {
                self.chat.finish_stream();
                self.chat.turns.push(chat::Turn::Tool {
                    name: format!("step {step}"),
                    arg: format!("{host} · {summary}"),
                    status: chat::ToolStatus::Running,
                    out: Vec::new(),
                });
            }
            AgentEvent::ExecLine => {
                if let Some(h) = self.agent.as_ref() {
                    let lines = h.exec.take();
                    if let Some(chat::Turn::Tool { out, .. }) = self.chat.turns.last_mut() {
                        out.extend(lines);
                        // The transcript keeps a window, not the whole log; the
                        // model's report keeps head and tail separately.
                        let excess = out.len().saturating_sub(200);
                        if excess > 0 {
                            out.drain(..excess);
                        }
                    }
                }
            }
            AgentEvent::ExecFinished { exit, timed_out } => {
                if let Some(chat::Turn::Tool { status, out, .. }) = self.chat.turns.last_mut() {
                    *status = if timed_out || exit != Some(0) {
                        chat::ToolStatus::Fail
                    } else if out.is_empty() {
                        chat::ToolStatus::Empty
                    } else {
                        chat::ToolStatus::Ok
                    };
                    let note = if timed_out {
                        "timed out".to_string()
                    } else {
                        match exit {
                            Some(c) => format!("exit {c}"),
                            None => "killed".to_string(),
                        }
                    };
                    out.push(note);
                }
            }
            AgentEvent::Done => {
                self.chat.finish_stream();
                self.busy = false;
                self.flash("Ready.", StatusKind::Idle);
            }
            AgentEvent::Cancelled => {
                self.chat.finish_stream();
                self.busy = false;
                self.flash("Cancelled.", StatusKind::Warn);
            }
            AgentEvent::Error(e) => {
                self.chat.finish_stream();
                self.busy = false;
                // In the transcript for the record, and in a dialog so it is
                // seen: the transcript keeps it, the dialog makes sure of it.
                self.chat.turns.push(chat::Turn::Error(e.clone()));
                self.chat.scroll = 0;
                self.fail(e);
            }
        }
    }

    // ---- host list ------------------------------------------------------

    /// Write a host record the operator asked the agent to write.
    ///
    /// The merge happens against what the database holds *now*, not the
    /// snapshot the turn began with, so a field the operator edited mid-turn is
    /// not silently reverted by a stale value. `crate::agent` cannot reach
    /// `DataBase` at all — it hands over a validated description and this is
    /// the only thing that performs it.
    fn apply_host_write(&mut self, write: HostWrite) {
        let existing = match &write {
            HostWrite::Create(_) => None,
            HostWrite::Edit { name, .. } => {
                // Among the listed hosts: the filter is the agent's scope, and
                // it is checked again here, live — it may have changed since the
                // tool ran.
                let found = self.visible().find(|h| &h.name == name).cloned();
                match found {
                    Some(h) => Some(h),
                    // The operator is told, not the model: they are the one who
                    // asked for the change, and the model must not learn that a
                    // hidden host exists.
                    None => {
                        if self.hosts.iter().any(|h| &h.name == name) {
                            self.fail(format!(
                                "The agent asked to change {name}, which the host filter \
                                 hides. Nothing was changed."
                            ));
                        } else {
                            // The record went away between the tool validating
                            // and this running.
                            self.fail(format!("No host called {name} to edit."));
                        }
                        return;
                    }
                }
            }
        };
        let creating = existing.is_none();
        let rec = write.apply(existing.as_ref(), &self.cfg.mount_prefix);
        let name = rec.name.clone();
        // No two hosts share a name. Checked here, against all of them, because
        // the agent's own check sees only what the filter lists — and telling
        // the model a hidden name is taken would tell it the host exists. A
        // name kept as it was is not checked, so a duplicate from before can
        // still be edited.
        let own = existing.as_ref().map(|e| e.id);
        let named_anew = existing
            .as_ref()
            .is_none_or(|e| !e.name.eq_ignore_ascii_case(&name));
        let taken = self
            .hosts
            .iter()
            .any(|h| Some(h.id) != own && h.name.eq_ignore_ascii_case(&name));
        if named_anew && taken {
            let what = if creating {
                "add a host called"
            } else {
                "rename a host to"
            };
            self.fail(format!(
                "The agent asked to {what} {name}, but a host by that name already \
                 exists. Nothing was saved."
            ));
            return;
        }
        match self.db.save(&rec) {
            Ok(id) => {
                // The cursor stays on its host: the operator may be reading the
                // Hosts screen, and a write they did not initiate should not
                // move what is under their fingers.
                self.reload();
                let what = if creating { "added" } else { "changed" };
                let shown = self.visible().any(|h| h.id == id);
                let text = if shown {
                    format!("Agent {what} host {name}.")
                } else {
                    format!("Agent {what} host {name} — hidden by the filter.")
                };
                self.flash(text, StatusKind::Warn);
            }
            Err(e) => self.fail(format!("Could not save {name}: {e}")),
        }
    }

    pub fn reload(&mut self) {
        match self.db.search("") {
            Ok(mut hosts) => {
                let mounts = mtab::parse_mtab();
                for h in &mut hosts {
                    h.mounted = !h.mount_point.is_empty() && mounts.contains(&h.mount_point);
                }
                let on = self.host().map(|h| h.id);
                self.hosts = hosts;
                // Marks on a host that is gone, or that the filter now hides —
                // an edit can rename one out of it — go with it.
                self.prune_marks();
                // The cursor stays on the host it was on, not the index: a host
                // deleted above it, or the filter, moves its row.
                self.restore_cursor(on);
            }
            Err(e) => self.fail(format!("Could not read hosts: {e}")),
        }
    }

    /// Refresh only the mounted flags, which change outside the app.
    pub fn refresh_mounts(&mut self) {
        let mounts = mtab::parse_mtab();
        for h in &mut self.hosts {
            h.mounted = !h.mount_point.is_empty() && mounts.contains(&h.mount_point);
        }
    }

    /// The hosts the Hosts screen lists, as indices into `hosts`: all of them,
    /// or those the filter matches. Worked out on each use rather than kept, so
    /// it can never disagree with `hosts` — a reload, an agent's write, a test
    /// pushing a record.
    pub fn view(&self) -> Vec<usize> {
        match self.filter_lc() {
            None => (0..self.hosts.len()).collect(),
            Some(n) => (0..self.hosts.len())
                .filter(|&i| host_matches(&self.hosts[i], &n))
                .collect(),
        }
    }

    /// The listed hosts themselves.
    pub fn visible(&self) -> impl Iterator<Item = &HostRecord> {
        let n = self.filter_lc();
        self.hosts
            .iter()
            .filter(move |h| n.as_deref().is_none_or(|n| host_matches(h, n)))
    }

    pub fn is_visible(&self, h: &HostRecord) -> bool {
        self.filter_lc().is_none_or(|n| host_matches(h, &n))
    }

    fn filter_lc(&self) -> Option<String> {
        self.filter.as_ref().map(|f| f.to_lowercase())
    }

    /// The host under the cursor — a position in the view, not in `hosts`.
    pub fn host(&self) -> Option<&HostRecord> {
        self.view()
            .get(self.cursor)
            .and_then(|&i| self.hosts.get(i))
    }

    /// The hosts an action applies to: every marked host, else the cursor row.
    /// Only ever listed ones: Enter, mount and F8's delete go through here, and
    /// none of them may reach a host the filter hides.
    pub fn targets(&self) -> Vec<HostRecord> {
        if self.marked.is_empty() {
            self.host().cloned().into_iter().collect()
        } else {
            self.visible()
                .filter(|h| self.marked.contains(&h.id))
                .cloned()
                .collect()
        }
    }

    /// Marks only on hosts that exist and are listed.
    fn prune_marks(&mut self) {
        let keep: HashSet<i64> = self.visible().map(|h| h.id).collect();
        self.marked.retain(|id| keep.contains(id));
    }

    /// Put the cursor back on host `id` if it is listed; else keep the position,
    /// clamped.
    fn restore_cursor(&mut self, id: Option<i64>) {
        let pos = id.and_then(|id| self.visible().position(|h| h.id == id));
        match pos {
            Some(pos) => self.cursor = pos,
            None => self.clamp_cursor(),
        }
    }

    /// Apply a filter, or clear it with `None` or a blank needle. The cursor
    /// keeps its host if it is still listed, else goes to the top; marks on
    /// hosts it hides are dropped.
    pub fn set_filter(&mut self, needle: Option<String>) {
        let needle = needle
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty());
        let on = self.host().map(|h| h.id);
        self.filter = needle;
        self.prune_marks();
        self.cursor = 0;
        self.restore_cursor(on);
        let (shown, all) = (self.view().len(), self.hosts.len());
        let mut text = match (&self.filter, shown) {
            (None, _) => format!("Showing all {all} hosts."),
            (Some(_), 0) => "No host matches — Esc shows all.".to_string(),
            (Some(_), n) => format!("Showing {n} of {all} hosts."),
        };
        if self.busy {
            text.push_str(" The agent sees this from your next message.");
        }
        self.flash(text, StatusKind::Idle);
    }

    pub fn proxy_host(&self) -> Option<&HostRecord> {
        self.hosts.iter().find(|h| h.proxy)
    }

    pub fn mounted_count(&self) -> usize {
        self.hosts.iter().filter(|h| h.mounted).count()
    }

    fn clamp_cursor(&mut self) {
        let n = self.view().len();
        if n == 0 {
            self.cursor = 0;
            self.scroll = 0;
        } else if self.cursor >= n {
            self.cursor = n - 1;
        }
    }

    /// Move within the listed hosts.
    pub fn move_cursor(&mut self, delta: isize) {
        let n = self.view().len();
        if n == 0 {
            return;
        }
        let last = n as isize - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
    }

    /// Put the cursor on row `i` of the listed hosts.
    pub fn set_cursor(&mut self, i: usize) {
        if i < self.view().len() {
            self.cursor = i;
        }
    }

    pub fn toggle_mark(&mut self) {
        if let Some(h) = self.host() {
            let id = h.id;
            if !self.marked.insert(id) {
                self.marked.remove(&id);
            }
            self.move_cursor(1);
        }
    }

    /// Within the listed hosts: a hidden host is never marked.
    fn invert_marks(&mut self) {
        let listed: HashSet<i64> = self.visible().map(|h| h.id).collect();
        self.marked = listed.difference(&self.marked).copied().collect();
    }

    /// Ctrl+F: the filter dialog, holding the filter there is, to change.
    pub fn open_filter(&mut self) {
        self.filter_edit = Some(Input::new(self.filter.clone().unwrap_or_default()));
        self.mode = Mode::Filter;
    }

    fn close_filter(&mut self) {
        self.filter_edit = None;
        self.mode = Mode::Normal;
    }

    fn key_filter(&mut self, key: KeyEvent) {
        let ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            // Cancel leaves the filter as it was. Ctrl+C too: it quits only
            // from the screens, so here it would otherwise do nothing.
            KeyCode::Esc => self.close_filter(),
            _ if ctrl_c => self.close_filter(),
            KeyCode::Enter => {
                let needle = self.filter_edit.as_ref().map(|i| i.value().to_string());
                self.close_filter();
                self.set_filter(needle);
            }
            _ => {
                if let Some(i) = self.filter_edit.as_mut()
                    && let Some(req) = to_input_request(&ratatui::crossterm::event::Event::Key(key))
                {
                    i.handle(req);
                }
            }
        }
    }

    // ---- status ---------------------------------------------------------

    pub fn flash(&mut self, text: impl Into<String>, kind: StatusKind) {
        self.status = Status {
            text: text.into(),
            kind,
            set_at: Instant::now(),
        };
    }

    pub fn ok(&mut self, text: impl Into<String>) {
        self.flash(text, StatusKind::Ok);
    }

    /// An error, as a dialog the operator dismisses.
    ///
    /// It used to be red text in the status line, which gives a message 48
    /// columns and a few seconds: a failed mount's sshfs output, a database
    /// error, a backend's JSON body were all cut off mid-sentence and then gone.
    /// An error is the one message that must be read, so it waits to be.
    ///
    /// A second error while one is showing goes under it rather than replacing
    /// it — two failures are two things to know — and one already on screen is
    /// not repeated.
    pub fn fail(&mut self, text: impl Into<String>) {
        let text = text.into();
        match &mut self.alert {
            Some(shown) if shown.split("\n\n").any(|m| m == text) => {}
            Some(shown) => {
                shown.push_str("\n\n");
                shown.push_str(&text);
            }
            None => self.alert = Some(text),
        }
    }

    pub fn dismiss_alert(&mut self) {
        self.alert = None;
    }

    pub fn maybe_revert_status(&mut self) {
        if self.status.kind != StatusKind::Idle
            && self.status.kind != StatusKind::Loading
            && self.status.set_at.elapsed() > STATUS_REVERT
        {
            self.status = Status {
                text: "Ready.".into(),
                kind: StatusKind::Idle,
                set_at: Instant::now(),
            };
        }
    }

    /// Something is in progress that the screen should keep moving for. Not
    /// `busy`, which means a chat turn and changes what Esc and Enter do there.
    pub fn animating(&self) -> bool {
        self.busy || self.mounting.is_some()
    }

    pub fn tick_spinner(&mut self) {
        if self.animating() && self.last_spin.elapsed() >= SPINNER_TICK {
            self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
            self.last_spin = Instant::now();
        }
    }

    pub fn spinner_char(&self) -> char {
        SPINNER_FRAMES[self.spinner_frame % SPINNER_FRAMES.len()]
    }

    // ---- actions --------------------------------------------------------

    pub fn open_add(&mut self) {
        self.form = Some(FormState::new(&self.cfg.mount_prefix));
        self.mode = Mode::HostForm;
    }

    pub fn open_edit(&mut self) {
        if let Some(h) = self.host().cloned() {
            self.form = Some(FormState::from_record(&h, &self.cfg.mount_prefix));
            self.mode = Mode::HostForm;
        }
    }

    fn save_form(&mut self) {
        let Some(form) = self.form.clone() else {
            return;
        };
        if let Err(msg) = form.validate() {
            self.fail(msg);
            return;
        }
        let rec = form.to_record(&self.cfg.mount_prefix);
        let editing = form.is_edit();
        match self.db.save(&rec) {
            Ok(id) => {
                self.mode = Mode::Normal;
                self.form = None;
                self.reload();
                let name = rec.name.clone();
                // The filter stays: it is the agent's scope too, and only the
                // operator's Esc widens that. A host it hides is said so.
                let pos = self.visible().position(|h| h.id == id);
                match pos {
                    Some(pos) => {
                        self.cursor = pos;
                        if editing {
                            self.ok(format!("Saved {name}."));
                        } else {
                            self.ok(format!("Added {name}."));
                        }
                    }
                    None => self.flash(
                        format!(
                            "{} {name} — hidden by the filter; Esc shows all.",
                            if editing { "Saved" } else { "Added" }
                        ),
                        StatusKind::Warn,
                    ),
                }
            }
            Err(e) => self.fail(format!("Could not save: {e}")),
        }
    }

    fn open_delete(&mut self) {
        let targets = self.targets();
        if targets.is_empty() {
            return;
        }
        self.pending_delete = targets.iter().map(|h| h.id).collect();
        self.mode = Mode::ConfirmDelete;
    }

    fn do_delete(&mut self) {
        let ids = std::mem::take(&mut self.pending_delete);
        let n = ids.len();
        for id in &ids {
            if let Err(e) = self.db.remove(*id) {
                self.fail(format!("Could not delete: {e}"));
                self.mode = Mode::Normal;
                return;
            }
        }
        self.marked.clear();
        self.mode = Mode::Normal;
        self.reload();
        self.ok(format!(
            "Deleted {n} host{}.",
            if n == 1 { "" } else { "s" }
        ));
    }

    /// Bulk add, opened over the Add dialog by F2 or its Bulk add button.
    ///
    /// The form stays in `self.form`, so leaving without importing returns to
    /// it as it was left. Not from Edit: an import closes the form it came
    /// from, and an Edit form would take its unsaved changes with it.
    fn open_bulk(&mut self) {
        if self.form.as_ref().is_none_or(|f| f.is_edit()) {
            return;
        }
        self.bulk = Some(BulkImport::new());
        self.mode = Mode::BulkImport;
    }

    /// Leave bulk import without importing, back to the Add dialog it came
    /// from. Every way out but an import comes here, as the key dialog's all go
    /// through `close_popup`, so none can leave a form behind `Normal` for the
    /// next dialog to return to.
    fn close_bulk(&mut self) {
        self.bulk = None;
        self.mode = if self.form.is_some() {
            Mode::HostForm
        } else {
            Mode::Normal
        };
    }

    /// Add every new host in the paste, in one transaction.
    ///
    /// The review is recomputed here rather than taken from the last frame, so
    /// what is written is judged against the hosts as they are now. A problem
    /// anywhere stops the whole import: a list with one block out of shape may
    /// be out of step from there on, and half an import is harder to clean up
    /// than a second paste.
    fn import_bulk(&mut self) {
        let Some(b) = self.bulk.as_ref() else {
            return;
        };
        let rows = bulk::review(&b.joined(), &self.hosts, &self.cfg.mount_prefix);
        let counts = bulk::counts(&rows);
        if counts.problems > 0 {
            self.flash(
                format!(
                    "Fix {} first — Shift+Tab goes back to the text.",
                    bulk::count(counts.problems, "problem")
                ),
                StatusKind::Warn,
            );
            return;
        }
        let recs = bulk::records(&rows);
        if recs.is_empty() {
            self.flash("Nothing to import.", StatusKind::Warn);
            return;
        }
        match self.db.insert_all(&recs) {
            Ok(ids) => {
                // Done with both. The Add form was only the way in; left in
                // `self.form` it would come back the next time Help closed.
                self.bulk = None;
                self.form = None;
                self.mode = Mode::Normal;
                self.reload();
                // Marked, so whatever comes next — mount them, open shells on
                // them, or delete them if the list was the wrong one — applies
                // to exactly what was imported. Only those listed: a mark on a
                // host the filter hides would be one no action can reach.
                let listed: HashSet<i64> = self.visible().map(|h| h.id).collect();
                self.marked = ids
                    .iter()
                    .copied()
                    .filter(|id| listed.contains(id))
                    .collect();
                let first = self.visible().position(|h| self.marked.contains(&h.id));
                if let Some(pos) = first {
                    self.cursor = pos;
                }
                let hidden = ids.len() - self.marked.len();
                let all = bulk::count(ids.len(), "host");
                if hidden == 0 {
                    self.ok(format!("Imported {all} — marked; Esc clears."));
                } else {
                    self.flash(
                        format!("Imported {all} — the filter hides {hidden}; Esc shows all."),
                        StatusKind::Warn,
                    );
                }
            }
            Err(e) => self.fail(format!("Could not import: {e}. Nothing was added.")),
        }
    }

    /// Close the help or key dialog, back to whatever opened it.
    ///
    /// The key dialog opens from the edit form, and closing it to Normal hid the
    /// form with its unsaved edits — the new `key_name` among them — still in
    /// `self.form`, where the next Edit replaced them. One function for every
    /// way of closing, because the first fix covered Esc and Enter and missed
    /// the click outside the dialog.
    fn close_popup(&mut self) {
        self.mode = if self.form.is_some() {
            Mode::HostForm
        } else {
            Mode::Normal
        };
        self.key_dialog = None;
    }

    /// Show the key of the host in the form, or generate one for it.
    ///
    /// The only place a key is made: the Hosts screen has no key action of its
    /// own, because generating one is a change to a host and the form is where a
    /// host is changed. Reached by F7 and by the key row's button.
    ///
    /// A host that already has a key is *shown* that key, read by the name the
    /// form holds. It used to generate against the typed name whatever the row
    /// said — so a host renamed in the form got a brand-new key under its new
    /// name, whose public half was installed nowhere, and Save pointed the host
    /// at it.
    fn form_key(&mut self) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let name = form.name.value().trim().to_string();
        if form.has_key() {
            let key_name = form.key_name.clone();
            match keys::public_key(&self.datadir, &key_name) {
                Ok(public) => {
                    self.key_dialog = Some((name, public));
                    self.mode = Mode::ShowKey;
                }
                Err(e) => self.fail(format!("Could not read the public key: {e}")),
            }
            return;
        }
        if name.is_empty() {
            self.fail("Name the host before generating a key.");
            return;
        }
        // Generation is idempotent — an existing key file is read, never
        // replaced — so the key is safe to make before Save and is recovered
        // rather than duplicated if the form is cancelled and opened again. The
        // form carries `key_name` to Save, which is what persists it.
        match keys::generate(&self.datadir, &name) {
            Ok((key_name, public)) => {
                if let Some(form) = self.form.as_mut() {
                    form.key_name = key_name;
                }
                self.key_dialog = Some((name, public));
                self.mode = Mode::ShowKey;
            }
            Err(e) => self.fail(format!("{e}")),
        }
    }

    /// Mount or unmount the targets. `None` is F9's toggle, `Some` is `m`/`u`.
    ///
    /// The toggle keeps qhostman's rule — if any target is unmounted, the action
    /// is Mount — but only ever touches the targets that are not already where
    /// they are going. It used to act on all of them: mounting a host that is
    /// already mounted fails in sshfs, the loop stopped at the first failure, and
    /// a mixed selection ended with the hosts after it never mounted at all.
    fn set_mount(&mut self, want: Option<bool>) {
        let targets = self.targets();
        if targets.is_empty() {
            return;
        }
        let mounting = want.unwrap_or_else(|| targets.iter().any(|h| !h.mounted));
        let todo: Vec<HostRecord> = targets
            .into_iter()
            .filter(|h| h.mounted != mounting)
            .collect();
        if todo.is_empty() {
            self.ok(if mounting {
                "Already mounted."
            } else {
                "Nothing mounted."
            });
            return;
        }
        if mounting {
            let (datadir, cfg) = (self.datadir.clone(), self.cfg.clone());
            self.start_mount(todo, move |h, cancel| {
                mount::mount(h, &datadir, &cfg, cancel)
            });
            return;
        }
        // Unmounting stays inline: `fusermount -uz` returns at once, and a dialog
        // for it would only flash.
        let mut done = 0usize;
        for h in &todo {
            match mount::unmount(h) {
                Ok(()) => done += 1,
                Err(e) => {
                    self.refresh_mounts();
                    self.fail(format!("{e}"));
                    return;
                }
            }
        }
        self.refresh_mounts();
        self.ok(format!(
            "Unmounted {done} host{}.",
            if done == 1 { "" } else { "s" }
        ));
    }

    /// Start a mount in the background and show its dialog.
    ///
    /// Takes the per-host mount as a function so tests can stand in for sshfs.
    pub fn start_mount<F>(&mut self, targets: Vec<HostRecord>, mount_one: F)
    where
        F: Fn(&HostRecord, &AtomicBool) -> anyhow::Result<mount::Outcome> + Send + 'static,
    {
        self.mounting = Some(mount_job::MountJob::start(targets, mount_one));
        self.mode = Mode::Mounting;
    }

    pub fn cancel_mount(&mut self) {
        if let Some(job) = &self.mounting {
            job.cancel();
        }
    }

    /// Called by the event loop on every pass. Returns true when the mount
    /// finished, so the loop redraws the table without the dialog.
    pub fn poll_mount(&mut self) -> bool {
        let Some(job) = &self.mounting else {
            return false;
        };
        let Some(finish) = job.progress().finish else {
            return false;
        };
        let done = job.progress().done;
        if let Some(mut job) = self.mounting.take() {
            job.join();
        }
        self.mode = Mode::Normal;
        // Re-read the mount table rather than trusting the count: a cancel can
        // land after sshfs has already mounted, and the table should say so.
        self.refresh_mounts();
        let hosts = |n: usize| format!("{n} host{}", if n == 1 { "" } else { "s" });
        match finish {
            mount_job::Finish::All => self.ok(format!("Mounted {}.", hosts(done))),
            mount_job::Finish::Cancelled if done == 0 => self.ok("Mount cancelled."),
            mount_job::Finish::Cancelled => {
                self.ok(format!("Cancelled after mounting {}.", hosts(done)))
            }
            mount_job::Finish::Failed(e) => self.fail(e),
        }
        true
    }

    /// On quit: stop a mount in flight rather than leave sshfs connecting after
    /// the app is gone, finishing a mount nobody is there to see.
    pub fn abort_mount(&mut self) {
        if let Some(mut job) = self.mounting.take() {
            job.cancel();
            job.join();
        }
    }

    fn toggle_proxy(&mut self) {
        let Some(h) = self.host().cloned() else {
            return;
        };
        let mut rec = h.clone();
        rec.proxy = !rec.proxy;
        if let Err(e) = self.db.save(&rec) {
            self.fail(format!("Could not save: {e}"));
            return;
        }
        if rec.proxy
            && let Err(e) = self.db.clear_proxy_except(rec.id)
        {
            self.fail(format!("Could not clear the previous proxy: {e}"));
            return;
        }
        self.stop_proxy_tunnel();
        if rec.proxy {
            match self.start_proxy_tunnel(&rec) {
                Ok(()) => {
                    self.reload();
                    self.ok(format!(
                        "Routing through {} on 127.0.0.1:{}.",
                        rec.name, self.cfg.proxy_port
                    ));
                }
                Err(e) => {
                    // Leave the flag off rather than advertise a dead tunnel.
                    rec.proxy = false;
                    let _ = self.db.save(&rec);
                    self.reload();
                    self.fail(format!("Could not open the proxy tunnel: {e}"));
                }
            }
        } else {
            self.reload();
            self.ok(format!("{} no longer used as proxy.", rec.name));
        }
    }

    fn start_proxy_tunnel(&mut self, rec: &HostRecord) -> anyhow::Result<()> {
        let launch = ssh::proxy_tunnel_command(rec, &self.datadir, &self.cfg);
        let mut cmd = std::process::Command::new(&launch.program);
        cmd.args(&launch.args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        for (k, v) in &launch.env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn()?;
        // ExitOnForwardFailure means a bound port shows up as an early exit.
        std::thread::sleep(Duration::from_millis(400));
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("ssh exited immediately ({status})");
        }
        self.proxy_tunnel = Some(child);
        Ok(())
    }

    fn stop_proxy_tunnel(&mut self) {
        if let Some(mut c) = self.proxy_tunnel.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// F5 — open a shell on every target, in one tab.
    pub fn open_shell(&mut self) {
        let targets = self.targets();
        if targets.is_empty() {
            self.flash("No host selected.", StatusKind::Warn);
            return;
        }
        let proxy = self.proxy_host().cloned();
        let entries: Vec<(String, Spawn)> = targets
            .iter()
            .map(|h| {
                let launch = ssh::shell_command(h, &self.datadir, &self.cfg, proxy.as_ref());
                (
                    h.name.clone(),
                    Spawn {
                        program: launch.program,
                        args: launch.args,
                        env: launch.env,
                    },
                )
            })
            .collect();

        let n = entries.len();
        // The real geometry is applied by the next render's sync_sizes; this is
        // only the starting size.
        let size = (24u16, 80u16);
        match self.term.open_tab(
            entries,
            size,
            self.cfg.scrollback,
            &self.cfg.term,
            &self.term_tx,
        ) {
            Ok(()) => {
                self.marked.clear();
                self.set_screen(Screen::Shells);
                if n > 1 {
                    self.ok(format!("Opened {n} shells in one tab."));
                } else {
                    self.ok(format!("Shell open on {}.", targets[0].name));
                }
            }
            Err(e) => self.fail(format!("Could not open a shell: {e}")),
        }
    }

    pub fn quit(&mut self) {
        self.abort_mount();
        if let Some(h) = self.agent.take() {
            h.cancel.store(true, Ordering::Release);
            let _ = h.tx.send(AgentCommand::Shutdown);
        }
        self.stop_proxy_tunnel();
        self.term.shutdown();
        self.should_quit = true;
    }

    // ---- key routing ----------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        // The error dialog is modal. Esc and Enter press Dismiss; any other key
        // is held back rather than dismissing it, because a dialog that closes on
        // whatever key the operator was already typing is an error nobody read.
        if self.alert.is_some() {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                self.dismiss_alert();
            }
            return;
        }
        match self.mode {
            // Cancel is the only button, so Enter presses it too; Ctrl+C means
            // "stop that" here as it does mid-turn in the chat.
            Mode::Mounting => {
                let ctrl_c =
                    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter) || ctrl_c {
                    self.cancel_mount();
                }
            }
            Mode::HostForm => self.key_host_form(key),
            Mode::ConfirmPlan => self.key_confirm_plan(key),
            Mode::ConfirmDelete => self.key_confirm_delete(key),
            Mode::BulkImport => self.key_bulk(key),
            Mode::Filter => self.key_filter(key),
            Mode::ShowKey | Mode::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::F(1)) {
                    self.close_popup();
                }
            }
            Mode::Normal => match self.screen {
                // The Shells screen hands every key to the terminal but the
                // tab keys; the rest of the app is a click on the header away.
                Screen::Shells => self.key_shells(key),
                Screen::Hosts => self.key_hosts(key),
                Screen::Chat => self.key_chat(key),
            },
        }
    }

    /// Keys shared by the Hosts and Chat screens: the tab keys, real F-keys,
    /// and the mc-style Esc+0 for Quit.
    fn key_global(&mut self, key: KeyEvent) -> bool {
        if self.key_tabs(key) {
            return true;
        }
        // Esc+0, which arrives as Alt+0, is Quit as in mc.
        if key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char('0') {
            self.function_key(0);
            return true;
        }
        if let KeyCode::F(n) = key.code {
            self.function_key(n);
            return true;
        }
        false
    }

    fn key_hosts(&mut self, key: KeyEvent) {
        if self.key_global(key) {
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Down => self.move_cursor(1),
            KeyCode::Up => self.move_cursor(-1),
            KeyCode::PageDown => self.move_cursor(10),
            KeyCode::PageUp => self.move_cursor(-10),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.view().len().saturating_sub(1),
            KeyCode::Insert | KeyCode::Char(' ') => self.toggle_mark(),
            KeyCode::Char('*') => self.invert_marks(),
            // Before the plain letter below, or Ctrl+A would add a host. All
            // the listed hosts: a hidden one is never marked.
            KeyCode::Char('a') if ctrl => {
                self.marked = self.visible().map(|h| h.id).collect();
            }
            KeyCode::Char('f' | 'F') if ctrl => self.open_filter(),
            // Letter twins of the F-keys, for keyboards and terminals where the
            // F-row is awkward. Either case, so Caps Lock is not a trap.
            KeyCode::Char('a' | 'A') => self.open_add(),
            KeyCode::Char('e' | 'E') => self.open_edit(),
            // F9 toggles; these say which way, so a mixed selection does what
            // was asked rather than what qhostman's rule would guess.
            KeyCode::Char('m' | 'M') => self.set_mount(Some(true)),
            KeyCode::Char('u' | 'U') => self.set_mount(Some(false)),
            // A filter first, then the marks: each Esc undoes one thing.
            KeyCode::Esc if self.filter.is_some() => self.set_filter(None),
            KeyCode::Esc => self.marked.clear(),
            KeyCode::Enter => self.open_shell(),
            KeyCode::Delete => self.open_delete(),
            _ => {}
        }
    }

    fn key_chat(&mut self, key: KeyEvent) {
        // Ctrl+C means "stop that" while something is running — which is what
        // it means in every shell — and keeps meaning "quit" when idle.
        if key.code == KeyCode::Char('c')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && self.busy
        {
            self.cancel_agent();
            return;
        }
        if self.key_global(key) {
            return;
        }
        match key.code {
            KeyCode::Enter => self.send_chat(),
            KeyCode::Esc if self.busy => self.cancel_agent(),
            KeyCode::Esc => self.chat.draft = tui_textarea::TextArea::default(),
            KeyCode::PageUp => self.chat.scroll_by(10),
            KeyCode::PageDown => self.chat.scroll_by(-10),
            // Ctrl+J opens a line. It *is* LF, so crossterm reports it in raw
            // mode on every terminal — unlike Shift+Enter, which needs a
            // keyboard protocol xterm, Terminal.app and plain tmux do not
            // speak, and which would therefore work on the author's machine
            // and nowhere else.
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.chat.draft.insert_newline();
            }
            // Everything else is editing: the composer is a text area, and
            // behaves like one.
            _ => {
                self.chat.draft.input(key);
            }
        }
    }

    /// The Shells screen: nothing is reserved but the tab keys, so mc, GNU
    /// Screen and vim keep the rest of their keyboard.
    fn key_shells(&mut self, key: KeyEvent) {
        // The exception to the rule below: Alt+1…9 and Alt+←/→ walk the tabs,
        // so there is a keyboard way out of a terminal and between shells, not
        // only a click. mc, vim and GNU Screen bind none of them by default;
        // readline's Alt+digit numeric argument is what this costs.
        if self.key_tabs(key) {
            return;
        }

        // With no pane to type into there is nothing to be transparent to, so
        // the normal app keys work there.
        let Some(id) = self.term.focused_session() else {
            self.key_hosts_or_global(key);
            return;
        };

        // Otherwise a focused pane takes every other key: F1-F10, Tab, Ctrl+A,
        // Alt+letter, Alt+0 and Esc. mc reads Esc+digit as its own F-key
        // emulation and Alt as its menu shortcuts, so reserving more of them
        // here would quietly break it. The app is otherwise reachable with the
        // mouse — the header tabs are clickable.
        let app_cursor = self
            .term
            .session(id)
            .map(|s| {
                s.parser()
                    .lock()
                    .map(|p| p.screen().application_cursor())
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        let bytes = crate::term::keys::encode(key, app_cursor);
        self.write_terminal(&bytes);
    }

    /// Alt+1…9 jump to that tab of the header strip; Alt+←/→ walk it,
    /// wrapping in both directions. The same on every screen, a focused
    /// terminal included. Returns whether the key was one of them.
    fn key_tabs(&mut self, key: KeyEvent) -> bool {
        if !key.modifiers.contains(KeyModifiers::ALT) {
            return false;
        }
        match key.code {
            KeyCode::Left => self.prev_tab(),
            KeyCode::Right => self.next_tab(),
            KeyCode::Char(c @ '1'..='9') => self.goto_index(c as usize - '1' as usize),
            _ => return false,
        }
        true
    }

    /// The keyboard the Shells screen falls back to when no pane is focused.
    fn key_hosts_or_global(&mut self, key: KeyEvent) {
        if self.key_global(key) {
            return;
        }
        if key.code == KeyCode::Esc {
            self.set_screen(Screen::Hosts);
        }
    }

    /// A paste from the outer terminal, forwarded to the focused pane.
    pub fn on_paste(&mut self, text: &str) {
        match self.screen {
            Screen::Shells if self.mode == Mode::Normal => {
                let bracketed = self
                    .term
                    .focused_session()
                    .and_then(|id| self.term.session(id))
                    .map(|s| {
                        s.parser()
                            .lock()
                            .map(|p| p.screen().bracketed_paste())
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                let bytes = crate::term::keys::encode_paste(text, bracketed);
                self.write_terminal(&bytes);
            }
            // A pasted block keeps its lines: flattening a log into one line
            // was the main reason the composer needed to grow up.
            //
            // The line endings have to be normalised first. A terminal sends
            // the *input* form of a newline inside a bracketed paste — tmux
            // and most emulators send CR — and crossterm hands the bytes over
            // untouched, so a paste arrives full of `\r` that nothing
            // downstream treats as a line break. Only the composer needs this:
            // a paste bound for a shell is that shell's business.
            Screen::Chat if self.mode == Mode::Normal => {
                self.chat
                    .draft
                    .insert_str(text.replace("\r\n", "\n").replace('\r', "\n"));
            }
            _ => match self.mode {
                Mode::HostForm => {
                    let prefix = self.cfg.mount_prefix.clone();
                    if let Some(form) = self.form.as_mut() {
                        for c in text.chars().filter(|c| !c.is_control()) {
                            form.handle_key(
                                KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty()),
                                &prefix,
                            );
                        }
                    }
                }
                // What the dialog is for. The same line-ending repair as the
                // composer's, for the same reason: the text area breaks lines
                // on LF only, and a terminal sends CR.
                Mode::BulkImport => {
                    if let Some(b) = self.bulk.as_mut()
                        && b.step == BulkStep::Paste
                    {
                        b.text
                            .insert_str(text.replace("\r\n", "\n").replace('\r', "\n"));
                    }
                }
                // One line: what a paste carries besides text goes.
                Mode::Filter => {
                    if let Some(i) = self.filter_edit.as_mut() {
                        for c in text.chars().filter(|c| !c.is_control()) {
                            i.handle(tui_input::InputRequest::InsertChar(c));
                        }
                    }
                }
                _ => {}
            },
        }
    }

    fn write_terminal(&mut self, bytes: &[u8]) {
        if let Err(e) = self.term.write_focused(bytes) {
            self.fail(format!("Terminal write failed: {e}"));
        }
    }

    /// One entry point for real F-keys, Alt+0 and function-bar clicks, so
    /// all three share the same behavior. On the Shells screen only the clicks
    /// reach it: a focused terminal keeps the whole keyboard.
    pub fn function_key(&mut self, n: u8) {
        // Nothing behind a dialog is reachable from here: F10 must not quit out
        // from under an unsaved form. Which F-keys the form has is
        // `key_host_form`'s call, so the two cannot disagree.
        if self.mode == Mode::HostForm {
            self.key_host_form(KeyEvent::new(KeyCode::F(n), KeyModifiers::empty()));
            return;
        }
        // Nor may a click on the bar quit out from under a paste.
        if matches!(self.mode, Mode::Mounting | Mode::BulkImport | Mode::Filter) {
            return;
        }
        // `Esc 0` and F10 are both Quit.
        if n == 0 || n == 10 {
            self.quit();
            return;
        }
        if n == 1 {
            self.mode = if self.mode == Mode::Help {
                Mode::Normal
            } else {
                Mode::Help
            };
            return;
        }
        match self.screen {
            // F9 is Mount here rather than the tab cycle it is on the other
            // screens; Alt+←/→ and Alt+1…9 still switch from Hosts.
            Screen::Hosts => match n {
                2 => self.open_add(),
                4 => self.open_edit(),
                6 => self.toggle_proxy(),
                8 => self.open_delete(),
                9 => self.set_mount(None),
                _ => {}
            },
            Screen::Shells => match n {
                2 => self.term.next_pane(),
                3 => self.next_tab(),
                4 => {
                    if !self.term.is_empty() {
                        self.term.close_active_tab();
                        self.ok("Shell closed.");
                        self.after_shell_closed();
                    }
                }
                5 => self.set_screen(Screen::Hosts),
                9 => self.next_tab(),
                _ => {}
            },
            Screen::Chat => match n {
                2 => self.open_plan(),
                9 => self.next_tab(),
                _ => {}
            },
        }
    }

    /// Every screen change goes through here, so the screen left for Shells is
    /// always remembered — whether it was a key, a header tab, F9 or opening a
    /// shell that got there.
    pub fn set_screen(&mut self, to: Screen) {
        if to == Screen::Shells && self.screen != Screen::Shells {
            self.prev_screen = self.screen;
        }
        self.screen = to;
    }

    /// After a shell tab has gone: if it was the last and the Shells screen is
    /// showing, go back to the screen that was up before it.
    ///
    /// An empty Shells screen is somewhere to be only on purpose. Closing the
    /// last shell left the operator staring at "no shell open" and reaching for
    /// Alt+← to get back to where they started — which, for the usual Enter on
    /// a host, `exit`, is exactly where they wanted to be.
    fn after_shell_closed(&mut self) {
        if self.term.is_empty() && self.screen == Screen::Shells {
            self.set_screen(self.prev_screen);
        }
    }

    /// Reap sessions that have exited, and go back if that emptied the screen.
    ///
    /// The event loop's way of noticing a shell that ended by itself — `exit`,
    /// a dropped connection — as opposed to one closed with F4 or its ×.
    pub fn reap_shells(&mut self) {
        let had_shells = !self.term.is_empty();
        self.term.reap_finished();
        if had_shells {
            self.after_shell_closed();
        }
    }

    /// How many tabs the header strip has: Hosts, Chat and the shells.
    pub fn tab_count(&self) -> usize {
        2 + self.term.tab_count()
    }

    /// The strip's tab at `i`, counting from 0.
    pub fn tab_at(&self, i: usize) -> Option<Tab> {
        match i {
            0 => Some(Tab::Hosts),
            1 => Some(Tab::Chat),
            _ if i - 2 < self.term.tab_count() => Some(Tab::Shell(i - 2)),
            _ => None,
        }
    }

    /// Where the showing tab is in the strip.
    pub fn current_tab(&self) -> usize {
        match self.screen {
            Screen::Hosts => 0,
            Screen::Chat => 1,
            Screen::Shells => 2 + self.term.active.unwrap_or(0),
        }
    }

    /// Show `tab`. A shell tab that has gone is ignored rather than leaving
    /// an empty Shells screen up.
    pub fn goto(&mut self, tab: Tab) {
        match tab {
            Tab::Hosts => self.set_screen(Screen::Hosts),
            Tab::Chat => self.set_screen(Screen::Chat),
            Tab::Shell(i) if i < self.term.tab_count() => {
                self.term.select_tab(i);
                self.set_screen(Screen::Shells);
            }
            Tab::Shell(_) => {}
        }
    }

    /// Show the strip's tab at `i`; past the end does nothing.
    fn goto_index(&mut self, i: usize) {
        if let Some(t) = self.tab_at(i) {
            self.goto(t);
        }
    }

    fn next_tab(&mut self) {
        self.goto_index((self.current_tab() + 1) % self.tab_count());
    }

    fn prev_tab(&mut self) {
        let n = self.tab_count();
        self.goto_index((self.current_tab() % n + n - 1) % n);
    }

    fn key_host_form(&mut self, key: KeyEvent) {
        let button = self
            .form
            .as_ref()
            .map(|f| f.focus)
            .filter(|f| f.is_button());
        match key.code {
            KeyCode::Esc => {
                self.cancel_form();
                return;
            }
            // On a button ↵ presses it, as Space does; from an input it saves,
            // as it always has.
            KeyCode::Enter => {
                match button {
                    Some(b) => self.press_form_button(b),
                    None => self.save_form(),
                }
                return;
            }
            KeyCode::Char(' ') if button.is_some() => {
                if let Some(b) = button {
                    self.press_form_button(b);
                }
                return;
            }
            // Key generation lives here now, on the F-key it always had.
            KeyCode::F(7) => {
                self.form_key();
                return;
            }
            // F2 opened this dialog from Hosts, so F2 again is "add, but many".
            KeyCode::F(2) => {
                self.open_bulk();
                return;
            }
            _ => {}
        }
        let prefix = self.cfg.mount_prefix.clone();
        let Some(form) = self.form.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Tab | KeyCode::Down => form.focus_next(),
            KeyCode::BackTab | KeyCode::Up => form.focus_prev(),
            // The cycler owns the arrows only while it is the focused field.
            // Everywhere else they move the caret, which is what a person
            // pressing ← in a text field means by it.
            KeyCode::Left if form.focus == FormField::Type => form.cycle_type(-1),
            KeyCode::Right if form.focus == FormField::Type => form.cycle_type(1),
            // Along the buttons the arrows walk them, as in any dialog.
            KeyCode::Left if form.focus.is_button() => form.focus_prev(),
            KeyCode::Right if form.focus.is_button() => form.focus_next(),
            // A button takes no text.
            _ if form.focus.is_button() => {}
            _ => {
                form.handle_key(key, &prefix);
            }
        }
    }

    /// What each of the form's buttons does — the same as its own key, so a
    /// button reached with Tab and one clicked or keyed cannot differ.
    fn press_form_button(&mut self, button: FormField) {
        match button {
            FormField::KeyButton => self.form_key(),
            FormField::BulkAdd => self.open_bulk(),
            FormField::Save => self.save_form(),
            FormField::Cancel => self.cancel_form(),
            _ => {}
        }
    }

    fn cancel_form(&mut self) {
        self.mode = Mode::Normal;
        self.form = None;
    }

    fn key_confirm_plan(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => self.reject_plan(),
            // F2 opens it, so F2 puts it away again — leaving the plan
            // waiting rather than deciding it.
            KeyCode::F(2) => self.hide_plan(),
            // One ↵ runs, however the dialog came to be up. It used to swallow
            // the first ↵ on a dialog that had opened by itself, in case that
            // keystroke was already in flight — but the gate could not tell a
            // stray key from an operator who had read the plan in silence, so
            // every such operator paid a second press for it.
            KeyCode::Enter => self.confirm_plan(),
            KeyCode::Char(' ') => {
                if let Some(sel) = self.plan.as_mut() {
                    sel.toggle_cursor();
                }
            }
            KeyCode::Up => {
                if let Some(sel) = self.plan.as_mut() {
                    sel.move_cursor(-1);
                }
            }
            KeyCode::Down => {
                if let Some(sel) = self.plan.as_mut() {
                    sel.move_cursor(1);
                }
            }
            // Pages move the view rather than the cursor: a script longer than
            // the dialog has lines no row sits on, and ↑/↓ would skip them.
            KeyCode::PageUp => {
                if let Some(sel) = self.plan.as_mut() {
                    sel.scroll_by(-(sel.page as isize));
                }
            }
            KeyCode::PageDown => {
                if let Some(sel) = self.plan.as_mut() {
                    sel.scroll_by(sel.page as isize);
                }
            }
            KeyCode::Home => {
                if let Some(sel) = self.plan.as_mut() {
                    sel.move_cursor(isize::MIN / 2);
                }
            }
            KeyCode::End => {
                if let Some(sel) = self.plan.as_mut() {
                    sel.move_cursor(isize::MAX / 2);
                }
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                if let Some(sel) = self.plan.as_mut() {
                    sel.set_all(true);
                }
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                if let Some(sel) = self.plan.as_mut() {
                    sel.set_all(false);
                }
            }
            _ => {}
        }
    }

    fn key_bulk(&mut self, key: KeyEvent) {
        let Some(b) = self.bulk.as_mut() else {
            self.close_bulk();
            return;
        };
        if key.code == KeyCode::Esc {
            self.close_bulk();
            return;
        }
        match b.step {
            // Enter is a newline here, not a submit: the list is lines, and in
            // a terminal without bracketed paste a paste arrives as keys with
            // an Enter between each line. Tab is the one key a list of names,
            // addresses and passwords never contains.
            BulkStep::Paste => match key.code {
                KeyCode::Tab => {
                    b.step = BulkStep::Review;
                    b.scroll = 0;
                }
                _ => {
                    b.text.input(key);
                }
            },
            BulkStep::Review => match key.code {
                KeyCode::BackTab | KeyCode::Left | KeyCode::Backspace => b.step = BulkStep::Paste,
                KeyCode::Up => b.scroll_by(-1),
                KeyCode::Down => b.scroll_by(1),
                KeyCode::PageUp => b.scroll_by(-(b.page as isize)),
                KeyCode::PageDown => b.scroll_by(b.page as isize),
                KeyCode::Home => b.scroll = 0,
                KeyCode::End => b.scroll = b.max_scroll,
                KeyCode::Enter => self.import_bulk(),
                _ => {}
            },
        }
    }

    fn key_confirm_delete(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.mode = Mode::Normal;
                self.pending_delete.clear();
            }
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => self.do_delete(),
            _ => {}
        }
    }

    // ---- mouse ----------------------------------------------------------

    pub fn on_mouse(&mut self, ev: MouseEvent) {
        let at = Position::new(ev.column, ev.row);
        if ev.kind == MouseEventKind::Moved {
            self.hover = Some(at);
            return;
        }
        self.hover = Some(at);

        // Modal for the mouse too. The old alert let a click straight through to
        // whatever was under it — including a dialog's buttons, which register
        // in the same frame — so only the Dismiss button answers while it is up.
        if self.alert.is_some() {
            if ev.kind == MouseEventKind::Down(MouseButton::Left)
                && self
                    .regions
                    .clicks
                    .iter()
                    .any(|(r, c)| *c == Click::Dismiss && r.contains(at))
            {
                self.dismiss_alert();
            }
            return;
        }

        // Before a terminal can have it: once a scrollbar is pressed, the drag
        // and the release are the bar's wherever the pointer goes.
        if self.drag_scrollbar(&ev, at) {
            return;
        }

        // A focused terminal that asked for mouse reporting gets the event.
        if self.mode == Mode::Normal
            && self.screen == Screen::Shells
            && self.forward_mouse_to_terminal(&ev, at)
        {
            return;
        }

        match ev.kind {
            MouseEventKind::ScrollDown => self.on_scroll(3, at),
            MouseEventKind::ScrollUp => self.on_scroll(-3, at),
            MouseEventKind::Down(MouseButton::Left) => self.on_left_click(at),
            _ => {}
        }
    }

    /// The pane under the pointer: its whole rect, title and scrollbar
    /// included, its index in the tab, and its session.
    fn pane_at(&self, at: Position) -> Option<(Rect, usize, SessionId)> {
        let (rect, pane) = self
            .regions
            .panes
            .iter()
            .find(|(r, _)| r.contains(at))
            .copied()?;
        let id = self.term.active_tab()?.panes.get(pane).copied()?;
        Some((rect, pane, id))
    }

    /// A press on a scrollbar, and the drag and release that follow it.
    /// Returns whether the event was the scrollbar's.
    ///
    /// On the thumb, a press takes hold where it lands and moves nothing; on
    /// the track it jumps, centring the thumb on the pointer, and dragging
    /// carries on from there. From the press to the release every event is
    /// the bar's, so a drag that wanders over a pane is never forwarded to a
    /// program that asked for the mouse — which never saw the press, and would
    /// not know what to make of its release.
    fn drag_scrollbar(&mut self, ev: &MouseEvent, at: Position) -> bool {
        if self.mode != Mode::Normal {
            self.scroll_drag = None;
            return false;
        }
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.scroll_drag = None;
                let Some((band, target)) = self
                    .regions
                    .scrollbars
                    .iter()
                    .find(|(r, _)| r.contains(at))
                    .copied()
                else {
                    return false;
                };
                let Some((g, pos)) = self.scrollbar_now(target, band) else {
                    return true;
                };
                let row = i32::from(at.y) - i32::from(band.y);
                let (start, len) = g.thumb(pos);
                let (start, len) = (start as i32, len as i32);
                if (start..start + len).contains(&row) {
                    self.scroll_drag = Some(ScrollDrag {
                        target,
                        grab: row - start,
                        row: start,
                    });
                } else {
                    self.scroll_drag = Some(ScrollDrag {
                        target,
                        grab: len / 2,
                        // Not a row the thumb can be on, so the jump applies.
                        row: i32::MIN,
                    });
                    self.drag_to(band, row);
                }
                true
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(d) = self.scroll_drag else {
                    return false;
                };
                match self.regions.scrollbars.iter().find(|(_, t)| *t == d.target) {
                    Some(&(band, _)) => self.drag_to(band, i32::from(at.y) - i32::from(band.y)),
                    // Its bar is gone — the list shrank to fit, the pane
                    // closed, a full-screen program took it over.
                    None => self.scroll_drag = None,
                }
                true
            }
            MouseEventKind::Up(MouseButton::Left) => self.scroll_drag.take().is_some(),
            _ => false,
        }
    }

    /// Move the held thumb so the pointer, `pointer` rows into the bar, stays
    /// where the thumb was taken.
    fn drag_to(&mut self, band: Rect, pointer: i32) {
        let Some(mut d) = self.scroll_drag else {
            return;
        };
        let Some((g, _)) = self.scrollbar_now(d.target, band) else {
            self.scroll_drag = None;
            return;
        };
        let (_, len) = g.thumb(0);
        // A thumb that fills its track has nowhere to go.
        if len >= g.track {
            return;
        }
        let row = (pointer - d.grab).clamp(0, (g.track - len) as i32);
        if row == d.row {
            return;
        }
        d.row = row;
        self.scroll_drag = Some(d);
        self.set_scroll_position(d.target, &g, g.position_at(row));
    }

    /// A scrollbar's numbers and where it stands, read from live state rather
    /// than the last frame: a shell's history grows while its thumb is held.
    fn scrollbar_now(&self, target: ScrollTarget, band: Rect) -> Option<(ScrollGeometry, usize)> {
        match target {
            ScrollTarget::Hosts => {
                let visible = band.height as usize;
                let len = self.view().len();
                if len <= visible {
                    return None;
                }
                Some((
                    crate::ui::screens::hosts::scrollbar_geometry(len, visible),
                    crate::ui::screens::hosts::window_start(self.cursor, len, visible),
                ))
            }
            ScrollTarget::Pane(id) => {
                let s = self.term.session(id)?;
                let mut p = s.parser().lock().ok()?;
                let h = crate::term::scrollback::history(p.screen_mut())?;
                Some((
                    crate::ui::screens::shells::scrollbar_geometry(h, band.height),
                    h.lines - h.offset,
                ))
            }
        }
    }

    /// Scroll `target` to a scrollbar position, 0 at the top.
    fn set_scroll_position(&mut self, target: ScrollTarget, g: &ScrollGeometry, position: usize) {
        match target {
            // The window follows the cursor, so the list scrolls by moving the
            // cursor to where the window starts at `position` — as the wheel
            // moves it.
            ScrollTarget::Hosts => self.set_cursor(position + g.viewport / 2),
            // Positions count down from the oldest line; vt100's offset counts
            // back from the live screen.
            ScrollTarget::Pane(id) => {
                if let Some(s) = self.term.session(id) {
                    s.scroll_to(g.max - position);
                }
            }
        }
    }

    /// Returns true when the event belonged to a terminal pane.
    fn forward_mouse_to_terminal(&mut self, ev: &MouseEvent, at: Position) -> bool {
        let Some((rect, pane, id)) = self.pane_at(at) else {
            return false;
        };
        // Pane-local coordinates, measured from the area the program actually
        // draws on. A single-pane tab draws no title rule, so the offset must
        // come from the same accessor the renderer used — hard-coding 1 here
        // would swallow the pane's first row and land every forwarded click one
        // line high. The title row and the scrollbar column are ours: a report
        // there would name a cell the program does not have.
        let term = TerminalManager::pane_areas(rect, self.term.pane_chrome_rows()).term;
        if !term.contains(at) {
            return false;
        }
        let col = at.x - term.x;
        let row = at.y - term.y;

        let (mode, encoding, ok) = match self.term.session(id) {
            Some(s) => match s.parser().lock() {
                Ok(p) => {
                    let sc = p.screen();
                    (sc.mouse_protocol_mode(), sc.mouse_protocol_encoding(), true)
                }
                Err(_) => (
                    vt100::MouseProtocolMode::None,
                    vt100::MouseProtocolEncoding::Default,
                    false,
                ),
            },
            None => return false,
        };
        if !ok {
            return false;
        }

        match crate::term::keys::encode_mouse(ev, col, row, mode, encoding) {
            Some(bytes) => {
                // Clicking also moves focus, so typing follows the mouse.
                if matches!(ev.kind, MouseEventKind::Down(_)) {
                    self.term.focus_pane(pane);
                }
                if let Some(s) = self.term.session_mut(id) {
                    let _ = s.write(&bytes);
                }
                true
            }
            None => false,
        }
    }

    fn on_scroll(&mut self, delta: isize, at: Position) {
        if self.mode == Mode::BulkImport {
            if let Some(b) = self.bulk.as_mut()
                && b.step == BulkStep::Review
            {
                b.scroll_by(delta);
            }
            return;
        }
        if self.mode == Mode::ConfirmPlan {
            if let Some(sel) = self.plan.as_mut() {
                sel.scroll_by(delta);
            }
            return;
        }
        match self.screen {
            Screen::Hosts if self.mode == Mode::Normal => self.move_cursor(delta),
            // Up the screen is back in time, so the sign is inverted.
            Screen::Chat if self.mode == Mode::Normal => self.chat.scroll_by(-delta),
            // A wheel the program did not ask for scrolls the pane under the
            // pointer back through its history — focused or not, as a forwarded
            // wheel would be. Whether it may is `scrollback::scroll`'s call:
            // not on the alternate screen, and not where the program owns the
            // mouse. Up is back in time here too.
            Screen::Shells if self.mode == Mode::Normal => {
                if let Some((_, _, id)) = self.pane_at(at)
                    && let Some(s) = self.term.session(id)
                {
                    s.scroll_history(-delta);
                }
            }
            _ => {}
        }
    }

    fn on_left_click(&mut self, at: Position) {
        // Dialogs own every click while they are up.
        if self.mode != Mode::Normal {
            if let Some(click) = self
                .regions
                .clicks
                .iter()
                .find(|(r, _)| r.contains(at))
                .map(|(_, c)| *c)
            {
                self.dispatch_click(click);
            } else if matches!(self.mode, Mode::Help | Mode::ShowKey) {
                // Help and the key dialog are dismissed by looking away from
                // them. The form and bulk import are not: a stray click must not
                // throw away what was typed or pasted.
                self.close_popup();
            } else if self.mode == Mode::Filter {
                // Nothing typed there is lost that the filter held: it is
                // left as it was.
                self.close_filter();
            }
            return;
        }

        // A shell tab's `×` before the tab it sits in.
        if let Some((_, i)) = self
            .regions
            .shell_closes
            .iter()
            .find(|(r, _)| r.contains(at))
            .copied()
        {
            self.term.close_tab(i);
            self.after_shell_closed();
            return;
        }

        // Header tabs.
        if let Some((_, tab)) = self
            .regions
            .screen_tabs
            .iter()
            .find(|(r, _)| r.contains(at))
            .copied()
        {
            self.goto(tab);
            return;
        }

        // Function bar.
        if at.y == self.regions.fn_bar_y
            && let Some(&(_, _, fk)) = self
                .regions
                .fkeys
                .iter()
                .find(|(x0, x1, _)| at.x >= *x0 && at.x < *x1)
        {
            if fk == BAR_SHELL {
                self.open_shell();
            } else {
                self.function_key(fk);
            }
            return;
        }

        match self.screen {
            Screen::Hosts => self.click_hosts(at),
            Screen::Shells => self.click_shells(at),
            Screen::Chat => self.click_chat(at),
        }
    }

    /// The plan card is the transcript's one clickable thing: with the mouse
    /// working everywhere else, a card that says "click here" has to.
    fn click_chat(&mut self, at: Position) {
        if self.regions.plan_card.is_some_and(|r| r.contains(at)) {
            self.open_plan();
        }
    }

    fn click_hosts(&mut self, at: Position) {
        if self.regions.rows.contains(at) {
            let offset = (at.y - self.regions.rows.y) as usize;
            let idx = self.regions.row_start + offset;
            if idx < self.view().len() {
                let double = self.is_double_click(at.y);
                self.set_cursor(idx);
                if double {
                    self.open_shell();
                }
            }
        }
    }

    fn click_shells(&mut self, at: Position) {
        if let Some((_, pane)) = self
            .regions
            .panes
            .iter()
            .find(|(r, _)| r.contains(at))
            .copied()
        {
            self.term.focus_pane(pane);
        }
    }

    fn dispatch_click(&mut self, click: Click) {
        match click {
            Click::Key(code) => self.on_key(KeyEvent::new(code, KeyModifiers::empty())),
            Click::FocusField(f) => {
                if let Some(form) = self.form.as_mut() {
                    form.focus = f;
                }
            }
            Click::CycleType(d) => {
                if let Some(form) = self.form.as_mut() {
                    form.cycle_type(d);
                }
            }
            Click::ToggleStep(i) => {
                if let Some(sel) = self.plan.as_mut() {
                    sel.toggle_step(i as usize);
                }
            }
            Click::ToggleHost(i, j) => {
                if let Some(sel) = self.plan.as_mut() {
                    sel.toggle_host(i as usize, j as usize);
                }
            }
            Click::FormButton(b) => self.press_form_button(b),
            Click::Dismiss => self.dismiss_alert(),
        }
    }

    fn is_double_click(&mut self, row: u16) -> bool {
        let double = self
            .last_click
            .is_some_and(|(t, r)| r == row && t.elapsed() < DOUBLE_CLICK);
        self.last_click = if double {
            None
        } else {
            Some((Instant::now(), row))
        };
        double
    }

    // ---- helpers for the renderer ---------------------------------------

    /// The status-bar hint for whatever the pointer is over.
    pub fn hover_hint(&self) -> Option<String> {
        let at = self.hover?;
        if self.mode != Mode::Normal || self.screen != Screen::Hosts {
            return None;
        }
        if !self.regions.rows.contains(at) {
            return None;
        }
        let idx = self.regions.row_start + (at.y - self.regions.rows.y) as usize;
        let h = self.hosts.get(*self.view().get(idx)?)?;
        Some(format!(
            "{}@{}:{} · {} · double-click for a shell",
            h.login,
            h.addr,
            h.port,
            if h.mounted {
                format!("mounted at {}", h.mount_point)
            } else {
                "not mounted".to_string()
            }
        ))
    }

    /// Index of the host row under the pointer, for the hover highlight.
    pub fn hovered_row(&self) -> Option<usize> {
        let at = self.hover?;
        if !self.regions.rows.contains(at) {
            return None;
        }
        let idx = self.regions.row_start + (at.y - self.regions.rows.y) as usize;
        (idx < self.view().len()).then_some(idx)
    }

    pub fn is_hovered(&self, rect: Rect) -> bool {
        self.hover.is_some_and(|p| rect.contains(p))
    }

    /// Name a new host's mount point would get, for the form hint.
    pub fn auto_mount_for(&self, name: &str) -> String {
        mount_for(&self.cfg.mount_prefix, name)
    }
}
