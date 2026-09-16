//! Headless render smoke tests and synthetic mouse/hover tests.
//!
//! Follows cfdns's two patterns (`/root/cfdns/src/render_tests.rs` and the
//! mouse unit tests at `/root/cfdns/src/app/mod.rs:1318-1431`): draw every mode
//! over a `TestBackend`, and drive `on_mouse` with hand-set `regions` so no
//! terminal is needed.

use crate::app::{App, Click, Mode, Screen};
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
    for gone in ["F3", "F5", "F7", "GenKey"] {
        assert!(!bar.contains(gone), "{gone} should be gone: {bar}");
    }
    // Passwords are masked; an empty one shows a dash.
    assert!(!out.contains("hunter2"));
    assert!(out.contains("•"));
    // KEY, then PRX, then MNT last.
    let header = out.lines().find(|l| l.contains("NAME")).unwrap();
    let (k, p, m) = (
        header.find("KEY").unwrap(),
        header.find("PRX").unwrap(),
        header.find("MNT").unwrap(),
    );
    assert!(k < p && p < m, "column order: {header}");
    // MNT spells it out; KEY is a status circle, not a button — making a key is
    // the edit form's business.
    let web = out.lines().find(|l| l.contains("web-01")).unwrap();
    assert!(web.contains("[no]"), "{web}");
    assert!(!out.contains("[gen]"), "{out}");
    // The last four cells are KEY, PRX, MNT and the panel border.
    let cells: Vec<&str> = web.split_whitespace().collect();
    let tail = &cells[cells.len() - 4..];
    assert!(tail[0] == "○" || tail[0] == "●", "KEY is a circle: {web}");
    assert_eq!(tail[2], "[no]", "MNT is last: {web}");
    assert_eq!(tail[3], "│", "then the border: {web}");
}

#[test]
fn every_dialog_renders() {
    let (mut app, _rx) = test_app("dialogs");
    for (mode, needle) in [
        (Mode::Help, "Key Bindings"),
        (Mode::HostForm, "Add Host"),
        (Mode::ConfirmDelete, "Confirm Delete"),
    ] {
        let a = &mut app;
        a.mode = Mode::Normal;
        match mode {
            Mode::HostForm => a.open_add(),
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

#[test]
fn shells_screen_starts_with_an_empty_state_and_no_footer() {
    let (mut app, _rx) = test_app("shells");
    app.screen = Screen::Shells;
    let out = render(&mut app, 120, 30);
    assert!(out.contains("No open shells."), "{out}");
    // The Shells screen draws no function bar at all.
    assert!(!out.contains("Help"), "no function bar here: {out}");
    assert!(!out.contains("Esc-"), "and no stale chord captions: {out}");
    assert!(
        app.regions.fkeys.is_empty(),
        "no F-key hitboxes are registered"
    );
    // The header tabs are still there — they are the way back out.
    assert!(app.regions.screen_tabs.len() == 3);
    // With nothing open there are no shell tabs to name, so the brand keeps
    // the space it would otherwise yield.
    assert!(
        out.contains("OpenAdmin"),
        "the brand shows on an empty Shells screen: {out}"
    );
}

/// With no footer and no status line, the header's screen tabs are the whole
/// escape route from a focused pane, so they must stay registered.
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

    assert_eq!(app.regions.screen_tabs.len(), 3, "{out}");
    // The brand steps aside for the shell tabs on this screen.
    assert!(
        !out.contains("OpenAdmin"),
        "the wordmark is hidden here: {out}"
    );
    // The shell tab shares the header row with them.
    assert!(out.lines().next().unwrap().contains("local"), "{out}");
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
    assert!(
        out.contains("switch screens"),
        "the mouse way out is documented: {out}"
    );
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
    assert_eq!(app.term.session(id).unwrap().size(), (H - 1, 100));

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

/// The brand yields the header only while there are tabs to put there, and
/// takes the space back when the last shell closes.
#[test]
fn the_brand_returns_when_the_last_shell_closes() {
    let (mut app, _rx) = test_app("brandback");
    open_shells(&mut app, &["web-01"]);

    let out = render(&mut app, 120, 20);
    assert!(!out.contains("OpenAdmin"), "the tab takes the space: {out}");
    assert!(out.contains("web-01"), "{out}");

    app.term.close_active_tab();
    let out = render(&mut app, 120, 20);
    assert!(out.contains("OpenAdmin"), "the brand comes back: {out}");
    assert!(!out.contains("web-01"), "{out}");
    // And the way out is registered either way.
    assert_eq!(app.regions.screen_tabs.len(), 3);
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

    assert_eq!(
        app.regions.shell_tabs.len(),
        3,
        "all three tabs are registered"
    );
    let header = render(&mut app, 120, 20);
    let row0 = header.lines().next().unwrap().chars().collect::<Vec<_>>();

    for (rect, i) in app.regions.shell_tabs.clone() {
        let name = ["alpha", "bravo", "charlie"][i];
        let drawn: String = row0[rect.x as usize..(rect.x + rect.width) as usize]
            .iter()
            .collect();
        assert!(
            drawn.contains(name),
            "tab {i} hitbox {rect:?} covers {drawn:?}, not {name}"
        );
    }

    // And clicking the third one selects the third one.
    let (rect, _) = app.regions.shell_tabs[2];
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
            app.regions.screen_tabs.len(),
            3,
            "width {w}: the way out must survive"
        );
        // Shell tabs never overlap the screen tabs.
        for (srect, _) in &app.regions.screen_tabs {
            for (trect, i) in &app.regions.shell_tabs {
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
            app.regions.shell_tabs.iter().any(|(_, t)| *t == i),
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
    app.screen = Screen::Shells;
    for w in [120u16, 80, 60, 44, 36, 30] {
        let _ = render(&mut app, w, 20);
        assert_eq!(
            app.regions.screen_tabs.len(),
            3,
            "width {w}: every screen must stay clickable"
        );
        for (rect, _) in &app.regions.screen_tabs {
            assert!(
                rect.x + rect.width <= w,
                "width {w}: tab hitbox {rect:?} runs off the screen"
            );
        }
    }
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
            app.regions.screen_tabs.len(),
            3,
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
        .find(|(_, s)| *s == Screen::Chat)
        .unwrap();
    assert_eq!(screen, Screen::Chat);
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
        .find(|(_, c)| *c == Click::GenKey)
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
        .find(|(_, c)| *c == Click::Key(KeyCode::Esc))
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

#[test]
fn alt_digits_switch_screens_away_from_the_terminal() {
    let (mut app, _rx) = test_app("altdigit");
    app.on_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Chat);
    app.on_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Shells);
}

/// Alt+←/→ walk the screens, wrapping in both directions.
#[test]
fn alt_arrows_walk_the_screens() {
    let (mut app, _rx) = test_app("altarrows");
    assert_eq!(app.screen, Screen::Hosts);

    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Shells);
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Chat);
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Hosts, "wraps forward");

    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Chat, "wraps backward");
    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Shells);
}

/// The single exception to the Shells screen's keyboard transparency: it is
/// the only key that gets you out of a live terminal without a mouse.
#[test]
fn alt_arrows_escape_a_focused_pane() {
    use crate::term::session::Spawn;
    let (mut app, _rx) = test_app("altescape");
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

    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Hosts, "Alt+← leaves a focused pane");

    app.screen = Screen::Shells;
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Chat, "Alt+→ too");

    // Everything else still belongs to the terminal.
    app.screen = Screen::Shells;
    for code in [KeyCode::Up, KeyCode::Down, KeyCode::Tab, KeyCode::Esc] {
        app.on_key(KeyEvent::new(code, KeyModifiers::ALT));
        assert_eq!(app.screen, Screen::Shells, "{code:?} must reach the pty");
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

/// The rule for the Shells screen: a focused pane takes every key. Nothing —
/// no F-key, no Esc+digit, no Alt chord — is reserved by the app, because mc
/// reads Esc+digit as its own F-key emulation and Alt as its menu shortcuts.
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

    // Alt+digit, which is how a quickly typed Esc+digit arrives.
    for d in '0'..='9' {
        app.on_key(KeyEvent::new(KeyCode::Char(d), KeyModifiers::ALT));
        assert!(!app.should_quit, "Alt+{d} must not quit");
        assert_eq!(app.mode, Mode::Normal, "Alt+{d} must not open a dialog");
        assert_eq!(app.screen, Screen::Shells, "Alt+{d} must not change screen");
    }

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
        .find(|(_, s)| *s == Screen::Hosts)
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
    assert!(out.contains("more"), "the hint says there is more: {out}");
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
    // 12 steps across 2 hosts is 24 host-runs, and the button says so.
    assert!(out.contains("Run 12 step(s) on 24 host(s)"), "{out}");
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
    assert!(
        !app.plan.as_ref().unwrap().armed,
        "and it is disarmed, having opened on its own"
    );
    let out = render(&mut app, 120, 30);
    assert!(out.contains("Confirm Plan"), "{out}");
    assert!(out.contains("↵ twice to run"), "which it says: {out}");
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

/// The hazard that kept this feature out to begin with: a dialog landing under
/// the fingers, and the keystroke already in flight running scripts on a fleet.
#[test]
fn an_auto_opened_dialog_absorbs_the_first_enter() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("autoopen-arm");
    app.screen = Screen::Chat;
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("ls", vec![1]))));
    tick(&mut app);

    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::ConfirmPlan, "still up, nothing ran");
    assert!(app.pending_plan.is_some(), "and still unanswered");

    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal, "the second one is deliberate");
    assert!(app.pending_plan.is_none());
}

/// Opening it yourself is already the deliberate act; making that ↵ twice
/// would be friction with nothing to prevent.
#[test]
fn a_dialog_the_operator_opened_runs_on_the_first_enter() {
    use crate::agent::AgentEvent;
    let (mut app, _rx) = test_app("openarm");
    app.screen = Screen::Shells; // no auto-open here
    app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("ls", vec![1]))));
    app.screen = Screen::Chat;
    key(&mut app, KeyCode::F(2));
    assert!(app.plan.as_ref().unwrap().armed);

    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal, "it ran");
}

/// Anything that proves a person is looking at the dialog arms it — including
/// the pointer arriving, which is what lets the Run button work first click.
#[test]
fn touching_an_auto_opened_dialog_arms_it() {
    use crate::agent::AgentEvent;
    for touch in 0..2 {
        let (mut app, _rx) = test_app(&format!("autoopen-touch{touch}"));
        app.screen = Screen::Chat;
        app.on_agent_event(AgentEvent::Proposed(Box::new(a_plan("ls", vec![1]))));
        tick(&mut app);
        assert!(!app.plan.as_ref().unwrap().armed);

        if touch == 0 {
            key(&mut app, KeyCode::Down);
        } else {
            app.on_mouse(MouseEvent {
                kind: MouseEventKind::Moved,
                column: 40,
                row: 12,
                modifiers: KeyModifiers::empty(),
            });
        }
        assert!(app.plan.as_ref().unwrap().armed, "touch {touch}");
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.mode, Mode::Normal, "touch {touch}: it ran");
    }
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
    assert!(
        sel.armed,
        "and asking for it is deliberate enough to run it"
    );
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
    let seen: Vec<String> = app.agent_hosts().into_iter().map(|h| h.name).collect();
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
/// old F3/F5/F7 unbound. Every assertion goes through a path that cannot
/// reach sshfs or ssh, so nothing here opens a connection.
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
        .find(|(_, c)| *c == Click::Key(KeyCode::Esc))
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
/// another screen changes nothing, and an empty Shells screen visited on
/// purpose stays where it is.
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

    app.set_screen(Screen::Shells);
    app.reap_shells();
    app.function_key(4); // Close, with nothing to close
    assert_eq!(
        app.screen,
        Screen::Shells,
        "visited empty on purpose, stays"
    );
}

/// The screen remembered is the one before Shells, never Shells itself, and
/// every route onto Shells records it.
#[test]
fn every_route_onto_shells_remembers_where_it_came_from() {
    let (mut app, _rx) = test_app("routes");
    app.set_screen(Screen::Chat);
    // Alt+2.
    app.on_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Shells);
    assert_eq!(app.prev_screen, Screen::Chat);
    // Shells to Shells changes nothing.
    app.set_screen(Screen::Shells);
    assert_eq!(app.prev_screen, Screen::Chat);
    // Alt+← back to Hosts, then Alt+→ onto Shells.
    app.set_screen(Screen::Hosts);
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
    assert_eq!(app.screen, Screen::Shells);
    assert_eq!(app.prev_screen, Screen::Hosts);
}
