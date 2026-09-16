//! Application state and event routing.
//!
//! Structure follows `/root/cfdns/src/app/mod.rs`: a flat `Mode` enum (no
//! screen stack), a `Regions` hit-test table repopulated by the render pass,
//! and a `Click` enum whose dominant variant synthesizes a key event so the
//! mouse reuses the keyboard handlers verbatim.

pub mod approve;
pub mod chat;
pub mod form;
pub mod mount_job;

use crate::agent::hosts::HostWrite;
use crate::agent::plan::Plan;
use crate::agent::{AgentCommand, AgentEvent, ExecStream, Stream, Worker};
use crate::config::Config;
use crate::db::DataBase;
use crate::db::model::{HostRecord, mount_for};
use crate::term::manager::TerminalManager;
use crate::term::session::{Spawn, TermEvent};
use crate::{keys, mount, mtab, ssh};
use approve::{ConfirmedPlan, PlanSelection};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Hosts,
    Shells,
    Chat,
}

impl Screen {
    pub const ALL: [Screen; 3] = [Screen::Hosts, Screen::Shells, Screen::Chat];

    pub fn label(self) -> &'static str {
        match self {
            Screen::Hosts => "Hosts",
            Screen::Shells => "Shells",
            Screen::Chat => "Chat",
        }
    }

    fn index(self) -> usize {
        Screen::ALL.iter().position(|s| *s == self).unwrap_or(0)
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
            text: "Alt+1/2/3 switch screens · Insert marks hosts · F1 help".to_string(),
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
    GenKey,
    /// The error dialog's button.
    Dismiss,
}

/// Clickable screen regions, recaptured every render.
#[derive(Default, Clone)]
pub struct Regions {
    /// Host table data rows, and the host index drawn on the first of them.
    pub rows: Rect,
    pub row_start: usize,
    /// Header screen tabs.
    pub screen_tabs: Vec<(Rect, Screen)>,
    /// Function-bar row and the `(x_start, x_end, fkey)` of each cap.
    pub fn_bar_y: u16,
    pub fkeys: Vec<(u16, u16, u8)>,
    /// Shell tab labels and their `×` buttons, by tab index.
    pub shell_tabs: Vec<(Rect, usize)>,
    pub shell_closes: Vec<(Rect, usize)>,
    /// Terminal panes of the active tab, by pane index.
    pub panes: Vec<(Rect, usize)>,
    /// The pending plan's card in the transcript, while it is on screen.
    pub plan_card: Option<Rect>,
    /// Dialog controls, captured per render.
    pub clicks: Vec<(Rect, Click)>,
}

impl Regions {
    /// Cleared at the top of every frame; the renderers refill it.
    pub fn clear(&mut self) {
        self.screen_tabs.clear();
        // u16::MAX is off-screen, so a screen that draws no function bar
        // cannot inherit the previous one's row.
        self.fn_bar_y = u16::MAX;
        self.fkeys.clear();
        self.shell_tabs.clear();
        self.shell_closes.clear();
        self.panes.clear();
        self.plan_card = None;
        self.clicks.clear();
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

    pub form: Option<FormState>,
    pub pending_delete: Vec<i64>,
    /// The mount in progress, while `Mode::Mounting` shows it.
    pub mounting: Option<mount_job::MountJob>,
    /// `(host name, public key)` for the SSH Public Key dialog.
    pub key_dialog: Option<(String, String)>,
    pub alert: Option<String>,

    pub chat: ChatState,
    /// `None` until a model is configured.
    pub agent: Option<AgentHandle>,
    /// Where agent events are sent, so a worker can be started later.
    agent_events: Sender<AgentEvent>,
    /// A proposal waiting for the operator. It suspends the turn, so the
    /// dialog shows itself as soon as that is free (`maybe_auto_open_plan`) —
    /// but disarmed, because a modal appearing under the fingers is how a
    /// reflexive Enter authorizes a fleet-wide run.
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
            form: None,
            pending_delete: Vec::new(),
            mounting: None,
            key_dialog: None,
            alert: None,
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
    pub(crate) fn agent_hosts(&self) -> Vec<HostRecord> {
        self.hosts
            .iter()
            .filter(|h| h.proto.eq_ignore_ascii_case("ssh"))
            .cloned()
            .collect()
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
                hosts: self.agent_hosts(),
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
                plan,
                hosts: self.agent_hosts(),
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
                match self.hosts.iter().find(|h| &h.name == name) {
                    Some(h) => Some(h.clone()),
                    // Only reachable if the record went away between the tool
                    // validating and this running. The operator is told, because
                    // they are the one who asked for the change.
                    None => {
                        self.fail(format!("No host called {name} to edit."));
                        return;
                    }
                }
            }
        };
        let creating = existing.is_none();
        let rec = write.apply(existing.as_ref(), &self.cfg.mount_prefix);
        let name = rec.name.clone();
        match self.db.save(&rec) {
            Ok(_) => {
                // The cursor is left where it is: the operator may be reading
                // the Hosts screen, and a write they did not initiate should not
                // move what is under their fingers.
                self.reload();
                let what = if creating { "added" } else { "changed" };
                self.flash(format!("Agent {what} host {name}."), StatusKind::Warn);
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
                self.hosts = hosts;
                let ids: HashSet<i64> = self.hosts.iter().map(|h| h.id).collect();
                self.marked.retain(|id| ids.contains(id));
                self.clamp_cursor();
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

    pub fn host(&self) -> Option<&HostRecord> {
        self.hosts.get(self.cursor)
    }

    /// The hosts an action applies to: every marked host, else the cursor row.
    pub fn targets(&self) -> Vec<HostRecord> {
        if self.marked.is_empty() {
            self.host().cloned().into_iter().collect()
        } else {
            self.hosts
                .iter()
                .filter(|h| self.marked.contains(&h.id))
                .cloned()
                .collect()
        }
    }

    pub fn proxy_host(&self) -> Option<&HostRecord> {
        self.hosts.iter().find(|h| h.proxy)
    }

    pub fn mounted_count(&self) -> usize {
        self.hosts.iter().filter(|h| h.mounted).count()
    }

    fn clamp_cursor(&mut self) {
        if self.hosts.is_empty() {
            self.cursor = 0;
            self.scroll = 0;
        } else if self.cursor >= self.hosts.len() {
            self.cursor = self.hosts.len() - 1;
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        if self.hosts.is_empty() {
            return;
        }
        let last = self.hosts.len() as isize - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
    }

    pub fn set_cursor(&mut self, i: usize) {
        if i < self.hosts.len() {
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

    fn invert_marks(&mut self) {
        let all: HashSet<i64> = self.hosts.iter().map(|h| h.id).collect();
        self.marked = all.difference(&self.marked).copied().collect();
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
                if let Some(i) = self.hosts.iter().position(|h| h.id == id) {
                    self.cursor = i;
                }
                let name = rec.name.clone();
                if editing {
                    self.ok(format!("Saved {name}."));
                } else {
                    self.ok(format!("Added {name}."));
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
            Mode::ShowKey | Mode::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::F(1)) {
                    self.close_popup();
                }
            }
            Mode::Normal => match self.screen {
                // The Shells screen hands every key to the terminal; the app
                // is reachable only with the mouse, via the header tabs.
                Screen::Shells => self.key_shells(key),
                Screen::Hosts => self.key_hosts(key),
                Screen::Chat => self.key_chat(key),
            },
        }
    }

    /// Keys shared by the Hosts and Chat screens: real F-keys, Alt+digit, and
    /// the mc-style Esc+digit prefix.
    fn key_global(&mut self, key: KeyEvent) -> bool {
        // Alt+←/→ walk the screens, wrapping in both directions.
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Left => {
                    self.prev_screen();
                    return true;
                }
                KeyCode::Right => {
                    self.cycle_screen();
                    return true;
                }
                _ => {}
            }
        }
        if key.modifiers.contains(KeyModifiers::ALT)
            && let KeyCode::Char(c) = key.code
            && let Some(d) = c.to_digit(10)
        {
            if (1..=3).contains(&d) {
                self.set_screen(Screen::ALL[d as usize - 1]);
            } else {
                self.function_key(d as u8);
            }
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
            KeyCode::End => self.cursor = self.hosts.len().saturating_sub(1),
            KeyCode::Insert | KeyCode::Char(' ') => self.toggle_mark(),
            KeyCode::Char('*') => self.invert_marks(),
            // Before the plain letter below, or Ctrl+A would add a host.
            KeyCode::Char('a') if ctrl => {
                self.marked = self.hosts.iter().map(|h| h.id).collect();
            }
            // Letter twins of the F-keys, for keyboards and terminals where the
            // F-row is awkward. Either case, so Caps Lock is not a trap.
            KeyCode::Char('a' | 'A') => self.open_add(),
            KeyCode::Char('e' | 'E') => self.open_edit(),
            // F9 toggles; these say which way, so a mixed selection does what
            // was asked rather than what qhostman's rule would guess.
            KeyCode::Char('m' | 'M') => self.set_mount(Some(true)),
            KeyCode::Char('u' | 'U') => self.set_mount(Some(false)),
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

    /// The Shells screen: nothing is reserved except the Esc prefix, so mc,
    /// GNU Screen and vim all keep their full keyboard.
    fn key_shells(&mut self, key: KeyEvent) {
        // With no pane to type into there is nothing to be transparent to, and
        // capturing here is the difference between "no shells open" and "no way
        // out without a mouse" — so the normal app keys work in the empty state.
        // The one exception to the rule below: Alt+←/→ walk the screens, so
        // there is a keyboard way out of a terminal and not only a click.
        // mc, vim and GNU Screen bind neither chord by default.
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Left => {
                    self.prev_screen();
                    return;
                }
                KeyCode::Right => {
                    self.cycle_screen();
                    return;
                }
                _ => {}
            }
        }

        let Some(id) = self.term.focused_session() else {
            self.key_hosts_or_global(key);
            return;
        };

        // Otherwise a focused pane takes every key: F1-F10, Tab, Ctrl+A,
        // Alt+letter, Alt+digit and Esc. mc reads Esc+digit as its own F-key
        // emulation and Alt as its menu shortcuts, so reserving any of them
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
            _ => {
                if self.mode == Mode::HostForm {
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
            }
        }
    }

    fn write_terminal(&mut self, bytes: &[u8]) {
        if let Err(e) = self.term.write_focused(bytes) {
            self.fail(format!("Terminal write failed: {e}"));
        }
    }

    /// One entry point for real F-keys, Alt+digit and function-bar clicks, so
    /// all three share the same behavior. On the Shells screen only the clicks
    /// reach it: a focused terminal keeps the whole keyboard.
    pub fn function_key(&mut self, n: u8) {
        // A click on the bar reaches here in any mode, so a dialog's own keys are
        // honoured and nothing behind it is: F10 must not quit out from under an
        // unsaved form.
        if self.mode == Mode::HostForm {
            if n == 7 {
                self.form_key();
            }
            return;
        }
        if self.mode == Mode::Mounting {
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
            // F9 is Mount here rather than the screen cycle it is on the other two
            // screens; Alt+←/→ and Alt+1…3 still switch from Hosts.
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
                3 => self.term.next_tab(),
                4 => {
                    if !self.term.is_empty() {
                        self.term.close_active_tab();
                        self.ok("Shell closed.");
                        self.after_shell_closed();
                    }
                }
                5 => self.set_screen(Screen::Hosts),
                9 => self.cycle_screen(),
                _ => {}
            },
            Screen::Chat => match n {
                2 => self.open_plan(),
                9 => self.cycle_screen(),
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
    /// a host, `exit`, is exactly where they wanted to be. Only the transition
    /// moves anyone: visiting an empty Shells screen with Alt+2 stays put.
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

    fn cycle_screen(&mut self) {
        self.set_screen(Screen::ALL[(self.screen.index() + 1) % Screen::ALL.len()]);
    }

    fn prev_screen(&mut self) {
        let n = Screen::ALL.len();
        self.set_screen(Screen::ALL[(self.screen.index() + n - 1) % n]);
    }

    fn key_host_form(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.form = None;
                return;
            }
            KeyCode::Enter => {
                self.save_form();
                return;
            }
            // Key generation lives here now, on the F-key it always had.
            KeyCode::F(7) => {
                self.form_key();
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
            _ => {
                form.handle_key(key, &prefix);
            }
        }
    }

    fn key_confirm_plan(&mut self, key: KeyEvent) {
        // Every key but ↵ says the operator is looking at the dialog, so it
        // arms the one that isn't safe to guess at.
        if key.code != KeyCode::Enter
            && let Some(sel) = self.plan.as_mut()
        {
            sel.arm();
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => self.reject_plan(),
            // F2 opens it, so F2 puts it away again — leaving the plan
            // waiting rather than deciding it.
            KeyCode::F(2) => self.hide_plan(),
            KeyCode::Enter => {
                // A dialog that opened by itself absorbs the first ↵. The
                // operator may have had a keystroke in flight when it
                // appeared, and that keystroke must not run scripts on a
                // fleet. The second one is theirs.
                if self.plan.as_ref().is_some_and(|s| !s.armed) {
                    if let Some(sel) = self.plan.as_mut() {
                        sel.arm();
                    }
                    self.flash("Read the plan — ↵ again to run it.", StatusKind::Warn);
                    return;
                }
                self.confirm_plan()
            }
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
        // Moving a pointer onto a modal is something only a person who can see
        // it does, so any mouse event arms an auto-opened plan dialog — which
        // is what lets its Run button work on the first click.
        if self.mode == Mode::ConfirmPlan
            && let Some(sel) = self.plan.as_mut()
        {
            sel.arm();
        }
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

        // A focused terminal that asked for mouse reporting gets the event.
        if self.mode == Mode::Normal
            && self.screen == Screen::Shells
            && self.forward_mouse_to_terminal(&ev, at)
        {
            return;
        }

        match ev.kind {
            MouseEventKind::ScrollDown => self.on_scroll(3),
            MouseEventKind::ScrollUp => self.on_scroll(-3),
            MouseEventKind::Down(MouseButton::Left) => self.on_left_click(at),
            _ => {}
        }
    }

    /// Returns true when the event belonged to a terminal pane.
    fn forward_mouse_to_terminal(&mut self, ev: &MouseEvent, at: Position) -> bool {
        let Some((rect, pane)) = self
            .regions
            .panes
            .iter()
            .find(|(r, _)| r.contains(at))
            .copied()
        else {
            return false;
        };
        let Some(tab) = self.term.active_tab() else {
            return false;
        };
        let Some(id) = tab.panes.get(pane).copied() else {
            return false;
        };

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

        // Pane-local coordinates. A single-pane tab draws no title rule, so
        // the offset must come from the same accessor the renderer used —
        // hard-coding 1 here would swallow the pane's first row and land every
        // forwarded click one line high.
        let chrome = self.term.pane_chrome_rows();
        if at.y < rect.y + chrome {
            return false;
        }
        let col = at.x.saturating_sub(rect.x);
        let row = at.y.saturating_sub(rect.y + chrome);

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

    fn on_scroll(&mut self, delta: isize) {
        match self.screen {
            Screen::Hosts if self.mode == Mode::Normal => self.move_cursor(delta),
            // Up the screen is back in time, so the sign is inverted.
            Screen::Chat if self.mode == Mode::Normal => self.chat.scroll_by(-delta),
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
                self.close_popup();
            }
            return;
        }

        // Header screen tabs.
        if let Some((_, screen)) = self
            .regions
            .screen_tabs
            .iter()
            .find(|(r, _)| r.contains(at))
            .copied()
        {
            self.set_screen(screen);
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
            if idx < self.hosts.len() {
                let double = self.is_double_click(at.y);
                self.set_cursor(idx);
                if double {
                    self.open_shell();
                }
            }
        }
    }

    fn click_shells(&mut self, at: Position) {
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
        if let Some((_, i)) = self
            .regions
            .shell_tabs
            .iter()
            .find(|(r, _)| r.contains(at))
            .copied()
        {
            self.term.select_tab(i);
            return;
        }
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
            Click::GenKey => self.form_key(),
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
        let h = self.hosts.get(idx)?;
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
        (idx < self.hosts.len()).then_some(idx)
    }

    pub fn is_hovered(&self, rect: Rect) -> bool {
        self.hover.is_some_and(|p| rect.contains(p))
    }

    /// Name a new host's mount point would get, for the form hint.
    pub fn auto_mount_for(&self, name: &str) -> String {
        mount_for(&self.cfg.mount_prefix, name)
    }
}
