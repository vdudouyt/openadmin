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

    /// Write to whichever pane has focus.
    pub fn write_focused(&mut self, bytes: &[u8]) -> Result<()> {
        match self.focused_session() {
            Some(id) => match self.sessions.get_mut(&id) {
                Some(s) => s.write(bytes),
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
        let Some(tab) = self.active.and_then(|i| self.tabs.get(i)) else {
            return false;
        };
        let ids: Vec<SessionId> = tab.panes.clone();
        let mut changed = false;
        for (id, rect) in ids.iter().zip(rects) {
            let rows = rect.height.saturating_sub(PANE_CHROME_ROWS);
            let size = (rows, rect.width);
            if let Some(s) = self.sessions.get_mut(id)
                && s.size() != (rows.max(1), rect.width.max(1))
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
    let n = n.max(1) as u16;
    let per = rows / n;
    (per.saturating_sub(PANE_CHROME_ROWS).max(1), cols.max(1))
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
            assert_eq!(cols, 80);
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
            // 40 rows / 2 panes = 20, less the title row.
            assert_eq!(m.session(*id).unwrap().size(), (19, 120));
        }
        assert!(!m.sync_sizes(&rects), "a second sync is a no-op");
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
