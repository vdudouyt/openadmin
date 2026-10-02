//! Headless render smoke tests and synthetic mouse/hover tests.
//!
//! Follows cfdns's two patterns (`/root/cfdns/src/render_tests.rs` and the
//! mouse unit tests at `/root/cfdns/src/app/mod.rs:1318-1431`): draw every mode
//! over a `TestBackend`, and drive `on_mouse` with hand-set `regions` so no
//! terminal is needed.

use crate::app::{App, Click, Mode, Screen, Tab};
use crate::config::Config;
use crate::db::DataBase;
use crate::db::model::HostRecord;
use crate::term::session::TermEvent;
use crate::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

/// A throwaway data directory + unlocked database, seeded with a few hosts.
fn test_app(tag: &str) -> (App, Receiver<TermEvent>) {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("openadmin-render-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut db = DataBase::new(dir.join("openadmin.sqlite"));
    db.create("test123").unwrap();
    for (name, proto, addr, port, login, pass, mount) in [
        (
            "web-01",
            "ssh",
            "10.0.4.11",
            22,
            "deploy",
            "hunter2",
            "/net/web-01",
        ),
        (
            "db-main",
            "ssh",
            "10.0.8.3",
            22,
            "postgres",
            "s3cret",
            "/net/db-main",
        ),
        ("nas", "ftp", "192.168.1.240", 21, "media", "", "/net/nas"),
    ] {
        db.save(&HostRecord {
            name: name.into(),
            proto: proto.into(),
            addr: addr.into(),
            port,
            login: login.into(),
            pass: pass.into(),
            mount_point: mount.into(),
            ..Default::default()
        })
        .unwrap();
    }

    let (tx, rx) = channel();
    // The agent receiver is leaked deliberately: these tests never start a
    // worker, and a dropped receiver would make every event send fail.
    let (agent_tx, agent_rx) = channel();
    std::mem::forget(agent_rx);
    (App::new(db, Config::default(), dir, tx, agent_tx), rx)
}

/// The shell tabs the header drew, by shell tab index.
fn shell_tabs(app: &App) -> Vec<(ratatui::layout::Rect, usize)> {
    app.regions
        .screen_tabs
        .iter()
        .filter_map(|(r, t)| match t {
            Tab::Shell(i) => Some((*r, *i)),
            _ => None,
        })
        .collect()
}

/// Hosts and Chat, as the header drew them.
fn pinned_tabs(app: &App) -> Vec<(ratatui::layout::Rect, Tab)> {
    app.regions
        .screen_tabs
        .iter()
        .filter(|(_, t)| !matches!(t, Tab::Shell(_)))
        .copied()
        .collect()
}

fn render(app: &mut App, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw(f, app)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn key(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::empty()));
}

fn click(app: &mut App, x: u16, y: u16) {
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::empty(),
    });
}

fn wheel(app: &mut App, down: bool) {
    app.on_mouse(MouseEvent {
        kind: if down {
            MouseEventKind::ScrollDown
        } else {
            MouseEventKind::ScrollUp
        },
        column: 10,
        row: 10,
        modifiers: KeyModifiers::empty(),
    });
}

fn moved(app: &mut App, x: u16, y: u16) {
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: x,
        row: y,
        modifiers: KeyModifiers::empty(),
    });
}

// ---- render smoke tests --------------------------------------------------

#[test]
fn hosts_screen_shows_the_table_and_function_bar() {
    let (mut app, _rx) = test_app("hosts");
    let out = render(&mut app, 120, 30);
    assert!(out.contains("OpenAdmin"), "{out}");
    assert!(out.contains("Known Hosts"));
    assert!(out.contains("NAME") && out.contains("MOUNT POINT") && out.contains("PRX"));
    assert!(out.contains("web-01") && out.contains("db-main"));
    // The keymap, as the bar advertises it: the letter twin beside each F-key,
    // and Shell on Enter.
    let bar = out.lines().last().unwrap();
    for cap in ["F2", "F4", "↵", "F6", "F8", "F9", "F10"] {
        assert!(bar.contains(cap), "{cap} missing from: {bar}");
    }
    // Just the keys. The letter twins are in F1's help, not on the caps.
    for pair in ["F2/a", "F4/e", "F9/m", "F9/u", "/"] {
        assert!(
            !bar.contains(pair),
            "{pair} should not be on the bar: {bar}"
        );
    }
    assert!(bar.contains("Shell") && bar.contains("Mount"), "{bar}");
    for gone in ["F3", "F5", "F7", "GenKey", "F12", "Actions"] {
        assert!(!bar.contains(gone), "{gone} should be gone: {bar}");
    }
    // Passwords are masked; an empty one shows a dash.
    assert!(!out.contains("hunter2"));
    assert!(out.contains("•"));
    // MNT, then PRX, then KEY last.
    let header = out.lines().find(|l| l.contains("NAME")).unwrap();
    let (m, p, k) = (
        header.find("MNT").unwrap(),
        header.find("PRX").unwrap(),
        header.find("KEY").unwrap(),
    );
    assert!(m < p && p < k, "column order: {header}");
    // MNT spells it out; KEY is a status circle, not a button — making a key is
    // the edit form's business.
    let web = out.lines().find(|l| l.contains("web-01")).unwrap();
    assert!(web.contains("[no]"), "{web}");
    assert!(!out.contains("[gen]"), "{out}");
    // The last four cells are MNT, PRX, KEY and the panel border.
    let cells: Vec<&str> = web.split_whitespace().collect();
    let tail = &cells[cells.len() - 4..];
    assert_eq!(tail[0], "[no]", "MNT leads the three: {web}");
    assert!(
        tail[2] == "○" || tail[2] == "●",
        "KEY is a circle, last: {web}"
    );
    assert_eq!(tail[3], "│", "then the border: {web}");
}

#[test]
fn every_dialog_renders() {
    let (mut app, _rx) = test_app("dialogs");
    for (mode, needle) in [
        (Mode::Help, "Key Bindings"),
        (Mode::HostForm, "Add Host"),
        (Mode::ConfirmDelete, "Confirm Delete"),
        (Mode::BulkImport, "Bulk Import · 1 of 2"),
    ] {
        let a = &mut app;
        a.mode = Mode::Normal;
        match mode {
            Mode::HostForm => a.open_add(),
            Mode::BulkImport => {
                a.open_add();
                key(a, KeyCode::F(2));
            }
            Mode::ConfirmDelete => {
                a.pending_delete = vec![a.hosts[0].id];
                a.mode = Mode::ConfirmDelete;
            }
            m => a.mode = m,
        }
        let out = render(a, 120, 34);
        assert!(out.contains(needle), "missing {needle} in:\n{out}");
    }
}

/// With no shell open the strip is Hosts and Chat, and no key leads to an
/// empty Shells screen.
#[test]
fn with_no_shell_open_the_strip_is_hosts_and_chat() {
    let (mut app, _rx) = test_app("shells");
    let out = render(&mut app, 120, 30);
    let tabs: Vec<Tab> = app.regions.screen_tabs.iter().map(|(_, t)| *t).collect();
    assert_eq!(tabs, [Tab::Hosts, Tab::Chat], "{out}");
    let row0 = out.lines().next().unwrap();
    assert!(
        row0.starts_with(" ███ OpenAdmin "),
        "the logo is tab 1: {out}"
    );
    assert!(row0.contains("2 Chat"), "{out}");
    assert!(!row0.contains("Hosts"), "the logo stands for Hosts: {out}");

    app.on_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Hosts, "there is no tab 3 yet");
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Hosts, "Alt+→ wraps over the two");
}

/// The Shells screen draws no function bar, so the header's tabs are the
/// whole mouse route off a focused pane and must stay registered.
#[test]
fn the_screen_tabs_survive_a_focused_pane() {
    use crate::term::session::Spawn;
    let (mut app, _rx) = test_app("escaperoute");
    let mut spawn = Spawn::new("/bin/sh");
    spawn.args = vec!["-c".into(), "sleep 30".into()];
    app.term
        .open_tab(
            vec![("local".into(), spawn)],
            (24, 80),
            50,
            "xterm",
            &app.term_tx,
        )
        .unwrap();
    app.screen = Screen::Shells;
    let out = render(&mut app, 120, 30);

    assert!(!out.contains("Help"), "no function bar here: {out}");
    assert!(app.regions.fkeys.is_empty(), "no F-key hitboxes");
    assert_eq!(pinned_tabs(&app).len(), 2, "{out}");
    assert_eq!(shell_tabs(&app).len(), 1, "{out}");
    // The shell is tab 3 of the same strip.
    assert!(out.lines().next().unwrap().contains("3 ● local"), "{out}");
    app.term.shutdown();
}

#[test]
fn chat_screen_renders_the_transcript_and_composer() {
    let (mut app, _rx) = test_app("chat");
    app.screen = Screen::Chat;
    let out = render(&mut app, 120, 34);
    assert!(out.contains("Agent"), "{out}");
    // No model is guessed, so the panel says so instead of showing a blank.
    assert!(out.contains("no model set"), "{out}");
    // The composer is a text area in its own orange box now, so it says what
    // it is for rather than wearing a "»" that would only mark its first line.
    assert!(
        out.contains("ask the agent"),
        "the composer invitation is missing: {out}"
    );
    // The transcript starts empty: a fabricated sample above the operator's
    // first real message would be worse than useless.
    assert!(app.chat.turns.is_empty());
}

#[test]
fn the_help_dialog_explains_the_shells_keyboard() {
    let (mut app, _rx) = test_app("helpchords");
    app.mode = Mode::Help;
    let out = render(&mut app, 120, 44);
    assert!(out.contains("takes every key"), "{out}");
    assert!(
        out.contains("mc, GNU Screen and vim"),
        "the why is stated: {out}"
    );
    // Asked of the help itself, not "switch screens" alone, which the Hosts
    // status bar also says — that is how a help cut off eight lines short
    // passed this test.
    assert!(
        out.contains("a header tab goes there"),
        "the mouse way out is documented: {out}"
    );
    assert!(out.contains("scroll back"), "and the wheel: {out}");
    assert!(
        out.contains("double-click opens a shell"),
        "down to the last line: {out}"
    );
    assert!(out.contains("a scrollbar"), "and the scrollbars: {out}");
    assert!(!out.contains("Esc then"), "stale chord docs: {out}");
}

/// The Shells screen spends exactly one row on itself — the header, carrying
/// both tab groups. Pinned so chrome cannot creep back.
#[test]
fn the_shells_screen_reaches_the_bottom_row() {
    use crate::term::session::Spawn;
    let (mut app, _rx) = test_app("chromebudget");
    let mut spawn = Spawn::new("/bin/sh");
    spawn.args = vec!["-c".into(), "sleep 30".into()];
    app.term
        .open_tab(
            vec![("local".into(), spawn)],
            (24, 80),
            50,
            "xterm",
            &app.term_tx,
        )
        .unwrap();
    app.screen = Screen::Shells;

    const H: u16 = 24;
    let _ = render(&mut app, 100, H);
    let (pane, _) = *app.regions.panes.first().expect("a rendered pane");
    assert_eq!(pane.y + pane.height, H, "the pane must run to the last row");
    assert_eq!(pane.y, 1, "the pane starts directly under the header");

    // header(1) is the whole budget: no tab strip, no title rule.
    assert_eq!(app.term.pane_chrome_rows(), 0);
    let id = app.term.focused_session().unwrap();
    // The width, less the scrollbar's own column.
    assert_eq!(
        app.term.session(id).unwrap().size(),
        (H - 1, 100 - crate::term::manager::PANE_SCROLLBAR_COLS)
    );

    // For contrast, Hosts keeps its status line and function bar.
    app.screen = Screen::Hosts;
    let out = render(&mut app, 130, H);
    let rows: Vec<&str> = out.lines().collect();
    assert!(
        rows[H as usize - 1].contains("F10"),
        "Hosts keeps its footer"
    );
    assert!(
        rows[H as usize - 2].contains("hosts"),
        "and its status line"
    );
    app.term.shutdown();
}

/// Helper: open `n` single-pane tabs on the Shells screen.
#[cfg(test)]
fn open_shells(app: &mut App, names: &[&str]) {
    use crate::term::session::Spawn;
    for name in names {
        let mut spawn = Spawn::new("/bin/sh");
        spawn.args = vec!["-c".into(), "sleep 30".into()];
        app.term
            .open_tab(
                vec![((*name).to_string(), spawn)],
                (24, 80),
                50,
                "xterm",
                &app.term_tx,
            )
            .unwrap();
    }
    app.screen = Screen::Shells;
}

/// A shell's tab goes from the strip with it; Hosts and Chat stay.
#[test]
fn the_shell_tab_goes_when_its_shell_closes() {
    let (mut app, _rx) = test_app("brandback");
    open_shells(&mut app, &["web-01"]);

    let out = render(&mut app, 120, 20);
    assert!(out.contains("web-01"), "{out}");

    app.term.close_active_tab();
    let out = render(&mut app, 120, 20);
    assert!(!out.contains("web-01"), "{out}");
    assert_eq!(app.regions.screen_tabs.len(), 2);
}

/// The logo is a tab, so it is always drawn at the left edge; where room is
/// short it sheds its wordmark, never the mark.
#[test]
fn the_logo_stays_when_shells_crowd_the_strip() {
    let (mut app, _rx) = test_app("brandyield");
    open_shells(&mut app, &["alpha", "bravo", "charlie", "delta", "echo"]);
    let out = render(&mut app, 80, 20);
    let row0 = out.lines().next().unwrap();
    assert!(row0.starts_with(" ███ OpenAdmin "), "{out}");
    assert!(row0.contains("echo"), "the showing shell is drawn: {out}");

    let out = render(&mut app, 34, 20);
    let row0 = out.lines().next().unwrap();
    assert!(row0.starts_with(" ███ "), "{out}");
    assert!(
        !row0.contains("OpenAdmin"),
        "the wordmark goes first: {out}"
    );
    assert!(row0.contains("echo"), "{out}");
    app.term.shutdown();
}

/// The logo is the Hosts tab: at column 0, clickable, and highlighted like
/// any tab while Hosts is showing.
#[test]
fn the_logo_is_the_hosts_tab() {
    let (mut app, _rx) = test_app("logotab");
    app.set_screen(Screen::Chat);
    let _ = render(&mut app, 120, 30);
    let (logo, _) = *app
        .regions
        .screen_tabs
        .iter()
        .find(|(_, t)| *t == Tab::Hosts)
        .unwrap();
    assert_eq!((logo.x, logo.y), (0, 0));
    click(&mut app, logo.x + 6, logo.y);
    assert_eq!(app.screen, Screen::Hosts);

    let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
    term.draw(|f| ui::draw(f, &mut app)).unwrap();
    let buf = term.backend().buffer().clone();
    let (chat, _) = pinned_tabs(&app)[1];
    let on = crate::ui::theme::ORANGE;
    assert_eq!(buf[(logo.x + 6, 0)].bg, on, "the logo is the showing tab");
    assert_ne!(buf[(1, 0)].fg, on, "its mark stays visible on the orange");
    assert_ne!(buf[(chat.x + 1, 0)].bg, on, "Chat is not showing");
}

/// The tabs sit beside the logo: Chat one gap after it, the first shell one
/// gap after Chat.
#[test]
fn the_tabs_sit_beside_the_logo() {
    let (mut app, _rx) = test_app("besidelogo");
    open_shells(&mut app, &["web-01"]);
    let _ = render(&mut app, 120, 20);
    let (logo, _) = pinned_tabs(&app)[0];
    let (chat, _) = pinned_tabs(&app)[1];
    let (shell, _) = shell_tabs(&app)[0];
    assert_eq!(chat.x, logo.x + logo.width + 1);
    assert_eq!(shell.x, chat.x + chat.width + 1);
    app.term.shutdown();
}

/// A lone pane needs no title: the header tab already names the host.
#[test]
fn a_single_pane_tab_draws_no_title_rule() {
    let (mut app, _rx) = test_app("notitle");
    open_shells(&mut app, &["web-01"]);
    let out = render(&mut app, 100, 20);

    assert_eq!(app.term.pane_chrome_rows(), 0);
    assert!(
        !out.contains("active"),
        "no 'active' label for a lone pane: {out}"
    );
    assert!(!out.contains("click to focus"), "{out}");
    // Row 0 is the header; row 1 is already terminal.
    let rows: Vec<&str> = out.lines().collect();
    assert!(rows[0].contains("web-01"), "the tab names the host: {out}");
    assert!(
        !rows[1].starts_with('─'),
        "row 1 must not be a title rule: {out}"
    );
    app.term.shutdown();
}

/// A stacked group still needs titles — that is what tells its panes apart.
#[test]
fn a_group_tab_keeps_a_title_per_pane() {
    use crate::term::session::Spawn;
    let (mut app, _rx) = test_app("grouptitles");
    let entries: Vec<(String, Spawn)> = ["a", "b"]
        .iter()
        .map(|n| {
            let mut spawn = Spawn::new("/bin/sh");
            spawn.args = vec!["-c".into(), "sleep 30".into()];
            ((*n).to_string(), spawn)
        })
        .collect();
    app.term
        .open_tab(entries, (24, 80), 50, "xterm", &app.term_tx)
        .unwrap();
    app.screen = Screen::Shells;

    let out = render(&mut app, 100, 24);
    assert_eq!(app.term.pane_chrome_rows(), 1);
    assert!(
        out.contains("active"),
        "the focused pane is labelled: {out}"
    );
    assert!(
        out.contains("click to focus"),
        "the other one invites a click: {out}"
    );
    // Both panes report a title rule.
    assert!(out.matches('─').count() >= 2, "{out}");
    app.term.shutdown();
}

/// A dead pane says so without naming a chord that was deleted. Only a group
/// keeps such a pane on screen — a lone one is reaped within the frame.
#[test]
fn an_exited_pane_reports_itself_without_naming_a_dead_chord() {
    use crate::term::session::Spawn;
    use std::time::{Duration, Instant};
    let (mut app, rx) = test_app("exited");

    let mut dies = Spawn::new("/bin/sh");
    dies.args = vec!["-c".into(), "exit 0".into()];
    let mut lives = Spawn::new("/bin/sh");
    lives.args = vec!["-c".into(), "sleep 30".into()];
    app.term
        .open_tab(
            vec![("gone".into(), dies), ("alive".into(), lives)],
            (24, 80),
            50,
            "xterm",
            &app.term_tx,
        )
        .unwrap();
    app.screen = Screen::Shells;

    let id = app.term.active_tab().unwrap().panes[0];
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !app.term.session(id).unwrap().has_exited() {
        let _ = rx.recv_timeout(Duration::from_millis(100));
    }
    assert!(app.term.session(id).unwrap().has_exited());

    // The group already spends a row per pane, so nothing about the geometry
    // moves when one of them dies.
    assert_eq!(app.term.pane_chrome_rows(), 1);
    let out = render(&mut app, 100, 20);
    assert!(out.contains("process exited"), "{out}");
    assert!(out.contains("click × to close"), "{out}");
    assert!(
        !out.contains("Esc-"),
        "no reference to the removed chords: {out}"
    );
    app.term.shutdown();
}

/// Regression: the tab cursor once advanced one column further than it drew, so
/// every hitbox after the first drifted right. Only reproducible with several
/// tabs open, which no earlier test did.
#[test]
fn shell_tab_hitboxes_land_on_the_tabs_that_were_drawn() {
    let (mut app, _rx) = test_app("tabhitbox");
    open_shells(&mut app, &["alpha", "bravo", "charlie"]);
    let _ = render(&mut app, 120, 20);

    assert_eq!(shell_tabs(&app).len(), 3, "all three tabs are registered");
    let header = render(&mut app, 120, 20);
    let row0 = header.lines().next().unwrap().chars().collect::<Vec<_>>();

    for (rect, i) in shell_tabs(&app) {
        let name = ["alpha", "bravo", "charlie"][i];
        let drawn: String = row0[rect.x as usize..(rect.x + rect.width) as usize]
            .iter()
            .collect();
        assert!(
            drawn.contains(name),
            "tab {i} hitbox {rect:?} covers {drawn:?}, not {name}"
        );
    }

    // The showing tab's × is drawn where its hitbox is.
    let (close, i) = app.regions.shell_closes[0];
    assert_eq!(i, 2, "charlie, the last opened, is showing");
    let drawn: String = row0[close.x as usize..(close.x + close.width) as usize]
        .iter()
        .collect();
    assert_eq!(drawn, " ×");

    // And clicking the third one selects the third one.
    let (rect, _) = shell_tabs(&app)[2];
    click(&mut app, rect.x + 2, rect.y);
    assert_eq!(app.term.active, Some(2));
    app.term.shutdown();
}

/// Many tabs must not crowd out the escape route, at any width.
#[test]
fn screen_tabs_outrank_shell_tabs_at_every_width() {
    let (mut app, _rx) = test_app("crowded");
    open_shells(
        &mut app,
        &[
            "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf",
        ],
    );
    for w in [120u16, 90, 70, 50, 40, 32] {
        let _ = render(&mut app, w, 20);
        assert_eq!(
            pinned_tabs(&app).len(),
            2,
            "width {w}: the way out must survive"
        );
        // Shell tabs never overlap Hosts or Chat, nor run off the screen.
        for (srect, _) in &pinned_tabs(&app) {
            for (trect, i) in &shell_tabs(&app) {
                assert!(trect.x + trect.width <= w, "width {w}: tab {i} {trect:?}");
                assert!(
                    trect.x + trect.width <= srect.x || trect.x >= srect.x + srect.width,
                    "width {w}: shell tab {i} {trect:?} overlaps a screen tab {srect:?}"
                );
            }
        }
    }
    app.term.shutdown();
}

/// The active tab is always rendered, however many are open.
#[test]
fn the_tab_window_follows_the_active_tab() {
    let (mut app, _rx) = test_app("tabwindow");
    open_shells(
        &mut app,
        &["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"],
    );
    for i in 0..6 {
        app.term.select_tab(i);
        let _ = render(&mut app, 60, 20);
        assert!(
            shell_tabs(&app).iter().any(|(_, t)| *t == i),
            "tab {i} is active but was not rendered"
        );
    }
    // With tabs hidden, the strip says so.
    app.term.select_tab(0);
    let out = render(&mut app, 60, 20);
    assert!(out.contains('›'), "hidden tabs are marked: {out}");
    app.term.shutdown();
}

/// The screen tabs are the only mouse route off a focused pane, so they must
/// survive a narrow terminal even when their labels cannot.
#[test]
fn the_screen_tabs_shed_labels_rather_than_disappear() {
    let (mut app, _rx) = test_app("narrowtabs");
    open_shells(&mut app, &["a-rather-long-host-name"]);
    for w in [120u16, 80, 60, 44, 36, 30] {
        let _ = render(&mut app, w, 20);
        assert_eq!(
            pinned_tabs(&app).len(),
            2,
            "width {w}: every screen must stay clickable"
        );
        assert!(
            shell_tabs(&app).len() == 1,
            "width {w}: the showing shell keeps its tab"
        );
        for (rect, _) in &app.regions.screen_tabs {
            assert!(
                rect.x + rect.width <= w,
                "width {w}: tab hitbox {rect:?} runs off the screen"
            );
        }
    }
    app.term.shutdown();
}

/// Even a terminal too small for the full chrome keeps the header: it is the
/// only clickable way between screens, and on Shells the only way out.
#[test]
fn a_tiny_terminal_keeps_the_screen_tabs() {
    let (mut app, _rx) = test_app("tinytabs");
    open_shells(&mut app, &["alpha"]);
    for (w, h) in [(29u16, 12u16), (40, 5), (25, 3)] {
        let _ = render(&mut app, w, h);
        assert_eq!(
            pinned_tabs(&app).len(),
            2,
            "{w}x{h}: the escape route must survive"
        );
    }
    // Below three rows there is genuinely no room, and that must not panic.
    let _ = render(&mut app, 20, 2);
    let _ = render(&mut app, 1, 1);
    app.term.shutdown();
}

/// The small-terminal path shows the screen you are on, not always Shells.
#[test]
fn a_tiny_terminal_shows_the_active_screen() {
    let (mut app, _rx) = test_app("tinyscreen");
    app.screen = Screen::Hosts;
    let out = render(&mut app, 28, 12);
    assert!(
        out.contains("Known Hosts"),
        "Hosts must render its own body: {out}"
    );
    assert!(!out.contains("No open shells"), "{out}");
}

/// Absurd sizes must not panic — the classic ratatui crash.
#[test]
fn tiny_and_narrow_terminals_do_not_panic() {
    let (mut app, _rx) = test_app("tiny");
    for (w, h) in [(1u16, 1u16), (4, 3), (20, 6), (31, 9), (200, 60)] {
        let _ = render(&mut app, w, h);
    }
    app.mode = Mode::Help;
    let _ = render(&mut app, 10, 4);
    app.open_add();
    let _ = render(&mut app, 12, 5);
    app.screen = Screen::Chat;
    app.mode = Mode::Normal;
    let _ = render(&mut app, 8, 4);
}

#[test]
fn an_empty_host_list_still_renders() {
    let (mut app, _rx) = test_app("empty");
    for id in app.hosts.iter().map(|h| h.id).collect::<Vec<_>>() {
        app.db.remove(id).unwrap();
    }
    app.reload();
    let out = render(&mut app, 100, 24);
    assert!(out.contains("0 hosts"), "{out}");
    // Actions on an empty list are no-ops, not panics.
    key(&mut app, KeyCode::Down);
    app.function_key(4);
    app.function_key(8);
    app.function_key(9);
    for c in ['e', 'm', 'u'] {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
}

// ---- mouse and hover -----------------------------------------------------

#[test]
fn clicking_a_row_moves_the_cursor() {
    let (mut app, _rx) = test_app("clickrow");
    let _ = render(&mut app, 120, 30); // populate regions
    let rows = app.regions.rows;
    assert!(rows.height >= 2, "expected drawn rows");
    click(&mut app, rows.x + 5, rows.y + 1);
    assert_eq!(app.cursor, 1);
}

#[test]
fn double_clicking_a_row_opens_a_shell() {
    let (mut app, _rx) = test_app("dblclick");
    let _ = render(&mut app, 120, 30);
    let rows = app.regions.rows;
    // Two clicks on the same row inside the double-click window.
    click(&mut app, rows.x + 5, rows.y);
    assert_eq!(app.screen, Screen::Hosts, "one click only moves the cursor");
    click(&mut app, rows.x + 5, rows.y);

    // Whether the ssh helper is reachable depends on how the tests were
    // invoked, so assert that open_shell ran — either it opened the tab and
    // switched screens, or it reported why it could not.
    let opened = app.screen == Screen::Shells && app.term.tab_count() == 1;
    let refused = app
        .alert
        .as_deref()
        .is_some_and(|e| e.contains("Could not open a shell"));
    assert!(
        opened || refused,
        "double-click did not reach open_shell; error was {:?}",
        app.alert
    );
    app.quit();
}

#[test]
fn clicking_the_function_bar_runs_that_action() {
    let (mut app, _rx) = test_app("fnbar");
    let _ = render(&mut app, 120, 30);
    // F2 = Add: find its registered cap and click it.
    let (x0, _, _) = *app.regions.fkeys.iter().find(|(_, _, n)| *n == 2).unwrap();
    let y = app.regions.fn_bar_y;
    click(&mut app, x0 + 1, y);
    assert_eq!(app.mode, Mode::HostForm);
}

#[test]
fn clicking_a_header_tab_switches_screens() {
    let (mut app, _rx) = test_app("tabs");
    let _ = render(&mut app, 120, 30);
    let (rect, screen) = *app
        .regions
        .screen_tabs
        .iter()
        .find(|(_, s)| *s == Tab::Chat)
        .unwrap();
    assert_eq!(screen, Tab::Chat);
    click(&mut app, rect.x + 1, rect.y);
    assert_eq!(app.screen, Screen::Chat);
}

#[test]
fn hovering_a_row_produces_the_status_hint() {
    let (mut app, _rx) = test_app("hover");
    let _ = render(&mut app, 120, 30);
    let rows = app.regions.rows;
    assert!(
        app.hover_hint().is_none(),
        "no hint before the pointer moves"
    );

    moved(&mut app, rows.x + 3, rows.y);
    assert_eq!(app.hovered_row(), Some(0));
    let hint = app.hover_hint().expect("expected a hover hint");
    assert!(hint.contains("deploy@10.0.4.11:22"), "got {hint}");
    assert!(hint.contains("not mounted"));
    assert!(hint.contains("double-click for a shell"));

    // The hint reaches the status bar.
    let out = render(&mut app, 120, 30);
    assert!(out.contains("deploy@10.0.4.11:22"), "{out}");

    // Off the table, the hint goes away.
    moved(&mut app, 0, 0);
    assert!(app.hover_hint().is_none());
    assert!(app.hovered_row().is_none());
}

#[test]
fn scrolling_moves_the_cursor() {
    let (mut app, _rx) = test_app("scroll");
    let _ = render(&mut app, 120, 30);
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 5,
        row: app.regions.rows.y,
        modifiers: KeyModifiers::empty(),
    });
    assert_eq!(app.cursor, 2, "three rows down, clamped to the last host");
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 5,
        row: app.regions.rows.y,
        modifiers: KeyModifiers::empty(),
    });
    assert_eq!(app.cursor, 0);
}

/// Tab to a form button: `n` presses from the Name field.
fn tab_to(app: &mut App, n: usize) {
    for _ in 0..n {
        key(app, KeyCode::Tab);
    }
}

/// Every button in the Add dialog can be reached with Tab, in the order they
/// are drawn, and the one with focus is reversed — as the pointer over it
/// would — with the bar naming what ↵ will do.
#[test]
fn tab_reaches_every_button_of_the_host_form() {
    use crate::app::form::FormField;
    use ratatui::style::Modifier;
    let (mut app, _rx) = test_app("formtab");
    app.open_add();
    tab_to(&mut app, 7);
    let mut seen = Vec::new();
    for _ in 0..4 {
        seen.push(app.form.as_ref().unwrap().focus);
        key(&mut app, KeyCode::Tab);
    }
    assert_eq!(
        seen,
        [
            FormField::KeyButton,
            FormField::BulkAdd,
            FormField::Save,
            FormField::Cancel
        ]
    );
    assert_eq!(
        app.form.as_ref().unwrap().focus,
        FormField::Name,
        "and round"
    );

    // Cancel, by Shift+Tab from the first field.
    key(&mut app, KeyCode::BackTab);
    let buf = render_buf(&mut app, 120, 34);
    let reversed = |c: Click| {
        let r = button(&app, c);
        buf[(r.x + 1, r.y)].modifier.contains(Modifier::REVERSED)
    };
    assert!(
        reversed(Click::FormButton(FormField::Cancel)),
        "focus shows"
    );
    assert!(!reversed(Click::FormButton(FormField::Save)));
    let out = render(&mut app, 120, 34);
    assert!(
        out.lines().last().unwrap().contains("↵  Cancel"),
        "the bar says what ↵ does now: {out}"
    );
    assert!(out.contains("Enter or Space press"), "{out}");
}

/// ↵ or Space presses the focused button, each doing what its own key does.
#[test]
fn enter_and_space_press_the_focused_button() {
    let (mut app, _rx) = test_app("formpress");

    app.open_add();
    key(&mut app, KeyCode::BackTab); // Cancel
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal, "Cancel");
    assert!(app.form.is_none());

    app.open_add();
    tab_to(&mut app, 8); // Bulk add
    key(&mut app, KeyCode::Char(' '));
    assert_eq!(app.mode, Mode::BulkImport, "Bulk add, by Space");
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc);

    // Save, in Edit, where it is two back from the first field.
    app.set_cursor(1);
    let before = app.hosts[1].name.clone();
    app.open_edit();
    key(&mut app, KeyCode::BackTab);
    key(&mut app, KeyCode::BackTab);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal, "Save");
    assert_eq!(app.hosts[1].name, before);

    // The key button, from the same form.
    app.open_edit();
    tab_to(&mut app, 7);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::ShowKey, "the key button");
}

/// A click presses the button it lands on, whatever has focus. The Add Host
/// button used to be sent as ↵ — which now presses the focused button, so
/// with focus on Cancel, clicking Add Host would have cancelled.
#[test]
fn a_click_presses_its_own_button_whatever_has_focus() {
    use crate::app::form::FormField;
    let (mut app, _rx) = test_app("formclick");
    let before = app.hosts.len();
    app.open_add();
    for c in "bastion".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    tab_to(&mut app, 2); // Address
    for c in "10.9.9.9".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::BackTab);
    key(&mut app, KeyCode::BackTab);
    key(&mut app, KeyCode::BackTab); // round to Cancel
    assert_eq!(app.form.as_ref().unwrap().focus, FormField::Cancel);

    let _ = render(&mut app, 120, 34);
    let add = button(&app, Click::FormButton(FormField::Save));
    click(&mut app, add.x + 1, add.y);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.hosts.len(), before + 1, "it added, it did not cancel");
}

/// A button takes no text: typing or pasting on one leaves every field alone.
#[test]
fn typing_on_a_focused_button_types_nothing() {
    use crate::app::form::FormField;
    let (mut app, _rx) = test_app("formbuttontype");
    app.open_add();
    key(&mut app, KeyCode::BackTab); // Cancel
    key(&mut app, KeyCode::Char('x'));
    app.on_paste("yz");
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.focus, FormField::Cancel);
    assert_eq!(form.name.value(), "");
    assert_eq!(app.mode, Mode::HostForm);
}

#[test]
fn f7_in_the_edit_form_generates_a_key_and_comes_back_to_the_form() {
    let (mut app, _rx) = test_app("formkey");
    app.set_cursor(1); // db-main, which has no key
    assert!(app.hosts[1].key_name.is_empty());
    key(&mut app, KeyCode::Char('e'));
    assert_eq!(app.mode, Mode::HostForm);

    // An unsaved edit, to prove it survives the trip through the key dialog.
    key(&mut app, KeyCode::End);
    for c in "-2".chars() {
        key(&mut app, KeyCode::Char(c));
    }

    key(&mut app, KeyCode::F(7));
    assert_eq!(app.mode, Mode::ShowKey);
    let (host, public) = app.key_dialog.clone().unwrap();
    assert_eq!(host, "db-main-2", "made against the name in the form");
    assert!(public.starts_with("ssh-ed25519 "));
    let out = render(&mut app, 120, 34);
    assert!(out.contains("authorized_keys"), "{out}");

    // Closing the key returns to the form, with the edit still in it. It used
    // to close to the host list, hiding the form and everything typed into it.
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::HostForm, "back in the form");
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.name.value(), "db-main-2", "the edit survived");
    assert!(form.has_key(), "and the form carries the new key to Save");

    // Save is what persists it.
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    let saved = app.hosts.iter().find(|h| h.name == "db-main-2").unwrap();
    assert!(!saved.key_name.is_empty(), "Save persisted the key");
}

/// A host that already has a key is *shown* it. The button used to generate
/// against the typed name regardless, so renaming a host in the form and
/// clicking "Show public key" silently made a new key — installed nowhere —
/// and Save pointed the host at it.
#[test]
fn showing_a_renamed_hosts_key_does_not_make_a_new_one() {
    let (mut app, _rx) = test_app("renamekey");
    app.set_cursor(1);
    key(&mut app, KeyCode::Char('e'));
    key(&mut app, KeyCode::F(7));
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Enter); // save db-main with its key
    let original = app.hosts[1].key_name.clone();
    assert!(!original.is_empty());
    let (_, original_public) = {
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, KeyCode::F(7));
        let k = app.key_dialog.clone().unwrap();
        key(&mut app, KeyCode::Esc);
        key(&mut app, KeyCode::Esc);
        k
    };

    // Rename, then ask for the key.
    key(&mut app, KeyCode::Char('e'));
    key(&mut app, KeyCode::End);
    for c in "-renamed".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::F(7));
    let (_, shown) = app.key_dialog.clone().unwrap();
    assert_eq!(shown, original_public, "the host's own key, not a new one");
    key(&mut app, KeyCode::Esc);
    assert_eq!(
        app.form.as_ref().unwrap().key_name,
        original,
        "still pointing at the key that is installed"
    );
    let stray = app.datadir.join("keys").join("db-main-renamed");
    assert!(!stray.exists(), "no second key was generated");
}

/// The button in the form's key row goes the same way as F7.
#[test]
fn the_forms_key_button_is_clickable() {
    let (mut app, _rx) = test_app("formkeyclick");
    app.set_cursor(1);
    app.open_edit();
    let _ = render(&mut app, 120, 34);
    let (rect, _) = *app
        .regions
        .clicks
        .iter()
        .find(|(_, c)| *c == Click::FormButton(crate::app::form::FormField::KeyButton))
        .expect("a key button in the form");
    click(&mut app, rect.x + 1, rect.y);
    assert_eq!(app.mode, Mode::ShowKey);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::HostForm);
}

#[test]
fn dialog_buttons_are_clickable() {
    let (mut app, _rx) = test_app("btn");
    app.open_add();
    let _ = render(&mut app, 120, 34);
    let (rect, _) = *app
        .regions
        .clicks
        .iter()
        .find(|(_, c)| *c == Click::FormButton(crate::app::form::FormField::Cancel))
        .expect("a Cancel button");
    click(&mut app, rect.x + 1, rect.y);
    assert_eq!(app.mode, Mode::Normal, "Cancel closes the form");
    assert!(app.form.is_none());
}

#[test]
fn clicking_a_form_field_focuses_it() {
    use crate::app::form::FormField;
    let (mut app, _rx) = test_app("focus");
    app.open_add();
    let _ = render(&mut app, 120, 34);
    let (rect, _) = *app
        .regions
        .clicks
        .iter()
        .find(|(_, c)| *c == Click::FocusField(FormField::Login))
        .expect("a Login field row");
    click(&mut app, rect.x + 2, rect.y);
    assert_eq!(app.form.as_ref().unwrap().focus, FormField::Login);
}

// ---- CRUD through the UI -------------------------------------------------

#[test]
fn adding_a_host_through_the_form_persists_it() {
    let (mut app, _rx) = test_app("add");
    let before = app.hosts.len();
    app.function_key(2); // F2 Add
    assert_eq!(app.mode, Mode::HostForm);
    for c in "bastion".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Tab); // Type
    key(&mut app, KeyCode::Tab); // Address
    for c in "edge.corp.net".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);

    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.hosts.len(), before + 1);
    let added = app.hosts.iter().find(|h| h.name == "bastion").unwrap();
    assert_eq!(added.addr, "edge.corp.net");
    assert_eq!(
        added.mount_point, "/net/bastion",
        "the mount point followed the name"
    );
    assert_eq!(
        app.cursor,
        app.hosts.iter().position(|h| h.name == "bastion").unwrap()
    );
}

#[test]
fn an_invalid_form_keeps_the_dialog_open_and_explains_why() {
    let (mut app, _rx) = test_app("invalid");
    app.open_add();
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::HostForm, "the dialog stays open");
    let err = app.alert.clone().expect("an error dialog");
    assert!(err.contains("Host name is required"), "{err}");
    // Dismissing it returns to the form, with nothing lost: the error sits over
    // the mode rather than replacing it.
    key(&mut app, KeyCode::Esc);
    assert!(app.alert.is_none());
    assert_eq!(app.mode, Mode::HostForm, "back in the form");
    assert!(app.form.is_some());
}

#[test]
fn marking_hosts_then_deleting_removes_all_of_them() {
    let (mut app, _rx) = test_app("delete");
    key(&mut app, KeyCode::Insert); // mark web-01, cursor moves down
    key(&mut app, KeyCode::Insert); // mark db-main
    assert_eq!(app.marked.len(), 2);
    assert_eq!(app.targets().len(), 2);

    app.function_key(8); // F8 Delete
    assert_eq!(app.mode, Mode::ConfirmDelete);
    let out = render(&mut app, 120, 34);
    assert!(out.contains("Delete 2 hosts"), "{out}");

    key(&mut app, KeyCode::Char('y'));
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.hosts.len(), 1);
    assert_eq!(app.hosts[0].name, "nas");
    assert!(app.marked.is_empty());
}

#[test]
fn editing_a_host_round_trips_through_the_database() {
    let (mut app, _rx) = test_app("edit");
    app.function_key(4); // F4 Edit on web-01
    assert_eq!(app.mode, Mode::HostForm);
    // Move to Port and retype it.
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Backspace);
    key(&mut app, KeyCode::Backspace);
    for c in "2222".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);

    assert_eq!(app.mode, Mode::Normal);
    let h = app.hosts.iter().find(|h| h.name == "web-01").unwrap();
    assert_eq!(h.port, 2222);
    assert_eq!(h.pass, "hunter2", "untouched fields survive the edit");
}

#[test]
fn marks_are_dropped_when_their_host_disappears() {
    let (mut app, _rx) = test_app("staleMarks");
    key(&mut app, KeyCode::Insert);
    let gone = *app.marked.iter().next().unwrap();
    app.db.remove(gone).unwrap();
    app.reload();
    assert!(app.marked.is_empty(), "a mark must not outlive its host");
}

// ---- key routing ---------------------------------------------------------

fn alt(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::ALT));
}

/// One strip, one way to count it: Alt+1 Hosts, Alt+2 Chat, Alt+3… the shells
/// in the order they were opened.
#[test]
fn alt_digits_reach_every_tab() {
    let (mut app, _rx) = test_app("altdigit");
    open_shells(&mut app, &["alpha", "bravo"]);
    app.set_screen(Screen::Hosts);

    alt(&mut app, KeyCode::Char('2'));
    assert_eq!(app.screen, Screen::Chat);
    alt(&mut app, KeyCode::Char('4'));
    assert_eq!(app.screen, Screen::Shells);
    assert_eq!(app.term.active, Some(1), "bravo");
    alt(&mut app, KeyCode::Char('3'));
    assert_eq!(app.term.active, Some(0), "alpha");
    alt(&mut app, KeyCode::Char('1'));
    assert_eq!(app.screen, Screen::Hosts);

    // Past the last tab nothing happens.
    alt(&mut app, KeyCode::Char('5'));
    assert_eq!(app.screen, Screen::Hosts);
    app.term.shutdown();
}

/// Alt+←/→ walk the whole strip, shells included, wrapping both ways.
#[test]
fn alt_arrows_walk_hosts_chat_and_shells() {
    let (mut app, _rx) = test_app("altarrows");
    open_shells(&mut app, &["alpha", "bravo"]);
    app.set_screen(Screen::Hosts);

    let mut walk = |code| {
        alt(&mut app, code);
        app.current_tab()
    };
    assert_eq!(walk(KeyCode::Right), 1, "Chat");
    assert_eq!(walk(KeyCode::Right), 2, "alpha");
    assert_eq!(walk(KeyCode::Right), 3, "bravo");
    assert_eq!(walk(KeyCode::Right), 0, "wraps forward to Hosts");
    assert_eq!(walk(KeyCode::Left), 3, "wraps backward to bravo");
    assert_eq!(walk(KeyCode::Left), 2);
    assert_eq!(app.screen, Screen::Shells);
    assert_eq!(app.term.active, Some(0));
    app.term.shutdown();
}

/// The tab keys are the exception to the Shells screen's keyboard
/// transparency: they get you out of a live terminal, and between shells,
/// without a mouse.
#[test]
fn the_tab_keys_escape_a_focused_pane() {
    let (mut app, _rx) = test_app("altescape");
    open_shells(&mut app, &["alpha", "bravo"]);
    assert!(app.term.focused_session().is_some());

    alt(&mut app, KeyCode::Left);
    assert_eq!(app.screen, Screen::Shells, "Alt+← from bravo is alpha");
    assert_eq!(app.term.active, Some(0));
    alt(&mut app, KeyCode::Left);
    assert_eq!(app.screen, Screen::Chat, "and then Chat");

    alt(&mut app, KeyCode::Char('4'));
    alt(&mut app, KeyCode::Right);
    assert_eq!(app.screen, Screen::Hosts, "Alt+→ from the last wraps");

    alt(&mut app, KeyCode::Char('4'));
    alt(&mut app, KeyCode::Char('1'));
    assert_eq!(app.screen, Screen::Hosts, "Alt+digit leaves a focused pane");

    // Everything else still belongs to the terminal.
    alt(&mut app, KeyCode::Char('3'));
    for code in [KeyCode::Up, KeyCode::Down, KeyCode::Tab, KeyCode::Esc] {
        alt(&mut app, code);
        assert_eq!(app.screen, Screen::Shells, "{code:?} must reach the pty");
        assert_eq!(app.term.active, Some(0));
    }
    for n in 1..=10u8 {
        app.on_key(KeyEvent::new(KeyCode::F(n), KeyModifiers::empty()));
        assert_eq!(app.screen, Screen::Shells);
        assert!(!app.should_quit);
    }
    app.term.shutdown();
}

#[test]
fn key_releases_are_ignored() {
    let (mut app, _rx) = test_app("release");
    let mut ev = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
    ev.kind = KeyEventKind::Release;
    app.on_key(ev);
    assert_eq!(app.cursor, 0, "a release must not move the cursor");
}

/// The rule for the Shells screen: a focused pane takes every key but the tab
/// keys, Alt+1…9 and Alt+←/→. No F-key, no Esc then digit, no other Alt chord
/// is reserved by the app, because mc reads Esc+digit as its own F-key
/// emulation and Alt as its menu shortcuts.
#[test]
fn a_focused_pane_takes_every_key() {
    use crate::term::session::Spawn;
    let (mut app, _rx) = test_app("passthrough_all");
    let mut spawn = Spawn::new("/bin/sh");
    spawn.args = vec!["-c".into(), "sleep 30".into()];
    app.term
        .open_tab(
            vec![("local".into(), spawn)],
            (24, 80),
            50,
            "xterm",
            &app.term_tx,
        )
        .unwrap();
    app.screen = Screen::Shells;
    assert!(app.term.focused_session().is_some());

    // Every F-key.
    for n in 1..=10u8 {
        app.on_key(KeyEvent::new(KeyCode::F(n), KeyModifiers::empty()));
        assert!(!app.should_quit, "F{n} must not quit");
        assert_eq!(app.mode, Mode::Normal, "F{n} must not open a dialog");
        assert_eq!(app.screen, Screen::Shells, "F{n} must not change screen");
    }

    // Esc, alone and followed by every digit — mc's own F-key emulation.
    for d in '0'..='9' {
        key(&mut app, KeyCode::Esc);
        key(&mut app, KeyCode::Char(d));
        assert!(!app.should_quit, "Esc {d} must not quit");
        assert_eq!(app.mode, Mode::Normal, "Esc {d} must not open a dialog");
        assert_eq!(app.screen, Screen::Shells, "Esc {d} must not change screen");
    }

    // Alt+0, which is how a quickly typed Esc+0 — mc's F10 — arrives. It is
    // Quit on the other screens, but not here.
    app.on_key(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::ALT));
    assert!(!app.should_quit, "Alt+0 must not quit");
    assert_eq!(app.screen, Screen::Shells, "Alt+0 must not change screen");

    // Alt+letter, Tab and Ctrl chords belong to the terminal too.
    for c in ['o', 't', 'h', '?'] {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT));
        assert_eq!(app.screen, Screen::Shells);
    }
    key(&mut app, KeyCode::Tab);
    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(app.screen, Screen::Shells);
    assert_eq!(app.mode, Mode::Normal);
    app.term.shutdown();
}

/// The header tabs are the mouse route off a focused pane; with the footer
/// gone they are the only one, so this must keep working.
#[test]
fn the_header_tabs_escape_a_focused_pane() {
    use crate::term::session::Spawn;
    let (mut app, _rx) = test_app("mouseout");
    let mut spawn = Spawn::new("/bin/sh");
    spawn.args = vec!["-c".into(), "sleep 30".into()];
    app.term
        .open_tab(
            vec![("local".into(), spawn)],
            (24, 80),
            50,
            "xterm",
            &app.term_tx,
        )
        .unwrap();
    app.screen = Screen::Shells;
    let _ = render(&mut app, 120, 30);

    let (rect, _) = *app
        .regions
        .screen_tabs
        .iter()
        .find(|(_, s)| *s == Tab::Hosts)
        .expect("a Hosts tab");
    click(&mut app, rect.x + 1, rect.y);
    assert_eq!(
        app.screen,
        Screen::Hosts,
        "clicking a screen tab must still work"
    );

    // ...and the Hosts screen still has its footer.
    let out = render(&mut app, 120, 30);
    assert!(out.contains("F10") && out.contains("Quit"), "{out}");
    app.term.shutdown();
}

/// Closing the last shell must not strand the user: with nothing focused there
/// is nothing to be transparent to, so the keyboard comes back.
#[test]
fn the_keyboard_returns_when_no_pane_is_focused() {
    let (mut app, _rx) = test_app("emptyshells");
    app.screen = Screen::Shells;
    assert!(app.term.focused_session().is_none());

    key(&mut app, KeyCode::F(1));
    assert_eq!(app.mode, Mode::Help, "F1 works with no shell open");
    key(&mut app, KeyCode::Esc);

    key(&mut app, KeyCode::Esc);
    assert_eq!(
        app.screen,
        Screen::Hosts,
        "Esc leaves an empty Shells screen"
    );

    app.screen = Screen::Shells;
    key(&mut app, KeyCode::F(10));
    assert!(app.should_quit, "F10 quits with no shell open");
}

#[test]
fn f10_still_quits_from_the_hosts_screen() {
    let (mut app, _rx) = test_app("f10");
    key(&mut app, KeyCode::F(10));
    assert!(app.should_quit);
}

#[test]
fn help_toggles_and_closes_on_escape() {
    let (mut app, _rx) = test_app("helptoggle");
    key(&mut app, KeyCode::F(1));
    assert_eq!(app.mode, Mode::Help);
    key(&mut app, KeyCode::F(1));
    assert_eq!(app.mode, Mode::Normal);
    key(&mut app, KeyCode::F(1));
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn chat_composer_accepts_typing() {
    let (mut app, _rx) = test_app("compose");
    app.screen = Screen::Chat;
    for c in "restart api".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    assert_eq!(app.chat.draft_text(), "restart api");
    key(&mut app, KeyCode::Esc);
    assert!(
        app.chat.draft_text().is_empty(),
        "Esc clears the draft when idle"
    );
}

/// With no model configured, Enter must say so and keep what was typed —
/// silently swallowing the message would be the worst of both.
#[test]
fn sending_without_a_model_warns_and_keeps_the_draft() {
    use crate::app::StatusKind;
    let (mut app, _rx) = test_app("nomodel");
    app.screen = Screen::Chat;
    assert!(!app.cfg.agent.configured());
    for c in "hello".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);

    assert_eq!(app.status.kind, StatusKind::Warn);
    assert!(
        app.status.text.contains("No model configured"),
        "{}",
        app.status.text
    );
    assert_eq!(app.chat.draft_text(), "hello", "the message is not lost");
    assert!(app.chat.turns.is_empty(), "nothing was recorded");
    assert!(!app.busy);
}

#[test]
fn paste_routes_to_whatever_is_focused() {
    let (mut app, _rx) = test_app("paste");
    app.screen = Screen::Chat;
    app.on_paste("hello");
    assert_eq!(app.chat.draft_text(), "hello");

    app.screen = Screen::Hosts;
    app.open_add();
    app.on_paste("web-99\n");
    assert_eq!(
        app.form.as_ref().unwrap().name.value(),
        "web-99",
        "control chars are dropped"
    );
}

#[test]
fn regions_are_rebuilt_every_frame() {
    let (mut app, _rx) = test_app("regions");
    let _ = render(&mut app, 120, 30);
    let n = app.regions.fkeys.len();
    assert!(n > 0);
    let _ = render(&mut app, 120, 30);
    assert_eq!(
        app.regions.fkeys.len(),
        n,
        "regions must not accumulate across frames"
    );

    // Switching screens replaces the hitboxes rather than adding to them: the
    // Hosts screen's Shell cap must not survive onto the Chat bar.
    app.screen = Screen::Chat;
    let _ = render(&mut app, 120, 30);
    assert!(
        !app.regions
            .fkeys
            .iter()
            .any(|(_, _, k)| *k == crate::app::BAR_SHELL),
        "stale host hitboxes must be gone"
    );
}

#[test]
fn the_cursor_row_stays_visible_when_the_list_scrolls() {
    let (mut app, _rx) = test_app("scrollwin");
    for i in 0..40 {
        app.db
            .save(&HostRecord {
                name: format!("h{i:02}"),
                proto: "ssh".into(),
                addr: format!("10.0.0.{i}"),
                port: 22,
                ..Default::default()
            })
            .unwrap();
    }
    app.reload();
    app.cursor = app.hosts.len() - 1;
    let out = render(&mut app, 120, 20);
    let last = &app.hosts[app.cursor].name;
    assert!(
        out.contains(last.as_str()),
        "the cursor row must be on screen:\n{out}"
    );
    assert!(app.regions.row_start > 0, "the window scrolled");
}

/// The right border beside the host rows, from the cell by the column header
/// down to the bottom-right corner.
fn hosts_right_edge(app: &mut App, w: u16, h: u16) -> Vec<String> {
    let buf = render_buf(app, w, h);
    let rows = app.regions.rows;
    let x = rows.right();
    let mut edge = vec![buf[(x, rows.y - 1)].symbol().to_string()];
    let mut y = rows.y;
    loop {
        let cell = buf[(x, y)].symbol().to_string();
        let corner = cell == "┘";
        edge.push(cell);
        if corner {
            return edge;
        }
        y += 1;
    }
}

/// Where in the list the window is, on the frame's right edge: flush at the
/// top at the start and at the bottom at the end — and only beside the rows,
/// so the border by the column header and the corner below stay the frame's.
/// The thumb is a line, never a filled block: it sits beside the cursor row,
/// and a block there merged with the row's orange bar into one shape.
#[test]
fn the_hosts_scrollbar_shows_where_in_the_list_you_are() {
    let (mut app, _rx) = test_app("hostbar");
    for i in 0..40 {
        app.db
            .save(&HostRecord {
                name: format!("h{i:02}"),
                proto: "ssh".into(),
                addr: format!("10.0.0.{i}"),
                port: 22,
                ..Default::default()
            })
            .unwrap();
    }
    app.reload();

    let top = hosts_right_edge(&mut app, 120, 20);
    let n = top.len();
    assert_eq!(top[0], "│", "beside the header, the frame: {top:?}");
    assert_eq!(
        top[1], "┃",
        "at the start, the thumb is at the top: {top:?}"
    );
    assert!(!top.iter().any(|c| c == "█"), "no filled cells: {top:?}");
    assert_eq!(top[n - 2], "│", "{top:?}");
    assert_eq!(top[n - 1], "┘", "the corner is the frame's: {top:?}");

    key(&mut app, KeyCode::End);
    let end = hosts_right_edge(&mut app, 120, 20);
    assert_eq!(end[0], "│", "{end:?}");
    assert_eq!(end[1], "│", "{end:?}");
    assert_eq!(
        end[n - 2],
        "┃",
        "at the end, the thumb is at the bottom: {end:?}"
    );
    assert_eq!(end[n - 1], "┘", "{end:?}");
}

/// A list that fits has nowhere to scroll, and keeps its plain border.
#[test]
fn a_host_list_that_fits_keeps_a_plain_border() {
    let (mut app, _rx) = test_app("hostnobar");
    let edge = hosts_right_edge(&mut app, 120, 30);
    let (last, sides) = edge.split_last().unwrap();
    assert_eq!(last, "┘");
    assert!(sides.iter().all(|c| c == "│"), "{edge:?}");
    assert!(sides.len() > 4, "the border runs past the three rows");
}

fn press(app: &mut App, x: u16, y: u16) {
    click(app, x, y);
}

fn drag_to(app: &mut App, x: u16, y: u16) {
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::empty(),
    });
}

fn release(app: &mut App, x: u16, y: u16) {
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::empty(),
    });
}

/// The scrollbar drawn this frame for `target`.
fn scrollbar_band(app: &App, target: crate::app::ScrollTarget) -> ratatui::layout::Rect {
    app.regions
        .scrollbars
        .iter()
        .find(|(_, t)| *t == target)
        .map(|(r, _)| *r)
        .unwrap_or_else(|| panic!("no scrollbar for {target:?}"))
}

/// The thumb's first row within its band, as drawn.
fn drawn_thumb(app: &mut App, band: ratatui::layout::Rect, w: u16, h: u16) -> u16 {
    let buf = render_buf(app, w, h);
    (0..band.height)
        .find(|&r| buf[(band.x, band.y + r)].symbol() == "┃")
        .expect("a thumb")
}

/// Forty more hosts than the seeded three: 43, in a 120×20 window that shows
/// 14 of them.
fn many_hosts(tag: &str) -> App {
    let (mut app, _rx) = test_app(tag);
    for i in 0..40 {
        app.db
            .save(&HostRecord {
                name: format!("h{i:02}"),
                proto: "ssh".into(),
                addr: format!("10.0.0.{i}"),
                port: 22,
                ..Default::default()
            })
            .unwrap();
    }
    app.reload();
    app
}

/// A click on the track goes there: the bottom of the bar is the end of the
/// list, the top its start.
#[test]
fn clicking_the_hosts_scrollbar_jumps_there() {
    use crate::app::ScrollTarget;
    let mut app = many_hosts("hostbar-click");
    let _ = render(&mut app, 120, 20);
    let band = scrollbar_band(&app, ScrollTarget::Hosts);

    press(&mut app, band.x, band.bottom() - 1);
    release(&mut app, band.x, band.bottom() - 1);
    let _ = render(&mut app, 120, 20);
    assert_eq!(app.regions.row_start, 43 - 14, "the end of the list");

    press(&mut app, band.x, band.y);
    release(&mut app, band.x, band.y);
    let _ = render(&mut app, 120, 20);
    assert_eq!(app.regions.row_start, 0, "and back to its start");
}

/// The thumb moves with the pointer, held where it was taken, and the list
/// with it. Taking hold moves nothing; letting go ends it.
#[test]
fn dragging_the_hosts_thumb_scrolls_the_list_with_it() {
    use crate::app::ScrollTarget;
    let mut app = many_hosts("hostbar-drag");
    let _ = render(&mut app, 120, 20);
    let band = scrollbar_band(&app, ScrollTarget::Hosts);
    assert_eq!(drawn_thumb(&mut app, band, 120, 20), 0);

    // Take it by its second row.
    press(&mut app, band.x, band.y + 1);
    let _ = render(&mut app, 120, 20);
    assert_eq!(
        app.regions.row_start, 0,
        "a press on the thumb moves nothing"
    );

    drag_to(&mut app, band.x, band.y + 5);
    assert_eq!(
        drawn_thumb(&mut app, band, 120, 20),
        4,
        "the thumb followed, the pointer still on its second row"
    );
    let moved = app.regions.row_start;
    assert!(moved > 0, "and the list scrolled");

    release(&mut app, band.x, band.y + 5);
    drag_to(&mut app, band.x, band.y + 9);
    let _ = render(&mut app, 120, 20);
    assert_eq!(app.regions.row_start, moved, "let go, it stays put");
}

#[test]
fn resizing_between_frames_keeps_the_table_intact() {
    let (mut app, _rx) = test_app("resize");
    for (w, h) in [(120u16, 30u16), (60, 12), (200, 50), (40, 10)] {
        let out = render(&mut app, w, h);
        assert!(out.contains("Known Hosts") || w < 30, "width {w}:\n{out}");
        // Every rendered line is exactly the terminal width — no ragged rows.
        for line in out.lines() {
            assert_eq!(line.chars().count(), w as usize);
        }
    }
}

#[test]
fn a_cursor_row_and_a_marked_row_are_distinguishable() {
    let (mut app, _rx) = test_app("marks");
    let _ = render(&mut app, 120, 30);
    key(&mut app, KeyCode::Insert); // mark row 0, cursor moves to row 1
    assert_eq!(app.cursor, 1);
    let out = render(&mut app, 120, 30);
    assert!(
        out.contains("●"),
        "a marked row carries the dot glyph:\n{out}"
    );
    assert!(out.contains("▸"), "the cursor row carries the arrow glyph");
}

#[test]
fn invert_and_select_all_marks() {
    let (mut app, _rx) = test_app("invert");
    key(&mut app, KeyCode::Char('*'));
    assert_eq!(
        app.marked.len(),
        3,
        "* marks everything when nothing is marked"
    );
    key(&mut app, KeyCode::Char('*'));
    assert!(app.marked.is_empty(), "* inverts back to nothing");
    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(app.marked.len(), 3);
    key(&mut app, KeyCode::Esc);
    assert!(app.marked.is_empty(), "Esc clears marks");
}

/// A child that turns on mouse reporting must receive the events; one that has
/// not asked leaves them to OpenAdmin's own UI. Uses a local shell rather than
/// ssh so the test is hermetic.
#[test]
fn mouse_events_reach_a_child_that_enabled_reporting() {
    use crate::term::session::Spawn;
    use std::time::{Duration, Instant};

    let (mut app, rx) = test_app("mouseforward");
    let mut spawn = Spawn::new("/bin/sh");
    // Enable SGR mouse reporting, then echo whatever arrives.
    spawn.args = vec![
        "-c".into(),
        "printf '\\033[?1000h\\033[?1006h'; cat -v".into(),
    ];
    app.term
        .open_tab(
            vec![("local".into(), spawn)],
            (24, 80),
            100,
            "xterm-256color",
            &app.term_tx,
        )
        .unwrap();
    app.screen = Screen::Shells;
    let _ = render(&mut app, 100, 30);

    // Wait until the emulator has seen the mode-set sequence.
    let id = app.term.focused_session().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut enabled = false;
    while Instant::now() < deadline && !enabled {
        enabled = app
            .term
            .session(id)
            .and_then(|s| {
                s.parser()
                    .lock()
                    .ok()
                    .map(|p| p.screen().mouse_protocol_mode() != vt100::MouseProtocolMode::None)
            })
            .unwrap_or(false);
        if !enabled {
            let _ = rx.recv_timeout(Duration::from_millis(100));
        }
    }
    assert!(enabled, "the child never enabled mouse reporting");

    // Click inside the pane, below its title row.
    let (rect, _) = *app.regions.panes.first().expect("a rendered pane");
    let (x, y) = (rect.x + 4, rect.y + 3);
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::empty(),
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = String::new();
    while Instant::now() < deadline {
        seen = app
            .term
            .session(id)
            .and_then(|s| s.parser().lock().ok().map(|p| p.screen().contents()))
            .unwrap_or_default();
        if seen.contains("[<0;") {
            break;
        }
        let _ = rx.recv_timeout(Duration::from_millis(100));
    }
    // `cat -v` renders ESC as ^[ , so the forwarded press shows up verbatim.
    assert!(
        seen.contains("^[[<0;"),
        "no SGR press reached the child; saw:\n{seen}"
    );
    app.term.shutdown();
}

// ---- scrolling a shell back -----------------------------------------------

use crate::term::session::SessionId;

fn wheel_at(app: &mut App, x: u16, y: u16, down: bool) {
    app.on_mouse(MouseEvent {
        kind: if down {
            MouseEventKind::ScrollDown
        } else {
            MouseEventKind::ScrollUp
        },
        column: x,
        row: y,
        modifiers: KeyModifiers::empty(),
    });
}

/// One tab running `script` in a pane per name, with room for history, drawn
/// once at 100×30 so every parser already has its final size.
fn shells_tab(app: &mut App, names: &[&str], script: &str) -> Vec<SessionId> {
    use crate::term::session::Spawn;
    let entries = names
        .iter()
        .map(|n| {
            let mut spawn = Spawn::new("/bin/sh");
            spawn.args = vec!["-c".into(), script.into()];
            ((*n).to_string(), spawn)
        })
        .collect();
    app.term
        .open_tab(entries, (24, 80), 1000, "xterm", &app.term_tx)
        .unwrap();
    app.screen = Screen::Shells;
    let _ = render(app, 100, 30);
    app.term.active_tab().unwrap().panes.clone()
}

/// Straight into the emulator, as if the program had printed it — so a test
/// knows exactly what is in the history, with no child to race.
fn feed(app: &App, id: SessionId, bytes: &[u8]) {
    let s = app.term.session(id).unwrap();
    s.parser().lock().unwrap().process(bytes);
}

/// `line 001` to `line {n}`. In a 29-row pane that puts `n - 28` of them in the
/// history: 100 lines leave `line 001`..`line 072` scrolled off.
fn feed_lines(app: &App, id: SessionId, n: usize) {
    let text: String = (1..=n).map(|i| format!("line {i:03}\r\n")).collect();
    feed(app, id, text.as_bytes());
}

fn offset(app: &App, id: SessionId) -> usize {
    let s = app.term.session(id).unwrap();
    s.parser().lock().unwrap().screen().scrollback()
}

fn top_line(app: &App, id: SessionId) -> String {
    let s = app.term.session(id).unwrap();
    let contents = s.parser().lock().unwrap().screen().contents();
    contents.lines().next().unwrap_or("").to_string()
}

/// A point on the pane's terminal area, clear of its title and scrollbar.
fn inside(app: &App, pane: usize) -> (u16, u16) {
    let (rect, _) = app.regions.panes[pane];
    let term =
        crate::term::manager::TerminalManager::pane_areas(rect, app.term.pane_chrome_rows()).term;
    (term.x + 4, term.y + 3)
}

/// Wait for a child's output to reach the emulator: it arrives through a PTY
/// and a reader thread, never synchronously.
fn wait_for_screen(
    app: &App,
    rx: &Receiver<TermEvent>,
    id: SessionId,
    done: impl Fn(&vt100::Screen) -> bool,
) -> bool {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let ready = app
            .term
            .session(id)
            .and_then(|s| s.parser().lock().ok().map(|p| done(p.screen())))
            .unwrap_or(false);
        if ready {
            return true;
        }
        let _ = rx.recv_timeout(Duration::from_millis(100));
    }
    false
}

/// A child that turns on SGR mouse reports and echoes what it receives.
const MOUSE_ECHO: &str = "printf '\\033[?1000h\\033[?1006h'; cat -v";

/// The complaint itself: the wheel over a shell did nothing, with its history
/// kept but unreachable.
#[test]
fn the_wheel_scrolls_back_through_a_shells_history() {
    let (mut app, _rx) = test_app("wheelback");
    let id = shells_tab(&mut app, &["local"], "sleep 30")[0];
    feed_lines(&app, id, 100);
    let (x, y) = inside(&app, 0);

    wheel_at(&mut app, x, y, false);
    assert_eq!(offset(&app, id), 3, "one notch is three lines");
    assert_eq!(top_line(&app, id), "line 070");
    let out = render(&mut app, 100, 30);
    assert!(
        out.lines().nth(1).unwrap().contains("line 070"),
        "and that is what is drawn: {out}"
    );

    wheel_at(&mut app, x, y, true);
    assert_eq!(offset(&app, id), 0, "down comes back to the live screen");
    app.term.shutdown();
}

/// The boundary the operator drew: mc, vim, less and their like run on the
/// alternate screen, and are not scrolled.
#[test]
fn the_wheel_does_not_scroll_a_full_screen_program() {
    let (mut app, _rx) = test_app("wheelalt");
    let id = shells_tab(&mut app, &["local"], "sleep 30")[0];
    feed_lines(&app, id, 100);
    feed(&app, id, b"\x1b[?1049hA FULL-SCREEN PROGRAM");
    let before = render(&mut app, 100, 30);
    let (x, y) = inside(&app, 0);

    wheel_at(&mut app, x, y, false);
    assert_eq!(render(&mut app, 100, 30), before, "nothing moved");

    // Nor was the shell underneath scrolled while it was covered.
    feed(&app, id, b"\x1b[?1049l");
    assert_eq!(offset(&app, id), 0);
    app.term.shutdown();
}

/// A program that asked for the mouse — mc does — scrolls itself: it is sent
/// the wheel, and the history stays where it is.
#[test]
fn the_wheel_goes_to_a_program_that_asked_for_the_mouse() {
    let (mut app, rx) = test_app("wheelmouse");
    let id = shells_tab(&mut app, &["local"], MOUSE_ECHO)[0];
    assert!(
        wait_for_screen(&app, &rx, id, |s| {
            s.mouse_protocol_mode() != vt100::MouseProtocolMode::None
        }),
        "the child never enabled mouse reporting"
    );
    let (x, y) = inside(&app, 0);

    wheel_at(&mut app, x, y, false);
    assert!(
        wait_for_screen(&app, &rx, id, |s| s.contents().contains("^[[<64;")),
        "no SGR wheel-up reached the child"
    );
    assert_eq!(offset(&app, id), 0);
    app.term.shutdown();
}

/// Typing into a scrolled-back shell means the prompt, as in any terminal —
/// and a paste is typing.
#[test]
fn typing_or_pasting_returns_a_shell_to_the_live_screen() {
    let (mut app, _rx) = test_app("wheeltype");
    let id = shells_tab(&mut app, &["local"], "sleep 30")[0];
    feed_lines(&app, id, 100);
    let (x, y) = inside(&app, 0);

    wheel_at(&mut app, x, y, false);
    wheel_at(&mut app, x, y, false);
    assert_eq!(offset(&app, id), 6);
    key(&mut app, KeyCode::Char('x'));
    assert_eq!(offset(&app, id), 0, "a keystroke");

    wheel_at(&mut app, x, y, false);
    assert_eq!(offset(&app, id), 3);
    app.on_paste("y");
    assert_eq!(offset(&app, id), 0, "a paste");
    app.term.shutdown();
}

/// A click sent to a program is not typing: the view stays where it is.
#[test]
fn a_forwarded_click_leaves_the_view_where_it_is() {
    let (mut app, _rx) = test_app("wheelclick");
    let id = shells_tab(&mut app, &["local"], "sleep 30")[0];
    feed_lines(&app, id, 100);
    let (x, y) = inside(&app, 0);
    wheel_at(&mut app, x, y, false);
    assert_eq!(offset(&app, id), 3);

    // Mouse reports on, still on the main screen: the click is forwarded.
    feed(&app, id, b"\x1b[?1000h");
    click(&mut app, x, y);
    assert_eq!(offset(&app, id), 3);
    app.term.shutdown();
}

/// The bar's own column, rightmost in the pane, alongside the terminal rows.
fn bar_column(app: &mut App) -> Vec<String> {
    let (rect, _) = app.regions.panes[0];
    let buf = render_buf(app, 100, 30);
    let x = rect.x + rect.width - 1;
    (rect.y..rect.y + rect.height)
        .map(|y| buf[(x, y)].symbol().to_string())
        .collect()
}

/// A scrollbar counts down from the top and vt100 counts back from the live
/// screen; the conversion is where an upside-down bar would come from. The
/// thumb is a line, never a filled block, as on the Hosts list: programs draw
/// full-width bars right up to this column.
#[test]
fn the_scrollbar_thumb_is_at_the_bottom_when_live_and_the_top_at_the_oldest_line() {
    let (mut app, _rx) = test_app("wheelbar");
    let id = shells_tab(&mut app, &["local"], "sleep 30")[0];
    feed_lines(&app, id, 100);

    let live = bar_column(&mut app);
    assert_eq!(
        live.last().unwrap(),
        "┃",
        "live: thumb at the bottom {live:?}"
    );
    assert_eq!(live.first().unwrap(), "│", "{live:?}");
    assert!(!live.iter().any(|c| c == "█"), "no filled cells: {live:?}");

    let (x, y) = inside(&app, 0);
    for _ in 0..30 {
        wheel_at(&mut app, x, y, false);
    }
    assert_eq!(offset(&app, id), 72, "at the oldest line");
    let oldest = bar_column(&mut app);
    assert_eq!(
        oldest.first().unwrap(),
        "┃",
        "oldest: thumb at the top {oldest:?}"
    );
    assert_eq!(oldest.last().unwrap(), "│", "{oldest:?}");
    app.term.shutdown();
}

/// No history, or a full-screen program: nothing to scroll, so nothing drawn.
#[test]
fn the_scrollbar_column_is_blank_with_no_history_or_on_the_alternate_screen() {
    let (mut app, _rx) = test_app("wheelblank");
    let id = shells_tab(&mut app, &["local"], "sleep 30")[0];
    let blank = |col: &[String]| col.iter().all(|c| c == " ");

    let fresh = bar_column(&mut app);
    assert!(blank(&fresh), "no history yet: {fresh:?}");

    feed_lines(&app, id, 100);
    assert!(!blank(&bar_column(&mut app)), "history: a bar");
    feed(&app, id, b"\x1b[?1049h");
    let covered = bar_column(&mut app);
    assert!(blank(&covered), "alternate screen: {covered:?}");
    app.term.shutdown();
}

/// What is drawn and what the program believes cannot disagree: every pane's
/// PTY and emulator are the drawn width less the bar's column.
#[test]
fn a_pane_reports_one_column_less_than_it_is_wide() {
    let (mut app, _rx) = test_app("wheelwidth");
    let ids = shells_tab(&mut app, &["a", "b"], "sleep 30");
    let _ = render(&mut app, 100, 30);
    for (i, id) in ids.iter().enumerate() {
        let (rect, _) = app.regions.panes[i];
        let s = app.term.session(*id).unwrap();
        let (_, cols) = s.size();
        assert_eq!(cols, rect.width - 1, "pane {i}");
        assert_eq!(s.parser().lock().unwrap().screen().size(), s.size());
    }
    app.term.shutdown();
}

/// Like a forwarded wheel, the pane under the pointer is the one that scrolls
/// — including over its title row and its bar, and whichever pane has focus.
#[test]
fn the_wheel_over_a_title_row_or_scrollbar_scrolls_that_pane() {
    let (mut app, _rx) = test_app("wheeledges");
    let ids = shells_tab(&mut app, &["a", "b"], "sleep 30");
    for id in &ids {
        feed_lines(&app, *id, 100);
    }
    let (r0, _) = app.regions.panes[0];
    let (r1, _) = app.regions.panes[1];
    assert_eq!(app.term.active_tab().unwrap().focus, 0);

    wheel_at(&mut app, r1.x + 3, r1.y, false); // pane 1's title row
    assert_eq!((offset(&app, ids[0]), offset(&app, ids[1])), (0, 3));

    wheel_at(&mut app, r0.x + r0.width - 1, r0.y + 3, false); // pane 0's bar
    assert_eq!((offset(&app, ids[0]), offset(&app, ids[1])), (3, 3));
    app.term.shutdown();
}

/// The bar's column is not the program's: a report there would name a cell it
/// does not have. The PTY is ordered, so once a later click has arrived, an
/// earlier one would have too.
#[test]
fn mouse_reports_never_land_on_the_scrollbar_column() {
    let (mut app, rx) = test_app("wheelbarclick");
    let id = shells_tab(&mut app, &["local"], MOUSE_ECHO)[0];
    assert!(
        wait_for_screen(&app, &rx, id, |s| {
            s.mouse_protocol_mode() != vt100::MouseProtocolMode::None
        }),
        "the child never enabled mouse reporting"
    );
    let (rect, _) = app.regions.panes[0];

    click(&mut app, rect.x + rect.width - 1, rect.y + 3); // the bar
    click(&mut app, rect.x + 4, rect.y + 3); // a sentinel, column 5
    assert!(
        wait_for_screen(&app, &rx, id, |s| s.contents().contains("^[[<0;5;4M")),
        "the sentinel click never arrived"
    );
    let seen = app
        .term
        .session(id)
        .unwrap()
        .parser()
        .lock()
        .unwrap()
        .screen()
        .contents();
    assert_eq!(
        seen.matches("^[[<0;").count(),
        1,
        "only the sentinel: {seen}"
    );
    app.term.shutdown();
}

/// A pane's bar scrolls its history: a press on the track jumps — the top of
/// the bar is the oldest line — and dragging to the bottom is the live screen.
#[test]
fn dragging_a_panes_scrollbar_scrolls_its_history() {
    use crate::app::ScrollTarget;
    let (mut app, _rx) = test_app("panebar-drag");
    let id = shells_tab(&mut app, &["local"], "sleep 30")[0];
    feed_lines(&app, id, 100);
    let _ = render(&mut app, 100, 30);
    let band = scrollbar_band(&app, ScrollTarget::Pane(id));

    // Taking hold of the thumb moves nothing, even where one of its rows
    // stands for several positions: one wheel notch back, then a press on it.
    let (x, y) = inside(&app, 0);
    wheel_at(&mut app, x, y, false);
    assert_eq!(offset(&app, id), 3);
    let _ = render(&mut app, 100, 30);
    let thumb = drawn_thumb(&mut app, band, 100, 30);
    press(&mut app, band.x, band.y + thumb);
    assert_eq!(offset(&app, id), 3, "a press on the thumb moves nothing");
    release(&mut app, band.x, band.y + thumb);

    press(&mut app, band.x, band.y);
    assert_eq!(offset(&app, id), 72, "the oldest line");
    assert_eq!(top_line(&app, id), "line 001");

    drag_to(&mut app, band.x, band.bottom() - 1);
    assert_eq!(offset(&app, id), 0, "the live screen");
    release(&mut app, band.x, band.bottom() - 1);

    // Held by the thumb and drawn up a few rows: somewhere in between.
    let _ = render(&mut app, 100, 30);
    let thumb = drawn_thumb(&mut app, band, 100, 30);
    press(&mut app, band.x, band.y + thumb);
    drag_to(&mut app, band.x, band.y + thumb - 6);
    let back = offset(&app, id);
    assert!(0 < back && back < 72, "part way back: {back}");
    release(&mut app, band.x, band.y + thumb - 6);
    app.term.shutdown();
}

/// From the press on the bar to the release, the pointer is the bar's: a
/// program that asked for motion reports hears nothing of a drag that
/// wanders over it, nor of the release — only the click that follows. And
/// the bar works although the program has the mouse: it is not the program's.
#[test]
fn a_scrollbar_drag_never_reaches_the_program() {
    use crate::app::ScrollTarget;
    let (mut app, rx) = test_app("panebar-forward");
    let id = shells_tab(
        &mut app,
        &["local"],
        "printf '\\033[?1002h\\033[?1006h'; cat -v",
    )[0];
    assert!(
        wait_for_screen(&app, &rx, id, |s| {
            s.mouse_protocol_mode() == vt100::MouseProtocolMode::ButtonMotion
        }),
        "the child never asked for motion reports"
    );
    feed_lines(&app, id, 100);
    let _ = render(&mut app, 100, 30);
    let band = scrollbar_band(&app, ScrollTarget::Pane(id));
    let (x, y) = inside(&app, 0);

    press(&mut app, band.x, band.y);
    assert!(offset(&app, id) > 0, "the bar moved the view");
    drag_to(&mut app, x, y);
    drag_to(&mut app, x + 3, y + 1);
    release(&mut app, x + 3, y + 1);

    app.term.session(id).unwrap().scroll_to_live();
    click(&mut app, x, y); // the sentinel
    assert!(
        wait_for_screen(&app, &rx, id, |s| s.contents().contains("^[[<0;5;4M")),
        "the sentinel click never arrived"
    );
    let seen = app
        .term
        .session(id)
        .unwrap()
        .parser()
        .lock()
        .unwrap()
        .screen()
        .contents();
    assert_eq!(seen.matches("^[[<").count(), 1, "only the sentinel: {seen}");
    app.term.shutdown();
}

/// Scrolling a pane you are not typing in leaves the keyboard where it was,
/// as the wheel does.
#[test]
fn dragging_an_unfocused_panes_scrollbar_leaves_the_keyboard_where_it_was() {
    use crate::app::ScrollTarget;
    let (mut app, _rx) = test_app("panebar-focus");
    let ids = shells_tab(&mut app, &["a", "b"], "sleep 30");
    for id in &ids {
        feed_lines(&app, *id, 100);
    }
    let _ = render(&mut app, 100, 30);
    let band = scrollbar_band(&app, ScrollTarget::Pane(ids[1]));

    press(&mut app, band.x, band.y);
    release(&mut app, band.x, band.y);
    assert!(offset(&app, ids[1]) > 0);
    assert_eq!(offset(&app, ids[0]), 0);
    assert_eq!(app.term.active_tab().unwrap().focus, 0);
    app.term.shutdown();
}

#[test]
fn proxy_toggle_is_exclusive() {
    let (mut app, _rx) = test_app("proxyflag");
    // Setting the flag directly exercises the mutual exclusion in the DB layer
    // without opening a real tunnel.
    let mut a = app.hosts[0].clone();
    a.proxy = true;
    app.db.save(&a).unwrap();
    let mut b = app.hosts[1].clone();
    b.proxy = true;
    app.db.save(&b).unwrap();
    app.db.clear_proxy_except(b.id).unwrap();
    app.reload();
    assert_eq!(app.hosts.iter().filter(|h| h.proxy).count(), 1);
    assert_eq!(app.proxy_host().unwrap().name, "db-main");
}

// ---- the plan gate -------------------------------------------------------

use crate::agent::plan::{Plan, PlanStep, StepKind};

fn a_plan(script: &str, hosts: Vec<i64>) -> Plan {
    Plan {
        id: 1,
        title: "tidy up".into(),
        steps: vec![PlanStep {
            summary: "clean the cache".into(),
            kind: StepKind::Scriptlet {
                script: script.into(),
            },
            hosts,
        }],
    }
}

/// Arrival alone opens nothing: the event handler records a card and stops.
/// Showing the dialog is a separate decision with its own conditions
/// (`maybe_auto_open_plan`), and the card has to stand on its own for every
/// case where those conditions say no.
#[test]
fn a_proposal_arrives_as_a_card_that_advertises_the_key() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("noautoopen");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan(
        "apt-get clean",
        vec![1],
    ))));

    assert_eq!(app.mode, Mode::Normal, "no modal appeared");
    assert!(app.plan.is_none());
    assert!(app.pending_plan.is_some(), "but it is waiting");
    let out = render(&mut app, 120, 30);
    assert!(out.contains("plan #1"), "a card names it: {out}");
    assert!(out.contains("awaiting review"), "{out}");
    // ...and says how to get in. A dialog nobody can find is a dialog that
    // does not exist — which is exactly how this shipped the first time.
    assert!(out.contains("F2"), "the card names the key: {out}");
    let rows: Vec<&str> = out.lines().collect();
    assert!(
        rows[rows.len() - 1].contains("F2") && rows[rows.len() - 1].contains("Review plan"),
        "the function bar offers it too: {}",
        rows[rows.len() - 1]
    );
}

/// The card advertises "click here", so it has to be clickable.
#[test]
fn clicking_the_plan_card_opens_the_dialog() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("cardclick");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("id", vec![1]))));
    let _ = render(&mut app, 120, 30);

    let card = app.regions.plan_card.expect("the card registers a hitbox");
    click(&mut app, card.x + 4, card.y);
    assert_eq!(app.mode, Mode::ConfirmPlan, "the dialog opened");
}

/// Once decided, the card is no longer a way in.
#[test]
fn a_decided_card_is_not_clickable() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("decidedcard");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("id", vec![1]))));
    app.function_key(2);
    key(&mut app, KeyCode::Esc); // reject

    let out = render(&mut app, 120, 30);
    assert!(out.contains("rejected"), "{out}");
    assert!(
        app.regions.plan_card.is_none(),
        "no hitbox on a decided card"
    );
    assert!(!out.contains("F2"), "and the key is not offered: {out}");
}

#[test]
fn the_dialog_shows_every_step_host_and_the_script_verbatim() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("dialog");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan(
        "set -euo pipefail\napt-get clean",
        vec![1, 2],
    ))));
    app.function_key(2);
    assert_eq!(app.mode, Mode::ConfirmPlan);

    let out = render(&mut app, 120, 34);
    assert!(out.contains("Confirm Plan"), "{out}");
    assert!(out.contains("Nothing has run yet"), "{out}");
    assert!(out.contains("clean the cache"), "{out}");
    // The script is shown in full — an elided one is unreviewable.
    assert!(out.contains("set -euo pipefail"), "{out}");
    assert!(out.contains("apt-get clean"), "{out}");
    // Both hosts, by name, each with a box.
    assert!(out.contains("web-01"), "{out}");
    assert!(out.contains("db-main"), "{out}");
    assert!(out.contains("[x]"), "{out}");
    assert!(out.contains("Run 1 step(s) on 2 host(s)"), "{out}");
}

/// An operator approving an upload is approving a destination, so the dialog
/// shows the path the file actually lands at — read from the same constant the
/// executor scps to, never a literal in the renderer.
#[test]
fn the_dialog_names_where_an_upload_lands() {
    use crate::agent::AgentEvent;
    use crate::agent::artifacts;
    let (mut app, _rx) = test_app("uploaddest");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(Plan {
        id: 1,
        title: "ship the config".into(),
        steps: vec![PlanStep {
            summary: "upload the site config".into(),
            kind: StepKind::Upload {
                artifact: "nginx/site.conf".into(),
            },
            hosts: vec![1],
        }],
    })));
    app.function_key(2);
    assert_eq!(app.mode, Mode::ConfirmPlan);

    let out = render(&mut app, 120, 34);
    assert!(out.contains("nginx/site.conf"), "{out}");
    assert!(
        out.contains(&artifacts::upload_target("nginx/site.conf").1),
        "the whole destination, not just the directory: {out}"
    );
}

/// The plan dialog showing `script`, drawn at `w`×`h`.
fn plan_dialog(tag: &str, script: &str, w: u16, h: u16) -> (App, ratatui::buffer::Buffer) {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app(tag);
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan(script, vec![1]))));
    app.function_key(2);
    assert_eq!(app.mode, Mode::ConfirmPlan);
    let buf = render_buf(&mut app, w, h);
    (app, buf)
}

/// The cell where `needle` first appears on screen. ASCII needles only: a
/// cell per character.
fn cell_of<'b>(buf: &'b ratatui::buffer::Buffer, needle: &str) -> &'b ratatui::buffer::Cell {
    let area = buf.area;
    for y in area.y..area.bottom() {
        let row: String = (area.x..area.right())
            .map(|x| buf[(x, y)].symbol())
            .collect();
        if let Some(i) = row.find(needle) {
            let x = row[..i].chars().count() as u16;
            return &buf[(area.x + x, y)];
        }
    }
    panic!("{needle:?} is not on screen");
}

fn screen_text(buf: &ratatui::buffer::Buffer) -> String {
    let area = buf.area;
    (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The dialog shows what runs, character for character: indentation, and a
/// run of spaces inside quotes — `echo "a    b"` runs differently from
/// `echo "a b"`, which is what the word wrap used to show.
#[test]
fn the_plan_dialog_shows_a_script_character_for_character() {
    let (_app, buf) = plan_dialog(
        "scriptexact",
        "if true; then\n    echo \"a    b\"\nfi",
        120,
        34,
    );
    let out = screen_text(&buf);
    assert!(out.contains("│     echo \"a    b\""), "{out}");
}

/// A line too long for a row continues under a dashed gutter, so it cannot
/// read as two commands; its rows put back together are the line.
#[test]
fn a_wrapped_line_continues_under_a_dashed_gutter() {
    let line = (0..40)
        .map(|i| format!("arg{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!("echo {line}");
    let (_app, buf) = plan_dialog("scriptwrap", &script, 80, 40);
    let out = screen_text(&buf);
    let rows: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("│ ") || l.contains("┆ "))
        .filter(|l| l.contains("arg") || l.contains("echo"))
        .collect();
    assert!(rows.len() >= 3, "{out}");
    assert!(rows[0].contains("    │ echo"), "{out}");
    let mut joined = Vec::new();
    for (k, r) in rows.iter().enumerate() {
        let gutter = if k == 0 { "│ " } else { "┆ " };
        let at = r
            .find(gutter)
            .unwrap_or_else(|| panic!("row {k} has no {gutter:?}: {r}"));
        let text = &r[at + gutter.len()..];
        // Up to the dialog's right border.
        let text = text.split('║').next().unwrap_or(text);
        joined.push(text.trim_end().to_string());
    }
    assert_eq!(joined.join(" "), script, "{out}");
}

/// What runs stands out: the command word bright and bold, a string green.
#[test]
fn the_command_word_is_bright_and_a_string_green() {
    use crate::ui::theme;
    use ratatui::style::Modifier;
    let (_app, buf) = plan_dialog("scriptcolour", "systemctl restart 'nginx'", 120, 34);
    let cmd = cell_of(&buf, "systemctl");
    assert_eq!(cmd.fg, theme::FG_BRIGHT);
    assert!(cmd.modifier.contains(Modifier::BOLD));
    assert_eq!(cell_of(&buf, "'nginx'").fg, theme::GREEN);
    assert_eq!(
        cell_of(&buf, "restart").fg,
        theme::FG,
        "an argument is plain"
    );
}

/// A comment is muted and italic — distinct and readable, never faint.
#[test]
fn a_comment_is_muted_and_italic() {
    use crate::ui::theme;
    use ratatui::style::Modifier;
    let (_app, buf) = plan_dialog("scriptcomment", "apt-get update # refresh", 120, 34);
    let c = cell_of(&buf, "# refresh");
    assert_eq!(c.fg, theme::FG_MUTED);
    assert!(c.modifier.contains(Modifier::ITALIC));
}

/// The screen hides an escape sequence; bash does not. Here the `#` after one
/// is inside a word — live — though with the sequence stripped it would read
/// as a comment. The sequence is shown, and nothing after it is dressed as one.
#[test]
fn an_escape_sequence_cannot_dress_live_code_as_a_comment() {
    use crate::ui::theme;
    use ratatui::style::Modifier;
    let (_app, buf) = plan_dialog("scriptesc", "echo x \u{1b}[0m# y; rm -rf /srv", 120, 34);
    let out = screen_text(&buf);
    assert!(out.contains("echo x ^[[0m# y; rm -rf /srv"), "{out}");
    let hash = cell_of(&buf, "# y");
    assert!(!hash.modifier.contains(Modifier::ITALIC), "not a comment");
    assert_ne!(hash.fg, theme::FG_MUTED);
    let rm = cell_of(&buf, "rm -rf");
    assert_eq!(rm.fg, theme::FG_BRIGHT, "and what it runs is bright");
    assert!(rm.modifier.contains(Modifier::BOLD));
}

/// A command hidden inside an escape sequence — which the dialog used to strip,
/// showing `echo hello world` while bash ran `echo LIVE` — is on screen, with
/// the sequence's control characters shown by name in the alarm style.
#[test]
fn a_hidden_command_is_shown_not_stripped() {
    use crate::ui::theme;
    let (_app, buf) = plan_dialog(
        "scripthidden",
        "echo hello \u{1b}]0; echo LIVE \u{7}world",
        120,
        34,
    );
    let out = screen_text(&buf);
    assert!(out.contains("echo hello ^[]0; echo LIVE ^Gworld"), "{out}");
    assert_eq!(cell_of(&buf, "^[").fg, theme::RED);
    assert_eq!(cell_of(&buf, "^G").fg, theme::RED);
}

/// Counted in characters, a line of wide characters fitted a row and had its
/// end clipped off screen — here, the `rm`. Counted in cells, it wraps.
#[test]
fn wide_characters_cannot_push_code_off_the_row() {
    let script = format!("echo {} ; rm -rf ~", "日".repeat(60));
    let (_app, buf) = plan_dialog("scriptwide", &script, 120, 34);
    let out = screen_text(&buf);
    assert!(out.contains("rm -rf ~"), "{out}");
}

#[test]
fn unchecking_a_host_changes_what_would_run() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("uncheck");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("id", vec![1, 2]))));
    app.function_key(2);
    let _ = render(&mut app, 120, 34);

    // Move to the first host row and toggle it off.
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char(' '));
    let sel = app.plan.as_ref().unwrap();
    assert_eq!(sel.counts(), (1, 1));
    let out = render(&mut app, 120, 34);
    assert!(out.contains("Run 1 step(s) on 1 host(s)"), "{out}");

    // Turning everything off leaves nothing to run, and no button to press.
    key(&mut app, KeyCode::Char('x'));
    assert!(app.plan.as_ref().unwrap().is_empty());
    let out = render(&mut app, 120, 34);
    assert!(out.contains("Nothing selected"), "{out}");
    assert!(!out.contains("Run 1 step"), "{out}");
}

/// Confirming with nothing selected must not close the dialog on a no-op.
#[test]
fn confirming_an_empty_selection_keeps_the_dialog() {
    use crate::agent::AgentEvent;
    use crate::app::StatusKind;
    let (mut app, _rx) = test_app("emptyconfirm");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("id", vec![1]))));
    app.function_key(2);
    key(&mut app, KeyCode::Char('x'));
    key(&mut app, KeyCode::Enter);

    assert_eq!(app.mode, Mode::ConfirmPlan, "the dialog stays up");
    assert_eq!(app.status.kind, StatusKind::Warn);
    assert!(app.status.text.contains("Nothing selected"));
}

#[test]
fn rejecting_a_plan_runs_nothing_and_marks_the_card() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("reject");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("id", vec![1]))));
    app.function_key(2);
    key(&mut app, KeyCode::Esc);

    assert_eq!(app.mode, Mode::Normal);
    assert!(app.plan.is_none());
    assert!(app.pending_plan.is_none());
    assert!(!app.busy, "nothing was started");
    let out = render(&mut app, 120, 30);
    assert!(out.contains("rejected"), "{out}");
}

/// Clicking a checkbox must toggle the row that was drawn there.
#[test]
fn dialog_checkbox_hitboxes_land_on_the_rows_drawn() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("planclicks");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("id", vec![1, 2]))));
    app.function_key(2);
    let _ = render(&mut app, 120, 34);

    let host_clicks: Vec<_> = app
        .regions
        .clicks
        .iter()
        .filter_map(|(r, c)| match c {
            Click::ToggleHost(i, j) => Some((*r, *i, *j)),
            _ => None,
        })
        .collect();
    assert_eq!(host_clicks.len(), 2, "one hitbox per host row");

    let (rect, i, j) = host_clicks[1];
    click(&mut app, rect.x + 6, rect.y);
    assert!(
        !app.plan.as_ref().unwrap().host_on[i as usize][j as usize],
        "the clicked host was the one toggled"
    );
}

/// A plan too tall for the dialog scrolls rather than truncating.
#[test]
fn a_long_plan_scrolls_and_keeps_the_cursor_visible() {
    use crate::agent::AgentEvent;
    let steps: Vec<PlanStep> = (0..12)
        .map(|i| PlanStep {
            summary: format!("step number {i}"),
            kind: StepKind::Scriptlet {
                script: format!("echo {i}"),
            },
            hosts: vec![1, 2],
        })
        .collect();
    let (mut app, _rx) = test_app("longplan");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(Plan {
        id: 2,
        title: "big".into(),
        steps,
    })));
    app.function_key(2);

    let out = render(&mut app, 120, 24);
    assert!(out.contains("PgDn ↓"), "the hint says there is more: {out}");
    assert!(out.contains("step number 0"), "{out}");

    // Walk to the bottom; the last step must come into view.
    for _ in 0..60 {
        key(&mut app, KeyCode::Down);
    }
    let out = render(&mut app, 120, 24);
    assert!(
        out.contains("step number 11"),
        "the cursor stays visible: {out}"
    );
    // 12 steps on the same 2 hosts is 2 hosts, not 24 step×host runs.
    assert!(out.contains("Run 12 step(s) on 2 host(s)"), "{out}");
}

/// The dialog takes the transcript's place exactly, and no more: a centred
/// box capped at 78×34 wasted a large terminal on the transcript it was hiding
/// anyway, while the composer below stays the operator's.
#[test]
fn the_plan_dialog_covers_the_transcript_not_the_composer() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("plancover");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("id", vec![1]))));
    app.function_key(2);

    let buf = render_buf(&mut app, 160, 60);
    let log = app.regions.chat_log.expect("the chat screen records it");
    assert_eq!(
        buf[(log.x, log.y)].symbol(),
        "╔",
        "top-left on the transcript's"
    );
    assert_eq!(
        buf[(log.right() - 1, log.bottom() - 1)].symbol(),
        "╝",
        "bottom-right on the transcript's"
    );
    assert_eq!(log.width, 160, "the full width");
    assert!(log.height > 34, "and past the old cap: {log:?}");
    assert_eq!(
        buf[(log.x, log.bottom())].symbol(),
        "┌",
        "the composer is still drawn right below it"
    );
}

/// A plan whose one step is a 200-line script: no row sits between the step
/// and its host, so ↑/↓ alone could never show the middle of it.
fn a_long_script_plan() -> Plan {
    let script: String = (0..200).map(|i| format!("echo {i:03}\n")).collect();
    a_plan(&script, vec![1])
}

/// Every line of a script longer than the dialog can be brought into view,
/// and stays there: the cursor, left above on the step, must not drag the
/// window back on the next frame.
#[test]
fn a_long_script_can_be_read_to_the_end() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("longscript");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_long_script_plan())));
    app.function_key(2);

    let out = render(&mut app, 120, 30);
    assert!(out.contains("echo 000"), "{out}");
    assert!(!out.contains("echo 199"), "{out}");
    assert!(
        out.contains("PgDn"),
        "the hint names the key that scrolls: {out}"
    );

    key(&mut app, KeyCode::PageDown);
    let out = render(&mut app, 120, 30);
    assert!(
        !out.contains("echo 000") && !out.contains("echo 199"),
        "the middle of the script is on screen: {out}"
    );

    for _ in 0..40 {
        key(&mut app, KeyCode::PageDown);
        let _ = render(&mut app, 120, 30);
    }
    let out = render(&mut app, 120, 30);
    assert!(out.contains("echo 199"), "the end is reachable: {out}");
    assert!(out.contains("web-01"), "and the host under it: {out}");
    assert!(out.contains("PgUp"), "the hint says what is above: {out}");
    // The step row went out of sight, so the cursor came along to the one row
    // that is in it — and Space acts on something the operator can see.
    assert_eq!(app.plan.as_ref().unwrap().cursor, 1);
    key(&mut app, KeyCode::Char(' '));
    assert!(!app.plan.as_ref().unwrap().host_on[0][0]);
    let out = render(&mut app, 120, 30);
    assert!(
        out.contains("echo 199"),
        "toggling in view does not jump: {out}"
    );

    for _ in 0..40 {
        key(&mut app, KeyCode::PageUp);
        let _ = render(&mut app, 120, 30);
    }
    let out = render(&mut app, 120, 30);
    assert!(out.contains("echo 000"), "and back to the top: {out}");
    assert_eq!(app.plan.as_ref().unwrap().cursor, 0, "cursor on the step");

    key(&mut app, KeyCode::End);
    let out = render(&mut app, 120, 30);
    assert!(out.contains("web-01") && !out.contains("echo 000"), "{out}");
    key(&mut app, KeyCode::Home);
    let out = render(&mut app, 120, 30);
    assert!(out.contains("echo 000"), "{out}");
}

/// The wheel scrolls the dialog, not the transcript under it.
#[test]
fn the_wheel_scrolls_the_plan_dialog() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("planwheel");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_long_script_plan())));
    app.function_key(2);
    let _ = render(&mut app, 120, 30);

    wheel(&mut app, true);
    wheel(&mut app, true);
    assert_eq!(app.plan.as_ref().unwrap().scroll, 6);
    let out = render(&mut app, 120, 30);
    assert!(!out.contains("echo 000"), "{out}");

    wheel(&mut app, false);
    wheel(&mut app, false);
    assert_eq!(app.plan.as_ref().unwrap().scroll, 0);
    let out = render(&mut app, 120, 30);
    assert!(out.contains("echo 000"), "{out}");
}

/// The right border over the body is the scrollbar, and its thumb spans the
/// band exactly: flush with the top at the start, the bottom at the end.
#[test]
fn the_plan_scrollbar_thumb_runs_the_length_of_the_body() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("planbar");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_long_script_plan())));
    app.function_key(2);

    let buf = render_buf(&mut app, 120, 30);
    let log = app.regions.chat_log.unwrap();
    let x = log.right() - 1;
    // Border and padding, then the title, "Nothing has run yet" and a blank
    // above the body; a blank, the hint and the buttons below it.
    let top = log.y + 1 + 1 + 3;
    let bottom = log.bottom() - 1 - 1 - 3;
    assert_eq!(buf[(x, top)].symbol(), "█", "thumb at the top");
    assert_ne!(buf[(x, bottom - 1)].symbol(), "█");
    assert_eq!(buf[(x, top - 1)].symbol(), "║", "the frame above the band");

    for _ in 0..40 {
        key(&mut app, KeyCode::PageDown);
        let _ = render(&mut app, 120, 30);
    }
    let buf = render_buf(&mut app, 120, 30);
    assert_eq!(buf[(x, bottom - 1)].symbol(), "█", "thumb at the bottom");
    assert_ne!(buf[(x, top)].symbol(), "█");
    assert_eq!(buf[(x, bottom)].symbol(), "║", "the frame below the band");
}

/// Drive the loop's invariant the way `event_loop` does, once.
fn tick(app: &mut App) -> bool {
    app.maybe_auto_open_plan()
}

/// The wait is the thing worth removing: the turn is suspended until the plan
/// is answered, so a plan nobody has looked at is an agent doing nothing.
#[test]
fn a_waiting_plan_opens_itself_on_the_chat_screen() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("autoopen");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("ls", vec![1]))));

    assert!(tick(&mut app), "it opened");
    assert_eq!(app.mode, Mode::ConfirmPlan);
    let out = render(&mut app, 120, 30);
    assert!(out.contains("Confirm Plan"), "{out}");
    assert!(out.contains("↵ run"), "one press, and it says so: {out}");
    assert!(!tick(&mut app), "and it does not re-open over itself");
}

/// On the Shells screen every key belongs to a remote terminal. A modal there
/// would eat one — possibly mid-command, in a program like mc that reads
/// single keys.
#[test]
fn a_waiting_plan_does_not_steal_the_shells_screen() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("autoopen-shells");
    app.screen = Screen::Shells;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("ls", vec![1]))));

    assert!(!tick(&mut app), "not while a shell has the keyboard");
    assert_eq!(app.mode, Mode::Normal);

    // It is waiting, not lost: arriving on Chat is enough.
    app.screen = Screen::Chat;
    assert!(tick(&mut app));
    assert_eq!(app.mode, Mode::ConfirmPlan);
}

/// A sentence half typed into the composer is work in progress. The dialog
/// would not merely interrupt it — it would read the letters as commands, and
/// `n` on its own rejects the plan.
#[test]
fn a_waiting_plan_does_not_interrupt_a_half_typed_message() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("autoopen-draft");
    app.screen = Screen::Chat;
    app.chat.draft = tui_textarea::TextArea::from(["no, wait — check the"]);
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("ls", vec![1]))));

    assert!(!tick(&mut app), "not over a draft");
    assert_eq!(app.mode, Mode::Normal);
    let out = render(&mut app, 120, 30);
    assert!(
        out.contains("awaiting review"),
        "the card still offers it: {out}"
    );

    app.chat.draft = tui_textarea::TextArea::default();
    assert!(tick(&mut app), "and it appears once the draft is gone");
}

/// The operator who watched the plan arrive and read it without touching
/// anything used to need two presses: a dialog that opened by itself swallowed
/// the first ↵, in case it had been in flight. Nothing about that ↵ says which
/// it was, so the gate charged everyone for the rare stray one — and the only
/// explanation was a status line that reverted within seconds.
#[test]
fn an_auto_opened_dialog_runs_on_the_first_enter() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("autoopen-enter");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("ls", vec![1]))));
    assert!(tick(&mut app), "it opened by itself");

    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal, "one press answered it");
    assert!(app.pending_plan.is_none(), "and nothing is left waiting");
}

/// The same for a dialog the operator opened with F2.
#[test]
fn a_dialog_the_operator_opened_runs_on_the_first_enter() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("openarm");
    app.screen = Screen::Shells; // no auto-open here
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("ls", vec![1]))));
    app.screen = Screen::Chat;
    key(&mut app, KeyCode::F(2));
    assert_eq!(app.mode, Mode::ConfirmPlan);

    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal, "it ran");
}

/// The dialog now arrives uninvited, on top of the transcript that explains
/// why it exists. Putting it away must not be the same act as refusing it.
#[test]
fn hiding_a_plan_leaves_it_waiting_and_it_does_not_spring_back() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("planhide");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("ls", vec![1, 2]))));
    tick(&mut app);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char(' ')); // uncheck a host

    key(&mut app, KeyCode::F(2));
    assert_eq!(app.mode, Mode::Normal, "out of the way");
    assert!(app.pending_plan.is_some(), "but not answered");
    assert!(!tick(&mut app), "and it stays out of the way");
    let out = render(&mut app, 120, 30);
    assert!(
        out.contains("awaiting review"),
        "the card still offers it: {out}"
    );

    key(&mut app, KeyCode::F(2));
    assert_eq!(app.mode, Mode::ConfirmPlan, "F2 brings it back");
    let sel = app.plan.as_ref().unwrap();
    assert!(!sel.host_on[0][0], "with the checkbox as it was left");
}

/// A second proposal is a new decision: whatever the operator did with the
/// last one must not keep the new one off the screen.
#[test]
fn a_new_proposal_shows_itself_even_after_the_last_was_hidden() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("planhide2");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("ls", vec![1]))));
    tick(&mut app);
    key(&mut app, KeyCode::F(2));

    let mut next = a_plan("df -h", vec![1]);
    next.id = 2;
    app.on_agent_event(AgentEvent::Proposed(Box::new(next)));
    assert!(tick(&mut app), "the new one opens");
    assert_eq!(app.plan.as_ref().unwrap().plan.id, 2);
}

/// Every tool the model has goes over SSH, so an FTP entry is a name it could
/// use for nothing. It never learns one exists.
#[test]
fn the_agent_is_shown_ssh_hosts_only() {
    let (mut app, _rx) = test_app("agenthosts");
    app.hosts.push(crate::db::model::HostRecord {
        id: 99,
        name: "files-01".into(),
        proto: "ftp".into(),
        addr: "10.0.9.9".into(),
        port: 21,
        ..Default::default()
    });
    let seen: Vec<String> = app
        .agent_scope()
        .hosts
        .into_iter()
        .map(|h| h.name)
        .collect();
    assert!(!seen.is_empty(), "the ssh ones are still there");
    assert!(
        !seen.iter().any(|n| n == "files-01"),
        "but not ftp: {seen:?}"
    );
}

/// Render and hand back the buffer, for tests that care about colour rather
/// than glyphs.
fn render_buf(app: &mut App, w: u16, h: u16) -> ratatui::buffer::Buffer {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw(f, app)).unwrap();
    term.backend().buffer().clone()
}

/// Inputs are boxes on the panel, not wells sunk into it.
///
/// The old field was a run of `BG_INSET` two shades darker than the dialog,
/// which read as a hole rather than a control. Every input now wears the
/// composer's treatment: the panel's own background inside an orange border.
#[test]
fn form_inputs_are_orange_boxes_on_the_panel_background() {
    use crate::ui::theme;
    let (mut app, _rx) = test_app("inputstyle");
    app.open_add();
    let buf = render_buf(&mut app, 110, 30);

    let mut inset = 0;
    let mut bright = 0;
    let mut dim = 0;
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let c = &buf[(x, y)];
            if c.bg == theme::BG_INSET {
                inset += 1;
            }
            if c.fg == theme::ORANGE_BRIGHT {
                bright += 1;
            }
            if c.fg == theme::ORANGE_DIM {
                dim += 1;
            }
        }
    }
    assert_eq!(inset, 0, "no well is left anywhere in the form");
    // The focused field is bright, the rest dim — the only thing separating
    // them, since every box is orange.
    assert!(bright > 0, "the focused box is drawn bright");
    assert!(
        dim > bright,
        "and the unfocused ones outnumber it: {dim}/{bright}"
    );

    let out = render(&mut app, 110, 30);
    for label in [
        "┌ Host name",
        "┌ Type",
        "┌ Address",
        "┌ Port",
        "┌ Mount point",
        "┌ Login",
        "┌ Password",
    ] {
        assert!(out.contains(label), "{label} is boxed: {out}");
    }

    // The notes that used to hang in the bottom borders are gone; the mount
    // field's state moved into its label, where it does not interrupt a frame.
    assert!(!out.contains("the mount point follows it"), "{out}");
    assert!(out.contains("Mount point · auto"), "{out}");
}

/// The same treatment on the two dialogs that stand between the operator and
/// the program starting at all.
#[test]
fn the_password_dialogs_use_the_same_boxes() {
    use crate::ui::dialogs::password_prompt;
    use tui_input::Input;
    let three = Input::new("•••".to_string());
    let empty = Input::default();
    for (title, fields, err) in [
        ("Unlock Database", &[("Password", &three)][..], None),
        (
            "Create Database",
            &[("New password", &three), ("Confirm", &empty)][..],
            Some("Passwords do not match."),
        ),
    ] {
        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        term.draw(|f| password_prompt(f, title, fields, 0, err, "Enter unlock   Esc quit"))
            .unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(title), "{text}");
        for (label, _) in fields {
            assert!(
                text.contains(&format!("┌ {label}")),
                "{label} is boxed: {text}"
            );
        }
        if let Some(e) = err {
            assert!(text.contains(e), "the error still shows: {text}");
        }
        assert!(
            text.contains("Esc quit"),
            "and the hint is not clipped: {text}"
        );
        assert_eq!(
            (0..buf.area.height)
                .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
                .filter(|&(x, y)| buf[(x, y)].bg == crate::ui::theme::BG_INSET)
                .count(),
            0,
            "no wells here either"
        );
    }
}

/// The composer is a text area now: `Enter` sends, `Ctrl+J` opens a line, and
/// it grows to fit what is in it.
#[test]
fn the_composer_takes_more_than_one_line() {
    let (mut app, _rx) = test_app("composer-multiline");
    app.screen = Screen::Chat;
    for c in "check the disks".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
    for c in "on every host".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    assert_eq!(app.chat.draft_text(), "check the disks\non every host");
    assert_eq!(app.chat.draft.lines().len(), 2, "^J opened a line");

    // Both lines are on screen, so the box grew rather than hiding the first.
    let out = render(&mut app, 100, 30);
    assert!(out.contains("check the disks"), "{out}");
    assert!(out.contains("on every host"), "{out}");

    // Sending takes the whole thing, not just the line the caret is on.
    // (Driven through `take_draft` rather than Enter, because `send_chat`
    // needs a configured model and this is about the composer.)
    let sent = app.chat.take_draft().expect("a draft to send");
    assert_eq!(
        sent, "check the disks\non every host",
        "both lines were sent"
    );
    assert!(app.chat.draft.is_empty(), "and the composer is cleared");
}

/// A pasted block keeps its lines. Flattening a log into one line is what the
/// old composer did, and the reason this one exists.
#[test]
fn a_pasted_block_keeps_its_lines() {
    // All three line endings a terminal might send. CR is not a curiosity: it
    // is what tmux and most emulators put inside a bracketed paste, and
    // crossterm forwards the bytes untouched — so testing only "\n" passes
    // while every real paste arrives as one flattened line.
    for sep in ["\n", "\r", "\r\n"] {
        let (mut app, _rx) = test_app("composer-paste");
        app.screen = Screen::Chat;
        app.on_paste(&["line one", "line two", "line three"].join(sep));
        assert_eq!(
            app.chat.draft.lines().len(),
            3,
            "{sep:?} did not break lines: {:?}",
            app.chat.draft_text()
        );
        let out = render(&mut app, 100, 30);
        for l in ["line one", "line two", "line three"] {
            assert!(out.contains(l), "{l} is missing: {out}");
        }
    }
}

/// The transcript shows a multi-line message as it was written.
#[test]
fn a_multiline_message_keeps_its_shape_in_the_transcript() {
    let (mut app, _rx) = test_app("composer-transcript");
    app.screen = Screen::Chat;
    app.on_paste("first line\n\nthird line");
    app.chat.take_draft().expect("a draft to send");
    let out = render(&mut app, 100, 30);
    let rows: Vec<&str> = out.lines().collect();
    let first = rows
        .iter()
        .position(|r| r.contains("first line"))
        .expect("the message is shown");
    assert!(
        rows[first + 2].contains("third line"),
        "the blank line between them survives: {out}"
    );
}

/// A long prompt folds instead of sliding out of view sideways. Text that
/// scrolls off as you type is text you cannot re-read, and a prompt is written
/// to be re-read before it is sent.
#[test]
fn the_composer_wraps_a_long_line_and_grows_to_fit() {
    let (mut app, _rx) = test_app("composer-wrap");
    app.screen = Screen::Chat;
    let long = "check whether the nginx workers are leaking descriptors on the busiest hosts";
    app.on_paste(long);
    assert_eq!(app.chat.draft.lines().len(), 1, "still one logical line");

    let out = render(&mut app, 60, 24);
    let rows: Vec<&str> = out.lines().collect();
    let first = rows
        .iter()
        .position(|r| r.contains("check whether"))
        .expect("the prompt is shown");
    assert!(
        rows[first + 1].contains("hosts"),
        "the tail folded onto the next row rather than scrolling away: {out}"
    );
    // Every word survived the fold.
    let shown: String = rows[first..].concat();
    for word in ["nginx", "descriptors", "busiest"] {
        assert!(shown.contains(word), "{word} is missing: {out}");
    }
}

/// Folding does not let the composer grow without limit: the rows come out of
/// the transcript above it.
#[test]
fn the_composer_stops_growing_at_its_cap() {
    let (mut app, _rx) = test_app("composer-cap");
    app.screen = Screen::Chat;
    app.on_paste(
        &(1..=40)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    // The cap is applied when the composer is dressed, which happens as it is
    // drawn — so measure what the frame actually decided.
    let _ = render(&mut app, 80, 24);
    let tall = app.chat.draft.measure(80).preferred_rows;
    assert_eq!(tall, 10, "capped at ten rows, borders included");

    // And an empty composer still occupies its minimum.
    app.chat.draft = tui_textarea::TextArea::default();
    let _ = render(&mut app, 80, 24);
    assert_eq!(app.chat.draft.measure(80).preferred_rows, 3);
}

/// A backend error goes into the transcript, whole, under the message that
/// caused it. The status line has 48 columns and a few seconds for it, and the
/// part of a backend error that says what is wrong is rarely in the first 48
/// columns — this one's is the model name, at the end.
#[test]
fn a_backend_error_is_printed_into_the_chat_in_full() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("backend-error");
    app.screen = Screen::Chat;
    app.chat.turns.clear();
    app.chat.turns.push(crate::app::chat::Turn::User(
        "what disks does web-01 have?".into(),
    ));
    app.busy = true;

    let body = "HTTP 404 — check base_url and model\n\
                {\"error\":{\"message\":\"model \\\"qwen3.5:9b-instruct\\\" not found, try pulling it first\"}}";
    app.on_agent_event(AgentEvent::Error(body.to_string()));

    assert!(!app.busy, "the turn is over");
    assert!(
        matches!(app.chat.turns.last(), Some(crate::app::chat::Turn::Error(e)) if e == body),
        "the whole error is kept, not a truncation of it"
    );
    let out = render(&mut app, 120, 30);
    assert!(out.contains("HTTP 404"), "{out}");
    // The line that actually names the problem, past where the status line
    // would have cut it off.
    assert!(out.contains("not found, try pulling it first"), "{out}");
    assert!(out.contains("qwen3.5:9b-instruct"), "{out}");
    // Under the message that caused it.
    let asked = out.find("what disks does web-01").unwrap();
    let failed = out.find("HTTP 404").unwrap();
    assert!(asked < failed, "the error follows the question: {out}");
    // And a dialog makes sure it is seen, holding the error itself rather than
    // a pointer to it.
    let err = app.alert.clone().expect("an error dialog");
    assert_eq!(err, body);
    assert!(out.contains("Dismiss"), "{out}");
    // The transcript keeps it after the dialog is gone.
    key(&mut app, KeyCode::Enter);
    assert!(app.alert.is_none());
    let out = render(&mut app, 120, 30);
    assert!(out.contains("not found, try pulling it first"), "{out}");
    assert!(!out.contains("Dismiss"), "{out}");
}

/// A server chose this text, so escape sequences in it must not reach the
/// operator's terminal.
#[test]
fn a_backend_error_is_sanitized_before_it_is_drawn() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("backend-error-esc");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Error(
        "bad gateway\x1b[2J\x1b]0;pwned\x07 upstream closed".into(),
    ));
    let out = render(&mut app, 120, 30);
    assert!(out.contains("bad gateway"), "{out}");
    assert!(out.contains("upstream closed"), "{out}");
    assert!(!out.contains('\x1b'), "no escape reaches the screen");
    assert!(
        !out.contains("pwned"),
        "an OSC title is dropped whole: {out}"
    );
}

/// The Hosts keymap: letter twins, Enter for a shell, F9 for mount, and the
/// old F3/F5/F7 unbound — and F12 and `i`, whose Actions menu is gone. Every
/// assertion goes through a path that cannot reach sshfs or ssh, so nothing
/// here opens a connection.
#[test]
fn the_hosts_keymap() {
    use crate::app::StatusKind;
    let (mut app, _rx) = test_app("keymap");

    // a / A add, e / E edit — a new host has id 0, an existing one does not.
    for c in ['a', 'A'] {
        key(&mut app, KeyCode::Char(c));
        assert_eq!(app.mode, Mode::HostForm, "{c}");
        assert!(!app.form.as_ref().unwrap().is_edit(), "{c} adds");
        key(&mut app, KeyCode::Esc);
    }
    for c in ['e', 'E'] {
        key(&mut app, KeyCode::Char(c));
        assert_eq!(app.mode, Mode::HostForm, "{c}");
        assert!(app.form.as_ref().unwrap().is_edit(), "{c} edits");
        key(&mut app, KeyCode::Esc);
    }
    app.function_key(2);
    assert!(!app.form.as_ref().unwrap().is_edit(), "F2 adds");
    key(&mut app, KeyCode::Esc);
    app.function_key(4);
    assert!(app.form.as_ref().unwrap().is_edit(), "F4 edits");
    key(&mut app, KeyCode::Esc);

    // Ctrl+A still selects all rather than adding a host.
    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(app.mode, Mode::Normal, "Ctrl+A is not a");
    assert_eq!(app.marked.len(), app.hosts.len());
    key(&mut app, KeyCode::Esc);

    // The old keys do nothing now.
    app.function_key(3);
    assert_eq!(app.mode, Mode::Normal, "F3 is unbound");
    app.function_key(5);
    assert!(app.term.is_empty(), "F5 opens no shell");
    app.function_key(7);
    assert_eq!(app.mode, Mode::Normal, "F7 is the form's now");
    app.function_key(12);
    assert_eq!(app.mode, Mode::Normal, "F12 is unbound");
    key(&mut app, KeyCode::Char('i'));
    assert_eq!(app.mode, Mode::Normal, "and so is i");

    // m and u go the way they say, and only touch hosts not already there.
    assert!(!app.hosts[0].mounted);
    key(&mut app, KeyCode::Char('u'));
    assert_eq!(
        app.status.text, "Nothing mounted.",
        "u on an unmounted host"
    );
    app.hosts[0].mounted = true;
    key(&mut app, KeyCode::Char('m'));
    assert_eq!(app.status.text, "Already mounted.", "m on a mounted host");
    app.hosts[0].mounted = false;

    // F9's label names the direction it will go.
    let bar = render(&mut app, 120, 30)
        .lines()
        .last()
        .unwrap()
        .to_string();
    assert!(bar.contains("F9") && bar.contains("Mount"), "{bar}");
    assert!(!bar.contains("Unmount"), "{bar}");
    app.hosts[0].mounted = true;
    let bar = render(&mut app, 120, 30)
        .lines()
        .last()
        .unwrap()
        .to_string();
    assert!(bar.contains("F9") && bar.contains("Unmount"), "{bar}");
    app.hosts[0].mounted = false;

    // With nothing to act on, Enter reaches the shell path — which says so —
    // rather than the edit form it used to open, and F9 no longer leaves the
    // screen.
    for id in app.hosts.iter().map(|h| h.id).collect::<Vec<_>>() {
        app.db.remove(id).unwrap();
    }
    app.reload();
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal, "Enter no longer edits");
    assert_eq!(app.status.kind, StatusKind::Warn);
    assert!(
        app.status.text.contains("No host selected"),
        "{}",
        app.status.text
    );
    app.function_key(9);
    assert_eq!(
        app.screen,
        Screen::Hosts,
        "F9 is Mount here, not the screen cycle"
    );
    // Everywhere else it still cycles.
    app.screen = Screen::Chat;
    app.function_key(9);
    assert_ne!(app.screen, Screen::Chat, "F9 still cycles from Chat");
}

/// The Shell cap has no F-number to dispatch, so it carries its own code — and
/// a click on it has to reach the shell path, not fall through.
#[test]
fn the_shell_cap_is_clickable() {
    use crate::app::StatusKind;
    let (mut app, _rx) = test_app("shellcap");
    for id in app.hosts.iter().map(|h| h.id).collect::<Vec<_>>() {
        app.db.remove(id).unwrap();
    }
    app.reload();
    let _ = render(&mut app, 120, 30);
    let (x0, _, _) = *app
        .regions
        .fkeys
        .iter()
        .find(|(_, _, k)| *k == crate::app::BAR_SHELL)
        .expect("a clickable Shell cap");
    let y = app.regions.fn_bar_y;
    click(&mut app, x0 + 1, y);
    assert_eq!(app.status.kind, StatusKind::Warn);
    assert!(
        app.status.text.contains("No host selected"),
        "{}",
        app.status.text
    );
}

/// MNT is one colour in both states — the bright orange of the old `[gen]`
/// cell — and the word carries which state it is. Checked on ordinary rows: on
/// the cursor and marked rows every accent takes the row's ink instead, since
/// orange on the orange cursor bar would not be read at all.
#[test]
fn the_mount_column_is_orange_in_both_states() {
    use ratatui::style::Color;
    let (mut app, _rx) = test_app("mntcolour");
    app.hosts[0].mounted = true;
    // The cursor on the last row, so web-01 ([yes]) and db-main ([no]) are
    // drawn plain.
    app.set_cursor(2);
    // Tall enough for all three hosts: at 8 rows only two fit, and the cursor
    // on the third scrolls web-01 away.
    let mut terminal = Terminal::new(TestBackend::new(120, 14)).unwrap();
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();

    let fg_of = |needle: &str| -> Color {
        for y in 0..buf.area.height {
            let row: String = (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect();
            if let Some(i) = row.find(needle) {
                // `find` is a byte offset; count the characters before it.
                let x = row[..i].chars().count() as u16;
                return buf[(x + 1, y)].fg;
            }
        }
        panic!("{needle} not drawn");
    };
    assert_eq!(fg_of("[yes]"), crate::ui::theme::ORANGE_BRIGHT, "mounted");
    assert_eq!(
        fg_of("[no]"),
        crate::ui::theme::ORANGE_BRIGHT,
        "not mounted"
    );
}

// ---- mounting ------------------------------------------------------------

/// Stand-in for sshfs that connects forever, until cancelled.
fn hangs_until_cancelled()
-> impl Fn(&HostRecord, &std::sync::atomic::AtomicBool) -> anyhow::Result<crate::mount::Outcome>
+ Send
+ 'static {
    |_, cancel| {
        while !cancel.load(std::sync::atomic::Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        Ok(crate::mount::Outcome::Cancelled)
    }
}

fn finish_mount(app: &mut App) {
    for _ in 0..1000 {
        if app.poll_mount() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the mount never finished");
}

fn targets(app: &App, names: &[&str]) -> Vec<HostRecord> {
    names
        .iter()
        .map(|n| app.hosts.iter().find(|h| h.name == *n).unwrap().clone())
        .collect()
}

#[test]
fn a_mount_in_progress_shows_a_dialog_and_cancel_stops_it() {
    use crate::app::StatusKind;
    let (mut app, _rx) = test_app("mountdialog");
    let t = targets(&app, &["web-01"]);
    app.start_mount(t, hangs_until_cancelled());
    assert_eq!(app.mode, Mode::Mounting);
    assert!(app.animating(), "the screen keeps moving while it connects");

    let out = render(&mut app, 120, 30);
    assert!(out.contains("Mounting"), "{out}");
    assert!(out.contains("web-01 → /net/web-01"), "{out}");
    assert!(out.contains("connecting"), "{out}");
    assert!(out.contains("Cancel"), "{out}");
    assert!(out.contains('░'), "a bar: {out}");
    // The last line of the dialog is on screen, so nothing above it — the
    // Cancel button least of all — was clipped by too short a frame.
    assert!(out.contains("Esc cancel"), "{out}");
    let bar = out.lines().last().unwrap();
    assert!(bar.contains("Esc") && bar.contains("Cancel"), "{bar}");

    key(&mut app, KeyCode::Esc);
    let out = render(&mut app, 120, 30);
    assert!(out.contains("cancelling"), "it says so until it has: {out}");

    finish_mount(&mut app);
    assert_eq!(app.mode, Mode::Normal, "the dialog closes");
    assert!(app.mounting.is_none());
    assert!(!app.animating());
    assert_eq!(app.status.kind, StatusKind::Ok);
    assert_eq!(app.status.text, "Mount cancelled.");
}

/// Enter presses the only button there is, and Ctrl+C means "stop that".
#[test]
fn enter_and_ctrl_c_cancel_a_mount_too() {
    for how in ["enter", "ctrl-c"] {
        let (mut app, _rx) = test_app(&format!("mountcancel-{how}"));
        let t = targets(&app, &["web-01"]);
        app.start_mount(t, hangs_until_cancelled());
        match how {
            "enter" => key(&mut app, KeyCode::Enter),
            _ => app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        }
        finish_mount(&mut app);
        assert_eq!(app.status.text, "Mount cancelled.", "{how}");
        assert!(!app.should_quit, "{how} cancels, it does not quit");
    }
}

#[test]
fn the_cancel_button_is_orange_and_clickable() {
    use ratatui::style::Color;
    let (mut app, _rx) = test_app("mountbutton");
    let t = targets(&app, &["web-01"]);
    app.start_mount(t, hangs_until_cancelled());

    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    let (rect, _) = *app
        .regions
        .clicks
        .iter()
        .find(|(_, c)| *c == Click::Key(KeyCode::Esc))
        .expect("a Cancel button");
    let buf = terminal.backend().buffer().clone();
    let cell = &buf[(rect.x + 1, rect.y)];
    assert_eq!(cell.bg, crate::ui::theme::ORANGE, "an orange button");
    let label: String = (rect.x..rect.x + rect.width)
        .map(|x| buf[(x, rect.y)].symbol().to_string())
        .collect();
    assert_eq!(label.trim(), "Cancel");
    let _: Color = cell.fg;

    click(&mut app, rect.x + 1, rect.y);
    assert!(app.mounting.as_ref().unwrap().is_cancelling());
    finish_mount(&mut app);
    assert_eq!(app.status.text, "Mount cancelled.");
}

#[test]
fn several_hosts_show_which_one_is_connecting() {
    use std::sync::atomic::Ordering;
    let (mut app, _rx) = test_app("mountmany");
    let t = targets(&app, &["web-01", "db-main"]);
    app.start_mount(t, |h, cancel| {
        if h.name == "web-01" {
            return Ok(crate::mount::Outcome::Mounted);
        }
        while !cancel.load(Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        Ok(crate::mount::Outcome::Cancelled)
    });
    for _ in 0..1000 {
        if app.mounting.as_ref().unwrap().progress().current == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let out = render(&mut app, 120, 30);
    assert!(out.contains("host 2 of 2"), "{out}");
    assert!(out.contains("db-main → /net/db-main"), "{out}");

    key(&mut app, KeyCode::Esc);
    finish_mount(&mut app);
    // web-01 was mounted before the cancel, and the message says so.
    assert_eq!(app.status.text, "Cancelled after mounting 1 host.");
}

#[test]
fn a_finished_mount_closes_the_dialog_and_says_how_many() {
    let (mut app, _rx) = test_app("mountdone");
    let t = targets(&app, &["web-01", "db-main"]);
    app.start_mount(t, |_, _| Ok(crate::mount::Outcome::Mounted));
    finish_mount(&mut app);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.status.text, "Mounted 2 hosts.");
}

#[test]
fn a_failed_mount_closes_the_dialog_and_shows_the_error() {
    let (mut app, _rx) = test_app("mountfail");
    let t = targets(&app, &["web-01"]);
    app.start_mount(t, |_, _| {
        anyhow::bail!("mounting web-01 failed: read: Connection reset by peer")
    });
    finish_mount(&mut app);
    assert_eq!(app.mode, Mode::Normal);
    let err = app.alert.clone().expect("an error dialog");
    assert!(err.contains("Connection reset"), "{err}");
}

/// Modal: nothing behind the dialog is reachable while sshfs connects.
#[test]
fn nothing_behind_the_mount_dialog_is_reachable() {
    let (mut app, _rx) = test_app("mountmodal");
    let t = targets(&app, &["web-01"]);
    app.start_mount(t, hangs_until_cancelled());
    let _ = render(&mut app, 120, 30);

    for c in ['a', 'e', 'm', 'u'] {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Delete);
    app.function_key(8);
    app.function_key(10);
    let row = app.regions.rows;
    click(&mut app, row.x + 2, row.y + 1);
    assert_eq!(app.mode, Mode::Mounting, "still just the dialog");
    assert!(app.form.is_none() && app.pending_delete.is_empty());
    assert!(!app.should_quit, "F10 from the bar does not quit mid-mount");

    // Quitting for real stops the mount rather than leaving sshfs connecting.
    app.quit();
    assert!(app.mounting.is_none(), "the mount was stopped and joined");
}

/// The key dialog opened from the form returns to the form however it is
/// closed — including a click outside it, which the first fix missed.
#[test]
fn clicking_outside_the_key_dialog_returns_to_the_form() {
    let (mut app, _rx) = test_app("keyclickout");
    app.set_cursor(1);
    key(&mut app, KeyCode::Char('e'));
    key(&mut app, KeyCode::F(7));
    assert_eq!(app.mode, Mode::ShowKey);
    let _ = render(&mut app, 120, 34);
    click(&mut app, 0, 0);
    assert_eq!(app.mode, Mode::HostForm, "back in the form");
    assert!(app.form.as_ref().unwrap().has_key());
}

// ---- the error dialog ----------------------------------------------------

#[test]
fn an_error_is_a_dialog_with_a_dismiss_button_not_footer_text() {
    let (mut app, _rx) = test_app("errdialog");
    let before = app.status.text.clone();
    app.fail("Could not save web-01: database is locked");
    assert_eq!(app.status.text, before, "nothing in the footer");

    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let out: String = (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
                + "\n"
        })
        .collect();
    assert!(out.contains("Error"), "{out}");
    assert!(out.contains("database is locked"), "{out}");

    let (rect, _) = *app
        .regions
        .clicks
        .iter()
        .find(|(_, c)| *c == Click::Dismiss)
        .expect("a Dismiss button");
    assert_eq!(
        buf[(rect.x + 1, rect.y)].bg,
        crate::ui::theme::ORANGE,
        "the default button"
    );
    click(&mut app, rect.x + 1, rect.y);
    assert!(app.alert.is_none(), "Dismiss dismisses");
}

/// A dialog that closes on whatever key the operator happened to be typing is
/// an error nobody read, so only Esc and Enter press Dismiss.
#[test]
fn only_esc_and_enter_dismiss_an_error() {
    let (mut app, _rx) = test_app("errkeys");
    for dismiss in [KeyCode::Esc, KeyCode::Enter] {
        app.fail("boom");
        for held in [
            KeyCode::Char('x'),
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::F(2),
        ] {
            key(&mut app, held);
            assert!(app.alert.is_some(), "{held:?} does not dismiss");
        }
        assert_eq!(app.mode, Mode::Normal, "and reached nothing behind it");
        assert!(app.form.is_none(), "F2 did not open a form");
        key(&mut app, dismiss);
        assert!(app.alert.is_none(), "{dismiss:?} dismisses");
    }
}

/// The old alert let clicks straight through to whatever was under it,
/// including the buttons of a dialog drawn in the same frame.
#[test]
fn clicks_do_not_pass_through_an_error() {
    let (mut app, _rx) = test_app("errclicks");
    app.open_add();
    let _ = render(&mut app, 120, 34);
    let (cancel, _) = *app
        .regions
        .clicks
        .iter()
        .find(|(_, c)| *c == Click::FormButton(crate::app::form::FormField::Cancel))
        .expect("the form's Cancel");

    app.fail("boom");
    let _ = render(&mut app, 120, 34);
    click(&mut app, cancel.x + 1, cancel.y);
    assert_eq!(
        app.mode,
        Mode::HostForm,
        "the form's Cancel was not reached"
    );
    assert!(
        app.alert.is_some(),
        "and a click elsewhere does not dismiss"
    );
    let y = app.regions.fn_bar_y;
    click(&mut app, 2, y);
    assert!(app.alert.is_some() && app.mode == Mode::HostForm);

    key(&mut app, KeyCode::Esc);
    assert!(app.alert.is_none());
    assert_eq!(app.mode, Mode::HostForm, "dismissing returns to the form");
}

#[test]
fn a_second_error_is_added_under_the_first_and_a_repeat_is_not() {
    let (mut app, _rx) = test_app("errstack");
    app.fail("mounting web-01 failed: Connection refused");
    app.fail("the agent worker has stopped");
    app.fail("mounting web-01 failed: Connection refused");
    let err = app.alert.clone().unwrap();
    assert_eq!(
        err,
        "mounting web-01 failed: Connection refused\n\nthe agent worker has stopped"
    );
    let out = render(&mut app, 120, 30);
    assert!(
        out.contains("Connection refused") && out.contains("worker has stopped"),
        "{out}"
    );
}

/// A frame taller than the terminal is clamped and clipped from the bottom,
/// which is where Dismiss is, so the message gives way instead.
#[test]
fn a_long_error_keeps_the_dismiss_button_on_screen() {
    let (mut app, _rx) = test_app("errlong");
    let long: String = (1..=80)
        .map(|i| format!("line {i} of a very long sshfs error\n"))
        .collect();
    app.fail(long.trim_end().to_string());
    let out = render(&mut app, 80, 20);
    assert!(out.contains("Dismiss"), "the button survives: {out}");
    assert!(
        out.contains("more lines"),
        "and what was cut is counted: {out}"
    );
    assert!(out.contains("line 1 of"), "{out}");
    assert!(!out.contains("line 80 of"), "{out}");
    key(&mut app, KeyCode::Enter);
    assert!(app.alert.is_none());
}

// ---- closing the last shell ----------------------------------------------

/// A tab running `script`, opened without touching the screen — so each test
/// decides where it came from.
fn spawn_tab(app: &mut App, name: &str, script: &str) {
    use crate::term::session::Spawn;
    let mut spawn = Spawn::new("/bin/sh");
    spawn.args = vec!["-c".into(), script.into()];
    app.term
        .open_tab(
            vec![(name.to_string(), spawn)],
            (24, 80),
            50,
            "xterm",
            &app.term_tx,
        )
        .unwrap();
}

#[test]
fn closing_the_last_shell_returns_to_the_screen_before_it() {
    for from in [Screen::Hosts, Screen::Chat] {
        let (mut app, _rx) = test_app(&format!("lastshell-{from:?}"));
        app.set_screen(from);
        spawn_tab(&mut app, "web-01", "sleep 30");
        app.set_screen(Screen::Shells);
        assert_eq!(app.prev_screen, from);

        app.function_key(4); // Close
        assert!(app.term.is_empty());
        assert_eq!(app.screen, from, "back where it came from");
    }
}

#[test]
fn closing_a_shell_that_is_not_the_last_stays_on_shells() {
    let (mut app, _rx) = test_app("notlast");
    spawn_tab(&mut app, "web-01", "sleep 30");
    spawn_tab(&mut app, "db-main", "sleep 30");
    app.set_screen(Screen::Shells);
    app.function_key(4);
    assert_eq!(app.term.tab_count(), 1);
    assert_eq!(app.screen, Screen::Shells);
    app.function_key(4);
    assert_eq!(app.screen, Screen::Hosts, "the last one does");
}

#[test]
fn the_last_tabs_close_button_goes_back_too() {
    let (mut app, _rx) = test_app("lastx");
    app.set_screen(Screen::Chat);
    spawn_tab(&mut app, "web-01", "sleep 30");
    app.set_screen(Screen::Shells);
    let _ = render(&mut app, 120, 20);
    let (rect, _) = *app.regions.shell_closes.first().expect("a × on the tab");
    click(&mut app, rect.x, rect.y);
    assert!(app.term.is_empty());
    assert_eq!(app.screen, Screen::Chat);
}

/// `exit` in the shell, or a dropped connection: nothing was clicked, and the
/// event loop's reap is what notices.
#[test]
fn a_last_shell_that_ends_by_itself_goes_back() {
    let (mut app, _rx) = test_app("lastexit");
    spawn_tab(&mut app, "web-01", "exit 0");
    app.set_screen(Screen::Shells);
    for _ in 0..400 {
        app.reap_shells();
        if app.term.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(app.term.is_empty(), "the session ended and was reaped");
    assert_eq!(app.screen, Screen::Hosts);
}

/// Only the transition moves anyone. A shell ending while the operator is on
/// another screen changes nothing.
#[test]
fn nobody_is_moved_who_was_not_on_the_last_shell() {
    let (mut app, _rx) = test_app("nomove");
    spawn_tab(&mut app, "web-01", "exit 0");
    app.set_screen(Screen::Chat);
    for _ in 0..400 {
        app.reap_shells();
        if app.term.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(app.term.is_empty());
    assert_eq!(app.screen, Screen::Chat, "not on Shells, so not moved");
}

/// The screen remembered is the one before Shells, never Shells itself, and
/// every route onto Shells records it.
#[test]
fn every_route_onto_shells_remembers_where_it_came_from() {
    let (mut app, _rx) = test_app("routes");
    spawn_tab(&mut app, "web-01", "sleep 30");
    spawn_tab(&mut app, "db-main", "sleep 30");
    app.set_screen(Screen::Chat);
    // Alt+3.
    alt(&mut app, KeyCode::Char('3'));
    assert_eq!(app.screen, Screen::Shells);
    assert_eq!(app.prev_screen, Screen::Chat);
    // Shell to shell changes nothing.
    alt(&mut app, KeyCode::Char('4'));
    assert_eq!(app.prev_screen, Screen::Chat);
    // From Hosts, Alt+← wraps onto the last shell.
    app.set_screen(Screen::Hosts);
    alt(&mut app, KeyCode::Left);
    assert_eq!(app.screen, Screen::Shells);
    assert_eq!(app.prev_screen, Screen::Hosts);
    app.term.shutdown();
}

// ---- bulk import -----------------------------------------------------------

/// qhostman's format, as a terminal delivers it inside a bracketed paste: CR
/// line endings.
const QHOSTMAN: &str =
    "mydomain-s5000\r1.2.3.4\rroot\r123123\r\rmydomain-s5001\r2.3.4.5\rroot\r123123\r";

/// Bulk add the keyboard way: F2 for the Add dialog, F2 again from there.
fn open_bulk(app: &mut App) {
    key(app, KeyCode::F(2));
    assert_eq!(app.mode, Mode::HostForm);
    key(app, KeyCode::F(2));
    assert_eq!(app.mode, Mode::BulkImport);
}

fn button(app: &App, want: Click) -> ratatui::layout::Rect {
    app.regions
        .clicks
        .iter()
        .find(|(_, c)| *c == want)
        .map(|(r, _)| *r)
        .unwrap_or_else(|| panic!("no {want:?} button"))
}

/// Bulk add lives in the Add dialog — a button, a cap, and F2 — and only
/// there: an import closes the form it came from, which from Edit would take
/// unsaved changes with it.
#[test]
fn bulk_add_is_offered_in_the_add_dialog_only() {
    let (mut app, _rx) = test_app("bulk-offer");
    app.open_add();
    let out = render(&mut app, 120, 34);
    assert!(out.contains("[ Bulk add ]"), "{out}");
    assert!(
        out.lines().last().unwrap().contains("Bulk add"),
        "and on the bar: {out}"
    );
    let _ = button(
        &app,
        Click::FormButton(crate::app::form::FormField::BulkAdd),
    );
    key(&mut app, KeyCode::Esc);

    app.open_edit();
    let name = app.form.as_ref().unwrap().name.value().to_string();
    let out = render(&mut app, 120, 34);
    assert!(!out.contains("Bulk add"), "{out}");
    assert!(
        !app.regions
            .clicks
            .iter()
            .any(|(_, c)| *c == Click::FormButton(crate::app::form::FormField::BulkAdd))
    );
    key(&mut app, KeyCode::F(2));
    assert_eq!(app.mode, Mode::HostForm, "F2 in Edit does nothing");
    assert!(app.bulk.is_none());
    assert_eq!(app.form.as_ref().unwrap().name.value(), name);
}

#[test]
fn f2_and_the_bulk_add_button_both_open_bulk_import() {
    let (mut app, _rx) = test_app("bulk-open");
    app.open_add();
    key(&mut app, KeyCode::F(2));
    assert_eq!(app.mode, Mode::BulkImport, "F2");
    key(&mut app, KeyCode::Esc);

    let _ = render(&mut app, 120, 34);
    let b = button(
        &app,
        Click::FormButton(crate::app::form::FormField::BulkAdd),
    );
    click(&mut app, b.x + 1, b.y);
    assert_eq!(app.mode, Mode::BulkImport, "the button");
}

/// It was opened from the Add dialog, so leaving it goes back there — with
/// what had been typed, and the caret where it was.
#[test]
fn esc_from_bulk_import_returns_to_the_add_dialog_as_it_was_left() {
    use crate::app::form::FormField;
    let (mut app, _rx) = test_app("bulk-back");
    app.open_add();
    for c in "bastion".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Tab); // Type
    key(&mut app, KeyCode::Tab); // Address
    for c in "edge".chars() {
        key(&mut app, KeyCode::Char(c));
    }

    key(&mut app, KeyCode::F(2));
    app.on_paste(QHOSTMAN);
    key(&mut app, KeyCode::Esc);

    assert_eq!(app.mode, Mode::HostForm);
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.name.value(), "bastion");
    assert_eq!(form.addr.value(), "edge");
    assert_eq!(form.focus, FormField::Addr);
    let out = render(&mut app, 120, 34);
    assert!(out.contains("Add Host") && out.contains("bastion"), "{out}");
}

/// An import closes the Add dialog too. Left in `app.form`, the form came back
/// the next time Help closed.
#[test]
fn an_import_leaves_no_add_dialog_behind() {
    let (mut app, _rx) = test_app("bulk-done");
    open_bulk(&mut app);
    app.on_paste(QHOSTMAN);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.form.is_none());

    key(&mut app, KeyCode::F(1));
    assert_eq!(app.mode, Mode::Help);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal, "not the Add dialog");
}

/// Draw at 40 columns, where the buttons overflow their row, and check that
/// what lies under each button's hitbox is part of that button's label.
fn assert_hitboxes_on_labels(app: &mut App, labels: &[(Click, &str)]) {
    let buf = render_buf(app, 40, 24);
    let mut clipped = false;
    for (click, label) in labels {
        // A button with nothing on screen has no hitbox, which is right.
        let Some((r, _)) = app.regions.clicks.iter().find(|(_, c)| c == click) else {
            continue;
        };
        assert!(
            r.right() <= buf.area.right(),
            "{click:?}'s hitbox runs off the screen: {r:?}"
        );
        let under: String = (r.x..r.right())
            .map(|x| buf[(x, r.y)].symbol().to_string())
            .collect();
        assert!(
            label.contains(&under),
            "{click:?} is over {under:?}, not part of {label:?}"
        );
        clipped |= under.chars().count() < label.chars().count();
    }
    assert!(clipped, "at 40 columns the row should overflow");
}

/// A button row wider than its dialog is cut at its end — a `Paragraph` keeps
/// an overlong line's start — and every hitbox must follow what is drawn: on
/// its own label, clipped where the label is cut, and none where it is gone.
/// The Add dialog's third button is what first makes its row overflow.
#[test]
fn a_clipped_button_row_keeps_every_hitbox_on_its_label() {
    let (mut app, _rx) = test_app("bulk-clip");
    app.open_add();
    assert_hitboxes_on_labels(
        &mut app,
        &[
            (
                Click::FormButton(crate::app::form::FormField::BulkAdd),
                "[ Bulk add ]",
            ),
            (
                Click::FormButton(crate::app::form::FormField::Save),
                " Add Host ",
            ),
            (
                Click::FormButton(crate::app::form::FormField::Cancel),
                "[ Cancel ]",
            ),
        ],
    );

    key(&mut app, KeyCode::F(2));
    app.on_paste(QHOSTMAN);
    key(&mut app, KeyCode::Tab);
    assert_hitboxes_on_labels(
        &mut app,
        &[
            (Click::Key(KeyCode::BackTab), "[ ◂ Back ]"),
            (Click::Key(KeyCode::Enter), " Import 2 hosts "),
            (Click::Key(KeyCode::Esc), "[ Cancel ]"),
        ],
    );
}

#[test]
fn a_pasted_list_is_reviewed_then_imported_and_marked() {
    let (mut app, _rx) = test_app("bulk");
    let before = app.hosts.len();
    open_bulk(&mut app);
    app.on_paste(QHOSTMAN);
    let paste = render(&mut app, 120, 34);
    assert!(paste.contains("Bulk Import · 1 of 2"), "{paste}");
    assert!(paste.contains("2 to add"), "a live summary: {paste}");

    key(&mut app, KeyCode::Tab);
    let review = render(&mut app, 120, 34);
    assert!(review.contains("Bulk Import · 2 of 2"), "{review}");
    assert!(review.contains("mydomain-s5000") && review.contains("root@2.3.4.5"));
    assert!(!review.contains("123123"), "passwords are masked: {review}");
    assert!(review.contains("Import 2 hosts"), "{review}");
    assert_eq!(app.hosts.len(), before, "nothing is written by reviewing");

    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.bulk.is_none());
    assert_eq!(app.hosts.len(), before + 2);
    let s5000 = app
        .hosts
        .iter()
        .find(|h| h.name == "mydomain-s5000")
        .unwrap();
    assert_eq!(
        (
            s5000.proto.as_str(),
            s5000.addr.as_str(),
            s5000.port,
            s5000.login.as_str(),
            s5000.pass.as_str(),
            s5000.mount_point.as_str()
        ),
        (
            "ssh",
            "1.2.3.4",
            22,
            "root",
            "123123",
            "/net/mydomain-s5000"
        )
    );
    let imported: std::collections::HashSet<i64> = app
        .hosts
        .iter()
        .filter(|h| h.name.starts_with("mydomain-"))
        .map(|h| h.id)
        .collect();
    assert_eq!(
        app.marked, imported,
        "exactly the imported hosts are marked"
    );
    assert_eq!(
        app.host().unwrap().id,
        s5000.id,
        "the cursor is on the first"
    );
    assert!(
        app.status.text.contains("Imported 2 hosts"),
        "{}",
        app.status.text
    );
}

#[test]
fn pasting_the_same_list_again_adds_nothing() {
    let (mut app, _rx) = test_app("bulk-again");
    open_bulk(&mut app);
    app.on_paste(QHOSTMAN);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Enter);
    let after = app.hosts.len();

    open_bulk(&mut app);
    app.on_paste(QHOSTMAN);
    key(&mut app, KeyCode::Tab);
    let out = render(&mut app, 120, 34);
    assert!(out.contains("already here — skipped"), "{out}");
    assert!(out.contains("Nothing to import"), "{out}");
    assert!(
        !app.regions
            .clicks
            .iter()
            .any(|(_, c)| *c == Click::Key(KeyCode::Enter)),
        "a dead Import button has no hitbox"
    );
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::BulkImport, "nothing to do, so it stays");
    assert_eq!(app.hosts.len(), after);
    assert!(app.status.text.contains("Nothing to import"));
}

#[test]
fn a_malformed_block_stops_the_whole_import() {
    let (mut app, _rx) = test_app("bulk-bad");
    let before = app.hosts.len();
    open_bulk(&mut app);
    app.on_paste(&QHOSTMAN.replace("2.3.4.5\r", "2.3.4.5\r2222\r"));
    key(&mut app, KeyCode::Tab);
    let out = render(&mut app, 120, 34);
    assert!(out.contains("line 6"), "{out}");
    assert!(out.contains("5 lines — expected 4"), "{out}");
    assert!(out.contains("Fix 1 problem before importing"), "{out}");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.hosts.len(), before, "not even the good block");
    assert!(
        app.status.text.contains("Fix 1 problem"),
        "{}",
        app.status.text
    );

    // Back to the text, which is where the fix goes, with the text intact.
    key(&mut app, KeyCode::BackTab);
    let b = app.bulk.as_ref().unwrap();
    assert_eq!(b.step, crate::app::bulk::BulkStep::Paste);
    assert!(b.joined().contains("mydomain-s5000"));
}

#[test]
/// Esc from either step writes nothing, and goes back to the Add dialog it was
/// opened from; a second Esc leaves that as it always did.
fn esc_from_either_step_writes_nothing() {
    let (mut app, _rx) = test_app("bulk-esc");
    let before = app.hosts.len();
    for tabs in [0, 1] {
        open_bulk(&mut app);
        app.on_paste(QHOSTMAN);
        for _ in 0..tabs {
            key(&mut app, KeyCode::Tab);
        }
        key(&mut app, KeyCode::Esc);
        assert_eq!(app.mode, Mode::HostForm, "step {tabs}");
        assert!(app.bulk.is_none());
        assert_eq!(app.hosts.len(), before);

        key(&mut app, KeyCode::Esc);
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.form.is_none());
    }
}

/// Without bracketed paste a paste arrives as keys, with an Enter for every
/// line break — so in this dialog Enter is a newline and never a submit.
#[test]
fn a_list_arriving_as_keystrokes_is_a_list_too() {
    let (mut app, _rx) = test_app("bulk-keys");
    open_bulk(&mut app);
    for line in ["mydomain-s5000", "1.2.3.4", "root", "123123"] {
        for c in line.chars() {
            key(&mut app, KeyCode::Char(c));
        }
        key(&mut app, KeyCode::Enter);
    }
    assert_eq!(app.mode, Mode::BulkImport);
    assert_eq!(
        app.bulk.as_ref().unwrap().step,
        crate::app::bulk::BulkStep::Paste
    );
    key(&mut app, KeyCode::Tab);
    let out = render(&mut app, 120, 34);
    assert!(out.contains("Import 1 host "), "{out}");
}

#[test]
fn the_bulk_buttons_are_clickable() {
    let (mut app, _rx) = test_app("bulk-click");
    let before = app.hosts.len();
    open_bulk(&mut app);
    app.on_paste(QHOSTMAN);
    let _ = render(&mut app, 120, 34);
    let next = button(&app, Click::Key(KeyCode::Tab));
    click(&mut app, next.x + 1, next.y);
    assert_eq!(
        app.bulk.as_ref().unwrap().step,
        crate::app::bulk::BulkStep::Review
    );

    let _ = render(&mut app, 120, 34);
    let back = button(&app, Click::Key(KeyCode::BackTab));
    click(&mut app, back.x + 1, back.y);
    assert_eq!(
        app.bulk.as_ref().unwrap().step,
        crate::app::bulk::BulkStep::Paste
    );

    key(&mut app, KeyCode::Tab);
    let _ = render(&mut app, 120, 34);
    let import = button(&app, Click::Key(KeyCode::Enter));
    click(&mut app, import.x + 1, import.y);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.hosts.len(), before + 2);
}

#[test]
fn a_long_review_scrolls() {
    let (mut app, _rx) = test_app("bulk-long");
    open_bulk(&mut app);
    let list: String = (0..60)
        .map(|i| format!("bulk-{i:02}\n10.1.0.{i}\nroot\npw\n\n"))
        .collect();
    app.on_paste(&list);
    key(&mut app, KeyCode::Tab);
    let out = render(&mut app, 120, 34);
    assert!(out.contains("bulk-00") && !out.contains("bulk-59"), "{out}");
    assert!(out.contains("PgDn ↓"), "the hint says there is more: {out}");
    assert!(out.contains("Import 60 hosts"), "{out}");

    key(&mut app, KeyCode::End);
    let out = render(&mut app, 120, 34);
    assert!(out.contains("bulk-59") && !out.contains("bulk-00"), "{out}");
    wheel(&mut app, false);
    assert!(app.bulk.as_ref().unwrap().scroll < app.bulk.as_ref().unwrap().max_scroll);

    // Small terminals keep the buttons on screen.
    for (w, h) in [(80, 24), (60, 16), (30, 8)] {
        let _ = render(&mut app, w, h);
    }
}

// ---- the host filter ---------------------------------------------------------

fn ctrl(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
}

fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        key(app, KeyCode::Char(c));
    }
}

/// Filter the Hosts screen to `needle`, the way the operator does.
fn filter_to(app: &mut App, needle: &str) {
    ctrl(app, 'f');
    assert_eq!(app.mode, Mode::Filter);
    app.filter_edit = Some(tui_input::Input::new(needle.to_string()));
    key(app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
}

fn listed(app: &App) -> Vec<String> {
    app.visible().map(|h| h.name.clone()).collect()
}

/// Ctrl+F, a needle, Enter: only hosts whose name or address contains it, in
/// any case — FTP hosts too, on the screen — and the frame says what it shows.
#[test]
fn ctrl_f_filters_by_name_or_address_in_any_case() {
    let (mut app, _rx) = test_app("filter");
    ctrl(&mut app, 'f');
    assert!(render(&mut app, 120, 30).contains("Filter Hosts"));
    type_str(&mut app, "WEB");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.filter.as_deref(), Some("WEB"));
    assert_eq!(listed(&app), ["web-01"]);
    let out = render(&mut app, 120, 30);
    assert!(out.contains("Known Hosts · WEB"), "{out}");
    assert!(out.contains(" 1 of 3 hosts "), "{out}");
    assert!(!out.contains("db-main"), "{out}");

    filter_to(&mut app, "10.0.8");
    assert_eq!(listed(&app), ["db-main"], "by address");
    filter_to(&mut app, "192.168");
    assert_eq!(listed(&app), ["nas"], "an FTP host is listed too");
}

/// The dialog holds the filter there is; cancelling — Esc or Ctrl+C — leaves
/// it as it was; an empty needle shows every host.
#[test]
fn the_filter_dialog_is_prefilled_and_cancelling_keeps_the_filter() {
    let (mut app, _rx) = test_app("filter-cancel");
    filter_to(&mut app, "web");
    ctrl(&mut app, 'f');
    assert_eq!(app.filter_edit.as_ref().unwrap().value(), "web");
    type_str(&mut app, "zzz");
    key(&mut app, KeyCode::Esc);
    assert_eq!(
        (app.mode, app.filter.as_deref()),
        (Mode::Normal, Some("web"))
    );

    ctrl(&mut app, 'f');
    ctrl(&mut app, 'c');
    assert_eq!(
        (app.mode, app.filter.as_deref()),
        (Mode::Normal, Some("web"))
    );

    filter_to(&mut app, "   ");
    assert_eq!(app.filter, None, "blank shows all");
    assert_eq!(listed(&app).len(), 3);
}

/// Each Esc undoes one thing: the filter first, then the marks.
#[test]
fn esc_clears_the_filter_before_the_marks() {
    let (mut app, _rx) = test_app("filter-esc");
    filter_to(&mut app, "web");
    key(&mut app, KeyCode::Char(' '));
    assert_eq!(app.marked.len(), 1);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.filter, None);
    assert_eq!(app.marked.len(), 1, "the marks stay");
    key(&mut app, KeyCode::Esc);
    assert!(app.marked.is_empty());
}

/// The cursor lives in the listed hosts: Home, End, arrows and a click move
/// over them, and the hover hint describes the host on that row.
#[test]
fn the_cursor_moves_within_the_filtered_list() {
    let mut app = many_hosts("filter-cursor");
    filter_to(&mut app, "h1");
    assert_eq!(listed(&app).len(), 10, "h10 to h19");
    key(&mut app, KeyCode::End);
    assert_eq!(app.host().unwrap().name, "h19");
    key(&mut app, KeyCode::Home);
    assert_eq!(app.host().unwrap().name, "h10");
    key(&mut app, KeyCode::Down);
    assert_eq!(app.host().unwrap().name, "h11");
    key(&mut app, KeyCode::PageDown);
    assert_eq!(app.host().unwrap().name, "h19", "clamped to the list");

    let _ = render(&mut app, 120, 20);
    let rows = app.regions.rows;
    click(&mut app, rows.x + 2, rows.y + 2);
    assert_eq!(app.host().unwrap().name, "h12");
    moved(&mut app, rows.x + 2, rows.y + 3);
    let hint = app.hover_hint().unwrap();
    assert!(hint.contains("10.0.0.13"), "{hint}");
}

/// Nothing reaches a host the filter hides: applying one drops its marks,
/// Ctrl+A and `*` act on the listed hosts, and what Enter, mount and F8 act on
/// is listed — even should a mark on a hidden host slip through.
#[test]
fn marks_and_actions_stay_inside_the_filter() {
    let (mut app, _rx) = test_app("filter-marks");
    ctrl(&mut app, 'a');
    assert_eq!(app.marked.len(), 3);
    filter_to(&mut app, "web");
    let web = app.hosts[0].id;
    assert_eq!(
        app.marked,
        [web].into_iter().collect(),
        "hidden marks dropped"
    );

    key(&mut app, KeyCode::Char('*'));
    assert!(app.marked.is_empty(), "inverted within the list");
    ctrl(&mut app, 'a');
    assert_eq!(
        app.marked,
        [web].into_iter().collect(),
        "all the listed ones"
    );

    app.marked.insert(app.hosts[1].id);
    let targets: Vec<String> = app.targets().into_iter().map(|h| h.name).collect();
    assert_eq!(targets, ["web-01"]);
    app.function_key(8);
    assert_eq!(
        app.pending_delete,
        vec![web],
        "F8 deletes only what is shown"
    );
}

/// A host deleted above the cursor does not move it to another host.
#[test]
fn reload_keeps_the_cursor_on_its_host() {
    let (mut app, _rx) = test_app("filter-reload");
    key(&mut app, KeyCode::End);
    assert_eq!(app.host().unwrap().name, "nas");
    app.db.remove(app.hosts[0].id).unwrap();
    app.reload();
    assert_eq!(app.host().unwrap().name, "nas");
}

/// A filter that matches nothing says so, and there is nothing to act on.
#[test]
fn a_filter_matching_nothing_says_so_and_nothing_acts() {
    let (mut app, _rx) = test_app("filter-none");
    filter_to(&mut app, "zzz");
    let out = render(&mut app, 120, 30);
    assert!(out.contains("No hosts match the filter"), "{out}");
    assert!(app.host().is_none());
    assert!(app.targets().is_empty());
    key(&mut app, KeyCode::Enter);
    assert!(app.term.is_empty(), "no shell opened");
    app.function_key(8);
    assert_eq!(app.mode, Mode::Normal, "nothing to delete");
}

/// Only Ctrl+F and Esc change the filter — it is the agent's scope too. A
/// host saved or imported that it would hide is said so, and not marked.
#[test]
fn a_saved_or_imported_host_the_filter_hides_leaves_the_filter_on() {
    let (mut app, _rx) = test_app("filter-save");
    filter_to(&mut app, "web");
    app.open_add();
    type_str(&mut app, "bastion");
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Tab);
    type_str(&mut app, "10.9.9.9");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.filter.as_deref(), Some("web"));
    assert!(
        app.status.text.contains("hidden by the filter"),
        "{}",
        app.status.text
    );
    assert_eq!(listed(&app), ["web-01"]);

    key(&mut app, KeyCode::F(2));
    key(&mut app, KeyCode::F(2));
    app.on_paste(QHOSTMAN);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.filter.as_deref(), Some("web"));
    assert!(app.marked.is_empty(), "nothing hidden is marked");
    assert!(
        app.status.text.contains("the filter hides 2"),
        "{}",
        app.status.text
    );
}

/// The dialog takes a paste as one line, its Filter button applies, and a
/// click outside it cancels.
#[test]
fn the_filter_dialog_takes_a_paste_its_button_and_a_click_outside() {
    let (mut app, _rx) = test_app("filter-dialog");
    ctrl(&mut app, 'f');
    app.on_paste("db\r\n");
    assert_eq!(app.filter_edit.as_ref().unwrap().value(), "db");
    let _ = render(&mut app, 120, 30);
    let b = button(&app, Click::Key(KeyCode::Enter));
    click(&mut app, b.x + 1, b.y);
    assert_eq!(app.filter.as_deref(), Some("db"));

    ctrl(&mut app, 'f');
    let _ = render(&mut app, 120, 30);
    click(&mut app, 0, 0);
    assert_eq!(
        (app.mode, app.filter.as_deref()),
        (Mode::Normal, Some("db"))
    );
}

/// The agent is given the SSH hosts the filter lists, is told it is filtered,
/// and keeps the proxy though the filter hides the proxy host.
#[test]
fn the_agent_scope_is_the_filtered_ssh_hosts_and_the_proxy_survives() {
    let (mut app, _rx) = test_app("filter-scope");
    app.hosts[1].proxy = true;
    filter_to(&mut app, "web");
    let scope = app.agent_scope();
    let names: Vec<&str> = scope.hosts.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["web-01"]);
    assert!(scope.filtered);
    assert_eq!(scope.proxy.map(|h| h.name), Some("db-main".to_string()));

    filter_to(&mut app, "nas");
    assert!(
        app.agent_scope().hosts.is_empty(),
        "an FTP host is no agent host"
    );
}

/// What the filter is never reaches the model — it can hold part of an
/// address — through any answer a filtered scope gives.
#[test]
fn the_needle_never_reaches_the_agent() {
    use crate::agent::tools::{EDIT_HOST, LIST_HOSTS, PROPOSE_PLAN, ToolCtx, dispatch};
    use std::sync::atomic::AtomicBool;
    let (mut app, _rx) = test_app("filter-needle");
    filter_to(&mut app, "10.0.4");
    let scope = app.agent_scope();
    let cancel = AtomicBool::new(false);
    let ctx = ToolCtx {
        hosts: &scope.hosts,
        filtered: scope.filtered,
        proxy: scope.proxy.as_ref(),
        datadir: std::path::Path::new("/tmp"),
        cfg: &app.cfg,
        cancel: &cancel,
        next_plan_id: 1,
        written_this_turn: &[],
    };
    let answers = [
        dispatch(&ctx, LIST_HOSTS, "{}").text().to_string(),
        dispatch(
            &ctx,
            "readonly_read_file",
            r#"{"host":"db-main","paths":["/etc/hostname"]}"#,
        )
        .text()
        .to_string(),
        dispatch(
            &ctx,
            PROPOSE_PLAN,
            r#"{"steps":[{"kind":"scriptlet","script":"id","hosts":["db-main"]}]}"#,
        )
        .text()
        .to_string(),
        dispatch(&ctx, EDIT_HOST, r#"{"host":"db-main","port":2222}"#)
            .text()
            .to_string(),
    ];
    for a in &answers {
        assert!(!a.contains("10.0.4"), "the needle leaked: {a}");
        assert!(a.contains("filter"), "and it is said to be filtered: {a}");
    }
}

/// A write the agent asks for that reaches past the filter — a hidden host
/// edited, a hidden host's name taken — is refused, to the operator.
#[test]
fn an_agent_write_outside_the_filter_is_refused_to_the_operator() {
    use crate::agent::AgentEvent;
    use crate::agent::hosts::{HostFields, HostWrite};
    let (mut app, _rx) = test_app("filter-write");
    filter_to(&mut app, "web");
    let fields = |name: Option<&str>, port: Option<i64>| HostFields {
        name: name.map(str::to_string),
        port,
        addr: Some("10.1.1.1".into()),
        ..Default::default()
    };
    let write =
        |app: &mut App, w: HostWrite| app.on_agent_event(AgentEvent::HostWrite(Box::new(w)));

    write(
        &mut app,
        HostWrite::Edit {
            name: "db-main".into(),
            fields: fields(None, Some(2222)),
        },
    );
    assert!(
        app.alert.as_deref().unwrap_or("").contains("hides"),
        "{:?}",
        app.alert
    );
    assert_eq!(app.hosts[1].port, 22, "not changed");
    app.alert = None;

    write(&mut app, HostWrite::Create(fields(Some("DB-MAIN"), None)));
    assert!(
        app.alert
            .as_deref()
            .unwrap_or("")
            .contains("already exists"),
        "{:?}",
        app.alert
    );
    assert_eq!(app.hosts.len(), 3, "not added");
    app.alert = None;

    write(
        &mut app,
        HostWrite::Edit {
            name: "web-01".into(),
            fields: fields(Some("nas"), None),
        },
    );
    assert!(
        app.alert
            .as_deref()
            .unwrap_or("")
            .contains("already exists"),
        "{:?}",
        app.alert
    );
    app.alert = None;

    // A listed host, its name kept: fine.
    write(
        &mut app,
        HostWrite::Edit {
            name: "web-01".into(),
            fields: HostFields {
                port: Some(2200),
                ..Default::default()
            },
        },
    );
    assert!(app.alert.is_none(), "{:?}", app.alert);
    assert_eq!(app.hosts[0].port, 2200);
}

/// A plan proposed before the filter changed names hosts it now hides: the
/// dialog says so, and leaves whether they run to the operator.
#[test]
fn the_plan_dialog_marks_hosts_the_filter_hides() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("filter-plan");
    filter_to(&mut app, "web");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("id", vec![1, 2]))));
    app.function_key(2);
    let out = render(&mut app, 120, 34);
    assert!(out.contains("db-main · hidden by filter"), "{out}");
    assert!(!out.contains("web-01 · hidden"), "{out}");
}

/// The delete dialog describes the host it deletes by id: another host of the
/// same name used to lend it its login and address.
#[test]
fn the_delete_dialog_describes_the_host_by_id() {
    let (mut app, _rx) = test_app("filter-delete");
    for addr in ["10.5.5.1", "10.5.5.2"] {
        app.db
            .save(&HostRecord {
                name: "dup".into(),
                proto: "ssh".into(),
                addr: addr.into(),
                port: 22,
                login: "root".into(),
                ..Default::default()
            })
            .unwrap();
    }
    app.reload();
    key(&mut app, KeyCode::End);
    app.function_key(8);
    let out = render(&mut app, 120, 30);
    assert!(out.contains("root@10.5.5.2:22"), "{out}");
}

/// The chat screen counts what the agent is given: SSH hosts, filtered.
#[test]
fn the_chat_label_counts_the_agents_hosts() {
    let (mut app, _rx) = test_app("filter-chat");
    app.cfg.agent.model = "m".into();
    app.screen = Screen::Chat;
    let out = render(&mut app, 120, 30);
    assert!(out.contains("2 hosts in context "), "{out}");
    filter_to_on_hosts(&mut app, "web");
    let out = render(&mut app, 120, 30);
    assert!(out.contains("1 hosts in context · filtered"), "{out}");
}

fn filter_to_on_hosts(app: &mut App, needle: &str) {
    let back = app.screen;
    app.screen = Screen::Hosts;
    filter_to(app, needle);
    app.screen = back;
}
