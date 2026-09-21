//! Tabs, panes and focus for the Shells screen.
//!
//! A tab opened from one host holds one pane. A tab opened with several hosts
//! marked is titled `Group: <first host>` and stacks one pane per host
//! vertically (`design/ui_kits/openadmin/ShellsScreen.jsx:81-97`).

use super::session::{SessionId, Spawn, TermEvent, TerminalSession};
use anyhow::Result;
use ratatui::layout::{Constraint, Layout, Rect};
use std::collections::HashMap;
use std::sync::mpsc::Sender;

/// Rows of chrome each pane spends on its title border.
pub const PANE_CHROME_ROWS: u16 = 1;

/// Columns each pane keeps for its scrollbar, as a GUI terminal does: the
/// program in the pane is told it is this much narrower, so the bar never
/// covers its text.
pub const PANE_SCROLLBAR_COLS: u16 = 1;

/// One pane, cut into what is drawn where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneAreas {
    /// The title rule, full width; zero rows high when there is none.
    pub title: Rect,
    /// What the program in the pane draws on — and so the size its PTY is.
    pub term: Rect,
    /// The scrollbar column, beside `term`.
    pub bar: Rect,
}

pub struct Tab {
    pub title: String,
    pub group: bool,
    pub panes: Vec<SessionId>,
    pub focus: usize,
}

impl Tab {
    pub fn focused(&self) -> Option<SessionId> {
        self.panes.get(self.focus).copied()
    }
}

#[derive(Default)]
pub struct TerminalManager {
    pub tabs: Vec<Tab>,
    pub active: Option<usize>,
    sessions: HashMap<SessionId, TerminalSession>,
    next_id: SessionId,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn session(&self, id: SessionId) -> Option<&TerminalSession> {
        self.sessions.get(&id)
    }

    pub fn session_mut(&mut self, id: SessionId) -> Option<&mut TerminalSession> {
        self.sessions.get_mut(&id)
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|i| self.tabs.get(i))
    }

    /// Rows the active tab spends on pane titles.
    ///
    /// A lone pane needs no title — the header tab already names the host — so
    /// it gets none and the terminal takes the whole rect. Every consumer of
    /// pane geometry goes through this, so the PTY size and the mouse mapping
    /// cannot drift from what is drawn.
    ///
    /// This depends on the pane count alone, which is fixed for a tab's whole
    /// life. Deliberately *not* on whether a pane has exited: that flag is set
    /// by the reader thread and could flip between the resize and the draw,
    /// and shrinking a `vt100` parser truncates its rows from the end — which
    /// is exactly where a dead shell's last output sits. A lone pane that
    /// exits is reaped within the frame anyway (`reap_finished`), so there is
    /// nothing to show a title for.
    pub fn pane_chrome_rows(&self) -> u16 {
        match self.active_tab() {
            Some(tab) if tab.panes.len() > 1 => PANE_CHROME_ROWS,
            _ => 0,
        }
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        match self.active {
            Some(i) => self.tabs.get_mut(i),
            None => None,
        }
    }

    /// The session that keystrokes go to.
    pub fn focused_session(&self) -> Option<SessionId> {
        self.active_tab().and_then(Tab::focused)
    }

    /// Open one tab for `entries` — a single pane, or a stacked group.
    ///
    /// Each entry is `(title, spawn)`. A failure to spawn any one pane aborts
    /// the whole tab rather than leaving it half-built.
    pub fn open_tab(
        &mut self,
        entries: Vec<(String, Spawn)>,
        size: (u16, u16),
        scrollback: usize,
        term: &str,
        tx: &Sender<TermEvent>,
    ) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let group = entries.len() > 1;
        let title = if group {
            format!("Group: {}", entries[0].0)
        } else {
            entries[0].0.clone()
        };

        let per = pane_size(size, entries.len());
        let mut panes = Vec::with_capacity(entries.len());
        for (name, spawn) in &entries {
            self.next_id += 1;
            let id = self.next_id;
            match TerminalSession::spawn(id, name, spawn, per, scrollback, term, tx.clone()) {
                Ok(s) => {
                    self.sessions.insert(id, s);
                    panes.push(id);
                }
                Err(e) => {
                    // Roll back the panes already spawned for this tab.
                    for id in panes {
                        self.sessions.remove(&id);
                    }
                    return Err(e);
                }
            }
        }

        self.tabs.push(Tab {
            title,
            group,
            panes,
            focus: 0,
        });
        self.active = Some(self.tabs.len() - 1);
        Ok(())
    }

    pub fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(index);
        for id in tab.panes {
            if let Some(mut s) = self.sessions.remove(&id) {
                s.kill();
            }
        }
        self.active = if self.tabs.is_empty() {
            None
        } else {
            Some(index.min(self.tabs.len() - 1))
        };
    }

    pub fn close_active_tab(&mut self) {
        if let Some(i) = self.active {
            self.close_tab(i);
        }
    }

    pub fn next_tab(&mut self) {
        if let Some(i) = self.active
            && !self.tabs.is_empty()
        {
            self.active = Some((i + 1) % self.tabs.len());
        }
    }

    pub fn select_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = Some(index);
        }
    }

    /// Move focus to the next pane within the active group tab.
    pub fn next_pane(&mut self) {
        if let Some(tab) = self.active_tab_mut()
            && !tab.panes.is_empty()
        {
            tab.focus = (tab.focus + 1) % tab.panes.len();
        }
    }

    pub fn focus_pane(&mut self, index: usize) {
        if let Some(tab) = self.active_tab_mut()
            && index < tab.panes.len()
        {
            tab.focus = index;
        }
    }

    /// Write to whichever pane has focus, returning its view to the live
    /// screen first — typing into a shell that is scrolled back means the
    /// prompt, as in any terminal.
    ///
    /// Keys and pastes come through here; forwarded mouse reports do not, so a
    /// click in a program leaves the view where it is. Nothing is written to a
    /// pane whose program has gone, so nothing moves its view either: reading
    /// back a dead shell's last output is what it is left open for.
    pub fn write_focused(&mut self, bytes: &[u8]) -> Result<()> {
        match self.focused_session() {
            Some(id) => match self.sessions.get_mut(&id) {
                Some(s) => {
                    if !bytes.is_empty() && !s.has_exited() {
                        s.scroll_to_live();
                    }
                    s.write(bytes)
                }
                None => Ok(()),
            },
            None => Ok(()),
        }
    }

    /// Any pane with pending output? Clears the flags as it goes.
    pub fn take_dirty(&mut self) -> bool {
        let mut dirty = false;
        for s in self.sessions.values() {
            dirty |= s.take_dirty();
        }
        dirty
    }

    /// Cut one pane into its title, terminal and scrollbar. The renderer, the
    /// PTY sizing and the mouse mapping all go through this, so what is drawn
    /// and what the program believes its size is cannot disagree.
    ///
    /// A pane one column wide keeps that column for the terminal and has no
    /// bar: a program with no width at all is worse than a missing scrollbar.
    pub fn pane_areas(pane: Rect, chrome: u16) -> PaneAreas {
        let title_h = chrome.min(pane.height);
        let bar_w = if pane.width > PANE_SCROLLBAR_COLS {
            PANE_SCROLLBAR_COLS
        } else {
            0
        };
        let term = Rect::new(
            pane.x,
            pane.y + title_h,
            pane.width - bar_w,
            pane.height - title_h,
        );
        PaneAreas {
            title: Rect {
                height: title_h,
                ..pane
            },
            term,
            bar: Rect::new(term.right(), term.y, bar_w, term.height),
        }
    }

    /// Split `area` between `n` stacked panes — the same geometry the renderer
    /// uses, so sizes can never drift from what is drawn.
    pub fn pane_rects(area: Rect, n: usize) -> Vec<Rect> {
        if n == 0 {
            return Vec::new();
        }
        Layout::vertical(vec![Constraint::Fill(1); n])
            .split(area)
            .to_vec()
    }

    /// Resize every pane of the active tab to match `rects`. Returns whether
    /// anything actually changed.
    pub fn sync_sizes(&mut self, rects: &[Rect]) -> bool {
        let chrome = self.pane_chrome_rows();
        let Some(tab) = self.active.and_then(|i| self.tabs.get(i)) else {
            return false;
        };
        let ids: Vec<SessionId> = tab.panes.clone();
        let mut changed = false;
        for (id, rect) in ids.iter().zip(rects) {
            let term = Self::pane_areas(*rect, chrome).term;
            let size = (term.height, term.width);
            if let Some(s) = self.sessions.get_mut(id)
                && s.size() != (term.height.max(1), term.width.max(1))
            {
                let _ = s.resize(size);
                changed = true;
            }
        }
        changed
    }

    /// Drop tabs whose panes have all exited — used when the user closes a
    /// shell from inside (`exit`) rather than with a chord.
    pub fn reap_finished(&mut self) {
        let finished: Vec<usize> = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                !t.panes.is_empty()
                    && t.panes.iter().all(|id| {
                        self.sessions
                            .get(id)
                            .map(TerminalSession::has_exited)
                            .unwrap_or(true)
                    })
            })
            .map(|(i, _)| i)
            .collect();
        for i in finished.into_iter().rev() {
            self.close_tab(i);
        }
    }

    pub fn shutdown(&mut self) {
        for (_, mut s) in self.sessions.drain() {
            s.kill();
        }
        self.tabs.clear();
        self.active = None;
    }
}

/// The starting size for each pane when a tab of `n` panes opens into `size`.
fn pane_size((rows, cols): (u16, u16), n: usize) -> (u16, u16) {
    // Only a stacked group spends rows on titles; the first render corrects
    // this anyway through `sync_sizes`.
    let chrome = if n > 1 { PANE_CHROME_ROWS } else { 0 };
    let n = n.max(1) as u16;
    let term = TerminalManager::pane_areas(Rect::new(0, 0, cols, rows / n), chrome).term;
    (term.height.max(1), term.width.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sleeper() -> Spawn {
        let mut s = Spawn::new("/bin/sh");
        s.args = vec!["-c".into(), "sleep 30".into()];
        s
    }

    fn mgr_with(n: usize) -> (TerminalManager, std::sync::mpsc::Receiver<TermEvent>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut m = TerminalManager::new();
        let entries: Vec<(String, Spawn)> =
            (0..n).map(|i| (format!("host-{i}"), sleeper())).collect();
        m.open_tab(entries, (24, 80), 50, "xterm-256color", &tx)
            .unwrap();
        (m, rx)
    }

    #[test]
    fn a_single_host_tab_is_named_after_the_host() {
        let (m, _rx) = mgr_with(1);
        assert_eq!(m.tabs[0].title, "host-0");
        assert!(!m.tabs[0].group);
        assert_eq!(m.tabs[0].panes.len(), 1);
    }

    #[test]
    fn several_hosts_open_one_group_tab_of_stacked_panes() {
        let (m, _rx) = mgr_with(3);
        assert_eq!(m.tabs.len(), 1, "a group is one tab, not three");
        assert_eq!(m.tabs[0].title, "Group: host-0");
        assert!(m.tabs[0].group);
        assert_eq!(m.tabs[0].panes.len(), 3);
    }

    #[test]
    fn group_panes_each_get_a_share_of_the_rows() {
        let (m, _rx) = mgr_with(3);
        for id in &m.tabs[0].panes {
            let (rows, cols) = m.session(*id).unwrap().size();
            // The full width, less the scrollbar's column.
            assert_eq!(cols, 80 - PANE_SCROLLBAR_COLS);
            // 24 rows / 3 panes = 8, less one row of title chrome.
            assert_eq!(rows, 7);
        }
    }

    #[test]
    fn focus_cycles_within_a_group_and_routes_input() {
        let (mut m, _rx) = mgr_with(3);
        let first = m.focused_session().unwrap();
        m.next_pane();
        let second = m.focused_session().unwrap();
        assert_ne!(first, second);
        m.next_pane();
        m.next_pane();
        assert_eq!(m.focused_session().unwrap(), first, "focus wraps");

        m.focus_pane(1);
        assert_eq!(m.focused_session().unwrap(), second);
        assert!(m.write_focused(b"x").is_ok());
    }

    #[test]
    fn closing_a_tab_kills_its_sessions_and_picks_a_neighbour() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut m = TerminalManager::new();
        for i in 0..3 {
            m.open_tab(
                vec![(format!("h{i}"), sleeper())],
                (24, 80),
                50,
                "xterm",
                &tx,
            )
            .unwrap();
        }
        assert_eq!(m.active, Some(2));
        m.close_active_tab();
        assert_eq!(m.tab_count(), 2);
        assert_eq!(m.active, Some(1));
        m.close_tab(0);
        assert_eq!(m.tab_count(), 1);
        assert_eq!(m.active, Some(0));
        m.close_tab(0);
        assert!(m.is_empty());
        assert_eq!(m.active, None, "no tabs means nothing is focused");
        assert!(
            m.write_focused(b"x").is_ok(),
            "input with no tabs is a no-op"
        );
    }

    #[test]
    fn tab_cycling_wraps() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut m = TerminalManager::new();
        for i in 0..3 {
            m.open_tab(
                vec![(format!("h{i}"), sleeper())],
                (24, 80),
                50,
                "xterm",
                &tx,
            )
            .unwrap();
        }
        m.select_tab(2);
        m.next_tab();
        assert_eq!(m.active, Some(0), "next wraps past the end");
        m.next_tab();
        assert_eq!(m.active, Some(1));
    }

    /// The renderer and the resizer must agree, so both go through pane_rects.
    #[test]
    fn pane_rects_tile_the_area_without_gaps() {
        let area = Rect::new(0, 2, 80, 24);
        let rects = TerminalManager::pane_rects(area, 3);
        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0].y, 2);
        assert_eq!(rects.iter().map(|r| r.height).sum::<u16>(), 24);
        for r in &rects {
            assert_eq!(r.width, 80);
        }
        assert!(TerminalManager::pane_rects(area, 0).is_empty());
    }

    #[test]
    fn sync_sizes_resizes_panes_to_the_drawn_layout() {
        let (mut m, _rx) = mgr_with(2);
        let rects = TerminalManager::pane_rects(Rect::new(0, 0, 120, 40), 2);
        assert!(m.sync_sizes(&rects), "first sync must report a change");
        for id in &m.tabs[0].panes.clone() {
            // 40 rows / 2 panes = 20, less the title row; the width, less the
            // scrollbar's column.
            assert_eq!(
                m.session(*id).unwrap().size(),
                (19, 120 - PANE_SCROLLBAR_COLS)
            );
        }
        assert!(!m.sync_sizes(&rects), "a second sync is a no-op");
    }

    /// The scrollbar has a column of its own, so the program in the pane is
    /// told it is one narrower and the bar never covers its text. The title
    /// rule keeps the full width.
    #[test]
    fn a_pane_keeps_its_right_column_for_the_scrollbar() {
        let a = TerminalManager::pane_areas(Rect::new(3, 1, 80, 24), 0);
        assert_eq!(a.term, Rect::new(3, 1, 79, 24));
        assert_eq!(a.bar, Rect::new(82, 1, 1, 24));
        assert_eq!(a.title.height, 0);

        let a = TerminalManager::pane_areas(Rect::new(0, 10, 80, 8), PANE_CHROME_ROWS);
        assert_eq!(a.title, Rect::new(0, 10, 80, 1), "the title spans the bar");
        assert_eq!(a.term, Rect::new(0, 11, 79, 7));
        assert_eq!(
            a.bar,
            Rect::new(79, 11, 1, 7),
            "the bar runs beside the text"
        );

        // Too narrow for both: the program keeps the column.
        let a = TerminalManager::pane_areas(Rect::new(0, 0, 1, 5), 0);
        assert_eq!((a.term.width, a.bar.width), (1, 0));
        let a = TerminalManager::pane_areas(Rect::new(0, 0, 0, 0), PANE_CHROME_ROWS);
        assert_eq!((a.term.width, a.bar.width, a.term.height), (0, 0, 0));
    }

    /// Typing into a shell that is scrolled back means the prompt. An empty
    /// write sends nothing, so it moves nothing either.
    #[test]
    fn typing_into_the_focused_pane_returns_it_to_the_live_screen() {
        let (mut m, _rx) = mgr_with(1);
        let id = m.tabs[0].panes[0];
        let offset = |m: &TerminalManager| {
            m.session(id)
                .unwrap()
                .parser()
                .lock()
                .unwrap()
                .screen()
                .scrollback()
        };
        {
            let s = m.session(id).unwrap();
            let mut p = s.parser().lock().unwrap();
            for i in 0..100 {
                p.process(format!("line {i}\r\n").as_bytes());
            }
        }
        m.session(id).unwrap().scroll_history(10);
        assert_eq!(offset(&m), 10);

        m.write_focused(b"").unwrap();
        assert_eq!(offset(&m), 10, "an empty write leaves the view alone");
        m.write_focused(b"x").unwrap();
        assert_eq!(offset(&m), 0, "a keystroke returns to the prompt");
    }

    #[test]
    fn a_failed_spawn_leaves_no_half_built_tab() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut m = TerminalManager::new();
        let entries = vec![
            ("good".to_string(), sleeper()),
            (
                "bad".to_string(),
                Spawn::new("/nonexistent/openadmin-test-binary"),
            ),
        ];
        assert!(m.open_tab(entries, (24, 80), 50, "xterm", &tx).is_err());
        assert!(m.is_empty(), "no tab should survive a failed pane");
        assert_eq!(m.active, None);
    }

    #[test]
    fn finished_tabs_are_reaped() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut m = TerminalManager::new();
        let mut quick = Spawn::new("/bin/sh");
        quick.args = vec!["-c".into(), "exit 0".into()];
        m.open_tab(vec![("gone".into(), quick)], (24, 80), 50, "xterm", &tx)
            .unwrap();

        // Wait for the reader thread to observe EOF.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let id = m.tabs[0].panes[0];
            if m.session(id).unwrap().has_exited() {
                break;
            }
            let _ = rx.recv_timeout(std::time::Duration::from_millis(100));
        }
        m.reap_finished();
        assert!(m.is_empty(), "a tab whose only pane exited should close");
    }
}
