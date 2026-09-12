//! Application state and event routing.
//!
//! Structure follows `/root/cfdns/src/app/mod.rs`: a flat `Mode` enum (no
//! screen stack), a `Regions` hit-test table repopulated by the render pass,
//! and a `Click` enum whose dominant variant synthesizes a key event so the
//! mouse reuses the keyboard handlers verbatim.

pub mod chat;
pub mod form;

use crate::config::Config;
use crate::db::DataBase;
use crate::db::model::{HostRecord, mount_for};
use crate::term::manager::TerminalManager;
use crate::term::session::{Spawn, TermEvent};
use crate::{keys, mount, mtab, ssh};
use chat::ChatState;
use form::{FormField, FormState};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

/// How long a transient status message stays before reverting.
const STATUS_REVERT: Duration = Duration::from_millis(3400);
const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const SPINNER_TICK: Duration = Duration::from_millis(80);
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
    HostForm,
    ConfirmDelete,
    ShowKey,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Idle,
    Ok,
    Warn,
    Err,
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
    FocusField(FormField),
    CycleType(i32),
    GenKey,
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
    /// `[gen]` cells in the KEY column, by host index.
    pub genkeys: Vec<(Rect, usize)>,
    /// Shell tab labels and their `×` buttons, by tab index.
    pub shell_tabs: Vec<(Rect, usize)>,
    pub shell_closes: Vec<(Rect, usize)>,
    /// Terminal panes of the active tab, by pane index.
    pub panes: Vec<(Rect, usize)>,
    /// Dialog controls, captured per render.
    pub clicks: Vec<(Rect, Click)>,
}

impl Regions {
    /// Cleared at the top of every frame; the renderers refill it.
    pub fn clear(&mut self) {
        self.screen_tabs.clear();
        self.fkeys.clear();
        self.genkeys.clear();
        self.shell_tabs.clear();
        self.shell_closes.clear();
        self.panes.clear();
        self.clicks.clear();
    }
}

pub struct App {
    pub screen: Screen,
    pub mode: Mode,

    pub hosts: Vec<HostRecord>,
    pub cursor: usize,
    pub scroll: usize,
    pub marked: HashSet<i64>,

    pub form: Option<FormState>,
    pub pending_delete: Vec<i64>,
    /// `(host name, public key)` for the SSH Public Key dialog.
    pub key_dialog: Option<(String, String)>,
    pub alert: Option<String>,

    pub chat: ChatState,
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
    pub fn new(db: DataBase, cfg: Config, datadir: PathBuf, term_tx: Sender<TermEvent>) -> Self {
        let mut app = App {
            screen: Screen::Hosts,
            mode: Mode::Normal,
            hosts: Vec::new(),
            cursor: 0,
            scroll: 0,
            marked: HashSet::new(),
            form: None,
            pending_delete: Vec::new(),
            key_dialog: None,
            alert: None,
            chat: ChatState::seeded(),
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
        app
    }

    // ---- host list ------------------------------------------------------

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

    pub fn fail(&mut self, text: impl Into<String>) {
        self.flash(text, StatusKind::Err);
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

    pub fn tick_spinner(&mut self) {
        if self.busy && self.last_spin.elapsed() >= SPINNER_TICK {
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

    pub fn gen_key(&mut self, index: Option<usize>) {
        let Some(h) = index
            .and_then(|i| self.hosts.get(i))
            .or_else(|| self.host())
            .cloned()
        else {
            return;
        };
        match keys::generate(&self.datadir, &h.name) {
            Ok((key_name, public)) => {
                let mut rec = h.clone();
                rec.key_name = key_name;
                if let Err(e) = self.db.save(&rec) {
                    self.fail(format!("Key generated but not saved: {e}"));
                    return;
                }
                self.reload();
                self.key_dialog = Some((h.name.clone(), public));
                self.mode = Mode::ShowKey;
            }
            Err(e) => self.fail(format!("{e}")),
        }
    }

    fn show_key(&mut self) {
        let Some(h) = self.host().cloned() else {
            return;
        };
        if h.key_name.is_empty() {
            self.gen_key(None);
            return;
        }
        match keys::public_key(&self.datadir, &h.key_name) {
            Ok(public) => {
                self.key_dialog = Some((h.name, public));
                self.mode = Mode::ShowKey;
            }
            Err(e) => self.fail(format!("Could not read the public key: {e}")),
        }
    }

    fn toggle_mount(&mut self) {
        let targets = self.targets();
        if targets.is_empty() {
            return;
        }
        // qhostman's rule: if any target is unmounted, the action is Mount.
        let mounting = targets.iter().any(|h| !h.mounted);
        let mut done = 0usize;
        for h in &targets {
            let res = if mounting {
                mount::mount(h, &self.datadir, &self.cfg)
            } else {
                mount::unmount(h)
            };
            match res {
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
            "{} {done} host{}.",
            if mounting { "Mounted" } else { "Unmounted" },
            if done == 1 { "" } else { "s" }
        ));
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
                self.screen = Screen::Shells;
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
        self.stop_proxy_tunnel();
        self.term.shutdown();
        self.should_quit = true;
    }

    // ---- key routing ----------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        // A blocking alert swallows the next key.
        if self.alert.is_some() {
            self.alert = None;
            return;
        }
        match self.mode {
            Mode::HostForm => self.key_host_form(key),
            Mode::ConfirmDelete => self.key_confirm_delete(key),
            Mode::ShowKey | Mode::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::F(1)) {
                    self.mode = Mode::Normal;
                    self.key_dialog = None;
                }
            }
            Mode::Normal => match self.screen {
                // The Shells screen hands every key to the terminal; the app is
                // reachable only through the Esc-digit chords.
                Screen::Shells => self.key_shells(key),
                Screen::Hosts => self.key_hosts(key),
                Screen::Chat => self.key_chat(key),
            },
        }
    }

    /// Keys shared by the Hosts and Chat screens: real F-keys, Alt+digit, and
    /// the mc-style Esc+digit prefix.
    fn key_global(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::ALT)
            && let KeyCode::Char(c) = key.code
            && let Some(d) = c.to_digit(10)
        {
            if (1..=3).contains(&d) {
                self.screen = Screen::ALL[d as usize - 1];
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
            KeyCode::Char('a') if ctrl => {
                self.marked = self.hosts.iter().map(|h| h.id).collect();
            }
            KeyCode::Esc => self.marked.clear(),
            KeyCode::Enter => self.open_edit(),
            KeyCode::Delete => self.open_delete(),
            _ => {}
        }
    }

    fn key_chat(&mut self, key: KeyEvent) {
        if self.key_global(key) {
            return;
        }
        match key.code {
            KeyCode::Enter => {
                if self.chat.send() {
                    self.flash("Agent backend is not wired up yet.", StatusKind::Warn);
                }
            }
            KeyCode::Backspace => {
                self.chat.draft.pop();
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.chat.draft.push(c);
            }
            _ => {}
        }
    }

    /// The Shells screen: nothing is reserved except the Esc prefix, so mc,
    /// GNU Screen and vim all keep their full keyboard.
    fn key_shells(&mut self, key: KeyEvent) {
        // With no pane to type into there is nothing to be transparent to, and
        // capturing here is the difference between "no shells open" and "no way
        // out without a mouse" — so the normal app keys work in the empty state.
        let Some(id) = self.term.focused_session() else {
            self.key_hosts_or_global(key);
            return;
        };

        // A focused pane takes *every* key: F1-F10, Tab, Ctrl+A, Alt+anything,
        // and Esc. mc reads Esc+digit as its own F-key emulation and Alt as its
        // menu shortcuts, so reserving any of them here would quietly break it.
        // The app is reachable with the mouse instead — the screen tabs and the
        // function bar are clickable.
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
            self.screen = Screen::Hosts;
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
            Screen::Chat if self.mode == Mode::Normal => self.chat.draft.push_str(text),
            _ => {
                if self.mode == Mode::HostForm {
                    let prefix = self.cfg.mount_prefix.clone();
                    if let Some(form) = self.form.as_mut() {
                        for c in text.chars().filter(|c| !c.is_control()) {
                            form.type_char(c, &prefix);
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
            Screen::Hosts => match n {
                2 => self.open_add(),
                3 => self.open_edit(),
                4 => self.toggle_mount(),
                5 => self.open_shell(),
                6 => self.toggle_proxy(),
                7 => self.show_key(),
                8 => self.open_delete(),
                9 => self.cycle_screen(),
                _ => {}
            },
            Screen::Shells => match n {
                2 => self.term.next_pane(),
                3 => self.term.next_tab(),
                4 => {
                    if !self.term.is_empty() {
                        self.term.close_active_tab();
                        self.ok("Shell closed.");
                    }
                }
                5 => self.screen = Screen::Hosts,
                9 => self.cycle_screen(),
                _ => {}
            },
            Screen::Chat => {
                if n == 9 {
                    self.cycle_screen();
                }
            }
        }
    }

    fn cycle_screen(&mut self) {
        self.screen = Screen::ALL[(self.screen.index() + 1) % Screen::ALL.len()];
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
            _ => {}
        }
        let prefix = self.cfg.mount_prefix.clone();
        let Some(form) = self.form.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Tab | KeyCode::Down => form.focus_next(),
            KeyCode::BackTab | KeyCode::Up => form.focus_prev(),
            KeyCode::Left if form.focus == FormField::Type => form.cycle_type(-1),
            KeyCode::Right if form.focus == FormField::Type => form.cycle_type(1),
            KeyCode::Backspace => form.backspace(&prefix),
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                form.type_char(c, &prefix);
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
        if ev.kind == MouseEventKind::Moved {
            self.hover = Some(at);
            return;
        }
        self.hover = Some(at);

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

        // Pane-local coordinates, below the 1-row title border.
        let col = at.x.saturating_sub(rect.x);
        let row =
            at.y.saturating_sub(rect.y + crate::term::manager::PANE_CHROME_ROWS);
        if at.y < rect.y + crate::term::manager::PANE_CHROME_ROWS {
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

    fn on_scroll(&mut self, delta: isize) {
        match self.screen {
            Screen::Hosts if self.mode == Mode::Normal => self.move_cursor(delta),
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
                self.mode = Mode::Normal;
                self.key_dialog = None;
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
            self.screen = screen;
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
            self.function_key(fk);
            return;
        }

        match self.screen {
            Screen::Hosts => self.click_hosts(at),
            Screen::Shells => self.click_shells(at),
            Screen::Chat => {}
        }
    }

    fn click_hosts(&mut self, at: Position) {
        // The `[gen]` cell is a button inside the row.
        if let Some((_, idx)) = self
            .regions
            .genkeys
            .iter()
            .find(|(r, _)| r.contains(at))
            .copied()
        {
            self.set_cursor(idx);
            self.gen_key(Some(idx));
            return;
        }
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
            Click::GenKey => {
                // From inside the form: generate against the typed name.
                let name = self
                    .form
                    .as_ref()
                    .map(|f| f.name.clone())
                    .unwrap_or_default();
                if name.trim().is_empty() {
                    self.fail("Name the host before generating a key.");
                    return;
                }
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
