//! OpenAdmin — a TUI for administering fleets of remote SSH machines.
//!
//! CLI mirrors qhostman's:
//!   --password <value>  unlock without the prompt (development only)
//!   --datadir  <value>  data directory (default ~/.openadmin)

mod agent;
mod app;
mod config;
mod db;
mod keys;
mod mount;
mod mtab;
mod ssh;
mod term;
mod ui;

#[cfg(test)]
mod render_tests;

use agent::AgentEvent;
use anyhow::{Context, Result};
use app::App;
use config::Config;
use db::DataBase;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::execute;
use std::io::stdout;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;
use term::session::TermEvent;
use tui_input::Input;
use tui_input::backend::crossterm::to_input_request;

/// Everything the event loop can wake on.
enum AppEvent {
    Input(Event),
    Term(TermEvent),
    Agent(AgentEvent),
}

struct Args {
    password: Option<String>,
    datadir: Option<String>,
}

/// Accepts both `--opt value` and `--opt=value`, like qhostman's
/// QCommandLineParser. The env vars are test-harness substitutes.
fn parse_args() -> Args {
    let mut args = Args {
        password: None,
        datadir: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut grab = |name: &str, slot: &mut Option<String>| {
            if arg == name {
                *slot = it.next();
                true
            } else if let Some(v) = arg.strip_prefix(&format!("{name}=")) {
                *slot = Some(v.to_string());
                true
            } else {
                false
            }
        };
        let _ = grab("--password", &mut args.password) || grab("--datadir", &mut args.datadir);
    }
    args.password = args
        .password
        .or_else(|| std::env::var("OPENADMIN_PASSWORD").ok());
    args.datadir = args
        .datadir
        .or_else(|| std::env::var("OPENADMIN_DATADIR").ok());
    args
}

fn main() -> Result<()> {
    let args = parse_args();
    let datadir: PathBuf = match &args.datadir {
        Some(d) => PathBuf::from(d),
        None => dirs::home_dir()
            .context("no home directory")?
            .join(".openadmin"),
    };
    std::fs::create_dir_all(&datadir).context("create data directory")?;

    // The two directories the operator fills by hand. Created empty on every
    // run, because a directory nobody creates is a feature nobody finds: an
    // operator who cannot see `manuals/` does not know they may write one, and
    // `artifacts/` had the same problem — it was never created at all.
    agent::artifacts::ensure_dir(&datadir)?;
    agent::manuals::ensure_dir(&datadir)?;
    // The same discoverability for the prompt: a file nobody writes is a
    // prompt nobody knows they can change, so the built-in one is materialized
    // once and the operator owns it from then on.
    agent::ensure_system_prompt(&datadir)?;

    // Materialize every setting on disk, so the knobs are discoverable without
    // reading the source — including ones added since the file was written. A
    // file that predates a feature would otherwise never mention it, and the
    // only way to configure it would be to know the field name already.
    let cfg = Config::load(&datadir)?;
    cfg.save_if_changed(&datadir)?;
    let mut db = DataBase::new(datadir.join("openadmin.sqlite"));

    install_panic_hook();
    let mut terminal = ratatui::init();
    let _ = execute!(stdout(), EnableMouseCapture, EnableBracketedPaste);

    let result = run(&mut terminal, &mut db, cfg, datadir, args.password);

    let _ = execute!(stdout(), DisableMouseCapture, DisableBracketedPaste);
    ratatui::restore();
    result
}

/// Restore the terminal before the default panic handler prints, so a panic
/// never leaves the user in raw/alt-screen mode.
fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), DisableMouseCapture, DisableBracketedPaste);
        ratatui::restore();
        prev(info);
    }));
}

fn run(
    terminal: &mut DefaultTerminal,
    db: &mut DataBase,
    cfg: Config,
    datadir: PathBuf,
    password: Option<String>,
) -> Result<()> {
    if !unlock(terminal, db, password)? {
        return Ok(());
    }

    let (tx, rx) = channel::<AppEvent>();
    let (term_tx, term_rx) = channel::<TermEvent>();
    let (agent_tx, agent_rx) = channel::<AgentEvent>();
    spawn_input_thread(tx.clone());
    spawn_term_bridge(term_rx, tx.clone());
    spawn_agent_bridge(agent_rx, tx);

    let owned = std::mem::replace(db, DataBase::new(datadir.join("openadmin.sqlite")));
    let mut app = App::new(owned, cfg, datadir, term_tx, agent_tx);
    event_loop(terminal, &mut app, &rx)
}

/// Forward input from a blocking reader thread into the single event channel.
fn spawn_input_thread(tx: Sender<AppEvent>) {
    std::thread::Builder::new()
        .name("openadmin-input".into())
        .spawn(move || {
            while let Ok(ev) = event::read() {
                if tx.send(AppEvent::Input(ev)).is_err() {
                    return;
                }
            }
        })
        .expect("spawn input thread");
}

/// Merge PTY wake-ups into the same channel.
fn spawn_term_bridge(term_rx: Receiver<TermEvent>, tx: Sender<AppEvent>) {
    std::thread::Builder::new()
        .name("openadmin-term-bridge".into())
        .spawn(move || {
            while let Ok(ev) = term_rx.recv() {
                if tx.send(AppEvent::Term(ev)).is_err() {
                    return;
                }
            }
        })
        .expect("spawn terminal bridge");
}

/// Merge agent events into the same channel, exactly as the terminal bridge
/// does. The worker owns a `Sender<AgentEvent>`; this relabels them.
fn spawn_agent_bridge(agent_rx: Receiver<AgentEvent>, tx: Sender<AppEvent>) {
    std::thread::Builder::new()
        .name("openadmin-agent-bridge".into())
        .spawn(move || {
            while let Ok(ev) = agent_rx.recv() {
                if tx.send(AppEvent::Agent(ev)).is_err() {
                    return;
                }
            }
        })
        .expect("spawn agent bridge");
}

/// The main loop. Unlike cfdns's fixed 100 ms poll, this blocks on a channel so
/// PTY output redraws immediately; the timeout exists only to service the
/// spinner, the status auto-revert, and a held Esc.
fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    rx: &Receiver<AppEvent>,
) -> Result<()> {
    let mut dirty = true;
    while !app.should_quit {
        // A waiting plan shows itself as soon as doing so is free. Checked
        // here rather than where the plan arrives, so every route onto the
        // Chat screen is covered by one rule instead of five.
        if app.maybe_auto_open_plan() {
            dirty = true;
        }
        if dirty {
            terminal.draw(|f| ui::draw(f, app))?;
            dirty = false;
        }

        // Input and PTY output both arrive on the channel; the timeout only
        // services the spinner and the status auto-revert.
        let wait = if app.busy { 80 } else { 200 };
        match rx.recv_timeout(Duration::from_millis(wait)) {
            Ok(AppEvent::Input(ev)) => {
                match ev {
                    Event::Key(k) if k.kind == KeyEventKind::Press => {
                        if is_hard_quit(&k, app) {
                            app.quit();
                        } else {
                            app.on_key(k);
                        }
                    }
                    Event::Mouse(m) => app.on_mouse(m),
                    Event::Paste(text) => app.on_paste(&text),
                    Event::Resize(_, _) => {}
                    _ => {}
                }
                dirty = true;
            }
            Ok(AppEvent::Agent(ev)) => {
                app.on_agent_event(ev);
                dirty = true;
            }
            Ok(AppEvent::Term(ev)) => {
                // An exiting session leaves its final screen on display; the
                // tab is reaped below once every pane in it is gone.
                if let TermEvent::Exit(_) = ev {
                    app.term.reap_finished();
                }
                dirty = true;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        app.term.reap_finished();
        if app.term.take_dirty() {
            dirty = true;
        }
        let spin_before = app.spinner_frame;
        app.tick_spinner();
        let before = app.status.kind;
        app.maybe_revert_status();
        // `tick_spinner` mutates a counter nothing else compares, so without
        // this a spinning spinner only advances when something else happens to
        // redraw.
        if app.status.kind != before || app.spinner_frame != spin_before {
            dirty = true;
        }
    }
    app.term.shutdown();
    Ok(())
}

/// Ctrl+C quits from the Hosts and Chat screens. On the Shells screen it is a
/// signal the terminal needs, so it is passed through instead.
fn is_hard_quit(k: &KeyEvent, app: &App) -> bool {
    k.code == KeyCode::Char('c')
        && k.modifiers.contains(KeyModifiers::CONTROL)
        && app.screen != app::Screen::Shells
        && app.mode == app::Mode::Normal
        // While a turn is running, Ctrl+C means "stop that", not "quit".
        && !(app.screen == app::Screen::Chat && app.busy)
}

/// Pre-loop bootstrap: unlock an existing database, or create a new one. Runs
/// its own miniature event loop before `App` exists, the same shape as cfdns's
/// token setup (`/root/cfdns/src/main.rs:49-91`).
/// A password as the dialog shows it: one bullet per character, carrying the
/// real caret so it lands between the same two characters it would in the
/// value itself.
fn mask(input: &Input) -> Input {
    Input::new("•".repeat(input.value().chars().count())).with_cursor(input.cursor())
}

fn unlock(
    terminal: &mut DefaultTerminal,
    db: &mut DataBase,
    password: Option<String>,
) -> Result<bool> {
    let creating = !db.exists();

    if let Some(pw) = password {
        if creating {
            db.create(&pw)?;
        } else if !db.open(&pw)? {
            anyhow::bail!("the supplied password does not decrypt the database");
        }
        return Ok(true);
    }

    let mut first = Input::default();
    let mut confirm = Input::default();
    let mut focus = 0usize;
    let mut error: Option<String> = None;

    loop {
        terminal.draw(|f| {
            // What is shown is bullets, but the caret is the real one: moving
            // through a password you cannot read is the case that needs it
            // most. One bullet per character keeps the two in step.
            let masked_first = mask(&first);
            let masked_confirm = mask(&confirm);
            if creating {
                ui::dialogs::password_prompt(
                    f,
                    "Create Database",
                    &[
                        ("New password", &masked_first),
                        ("Confirm", &masked_confirm),
                    ],
                    focus,
                    error.as_deref(),
                    "At least 6 characters.   Tab next field   Enter create   Esc quit",
                );
            } else {
                ui::dialogs::password_prompt(
                    f,
                    "Unlock Database",
                    &[("Password", &masked_first)],
                    0,
                    error.as_deref(),
                    "Enter unlock   Esc quit",
                );
            }
        })?;

        let Event::Key(k) = event::read()? else {
            continue;
        };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        match k.code {
            KeyCode::Esc => return Ok(false),
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => return Ok(false),
            KeyCode::Tab | KeyCode::Down if creating => focus = (focus + 1) % 2,
            KeyCode::BackTab | KeyCode::Up if creating => focus = (focus + 1) % 2,
            // Everything else is line editing, so these fields navigate like
            // any other: arrows, Home/End, word motions, the readline kills.
            _ if to_input_request(&Event::Key(k)).is_some() => {
                let req = to_input_request(&Event::Key(k)).expect("just checked");
                if focus == 0 {
                    first.handle(req);
                } else {
                    confirm.handle(req);
                }
            }
            KeyCode::Enter => {
                if creating {
                    // Same rules as qhostman's CreateDatabaseDialog.
                    if first.value().chars().count() < 6 {
                        error = Some("Password must be at least 6 characters.".into());
                    } else if first.value() != confirm.value() {
                        error = Some("The passwords do not match.".into());
                    } else {
                        db.create(first.value())?;
                        return Ok(true);
                    }
                } else if db.open(first.value())? {
                    return Ok(true);
                } else {
                    error = Some("Wrong password.".into());
                    first.reset();
                }
            }
            _ => {}
        }
    }
}
