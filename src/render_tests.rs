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
    (App::new(db, Config::default(), dir, tx), rx)
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
    assert!(out.contains("F5") && out.contains("Shell"));
    // Passwords are masked; an empty one shows a dash.
    assert!(!out.contains("hunter2"));
    assert!(out.contains("•"));
    assert!(
        out.contains("[gen]"),
        "hosts without a key offer to generate one"
    );
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
    assert!(out.contains("claude-sonnet-4.5"));
    assert!(out.contains("»"), "the composer prompt is missing");
    assert!(out.contains("ssh"), "tool calls render");
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
    app.function_key(3);
    app.function_key(8);
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
    assert!(
        opened || app.status.text.contains("Could not open a shell"),
        "double-click did not reach open_shell; status was {:?}",
        app.status.text
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
fn clicking_the_gen_cell_generates_a_key_for_that_row() {
    let (mut app, _rx) = test_app("genclick");
    let _ = render(&mut app, 120, 30);
    let (rect, idx) = *app.regions.genkeys.get(1).expect("a [gen] cell for row 1");
    assert_eq!(idx, 1);
    click(&mut app, rect.x + 1, rect.y);
    assert_eq!(app.mode, Mode::ShowKey);
    let (host, public) = app.key_dialog.clone().unwrap();
    assert_eq!(host, "db-main");
    assert!(public.starts_with("ssh-ed25519 "));
    // The row now reports an installed key.
    assert!(!app.hosts[1].key_name.is_empty());
    let out = render(&mut app, 120, 34);
    assert!(out.contains("authorized_keys"), "{out}");
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
    use crate::app::StatusKind;
    let (mut app, _rx) = test_app("invalid");
    app.open_add();
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::HostForm, "the dialog stays open");
    assert_eq!(app.status.kind, StatusKind::Err);
    assert!(app.status.text.contains("Host name is required"));
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
    app.function_key(3); // F3 Edit on web-01
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
fn chat_composer_accepts_typing_and_records_the_turn() {
    use crate::app::StatusKind;
    let (mut app, _rx) = test_app("compose");
    app.screen = Screen::Chat;
    let before = app.chat.turns.len();
    for c in "restart api".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    assert_eq!(app.chat.draft, "restart api");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.chat.turns.len(), before + 1);
    assert_eq!(
        app.status.kind,
        StatusKind::Warn,
        "the stub says so plainly"
    );
    assert!(app.status.text.contains("not wired up"));
}

#[test]
fn paste_routes_to_whatever_is_focused() {
    let (mut app, _rx) = test_app("paste");
    app.screen = Screen::Chat;
    app.on_paste("hello");
    assert_eq!(app.chat.draft, "hello");

    app.screen = Screen::Hosts;
    app.open_add();
    app.on_paste("web-99\n");
    assert_eq!(
        app.form.as_ref().unwrap().name,
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

    // Switching screens replaces the hitboxes rather than adding to them.
    app.screen = Screen::Chat;
    let _ = render(&mut app, 120, 30);
    assert!(
        app.regions.genkeys.is_empty(),
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
