//! The security boundary, as a type.
//!
//! `Plan` is what the model proposes: it is built from tool JSON, and the agent
//! worker constructs one every time it wants something to change.
//! `ConfirmedPlan` is what the operator released. Its fields are private and
//! its only constructor is private to this module, so no code in
//! `crate::agent` can name a way to build one — that is a privacy error at
//! compile time, not a rule the model is asked to follow.
//!
//! The executor takes a `ConfirmedPlan` **by value**. A model that ignored every
//! instruction in its system prompt still could not run a scriptlet, because
//! there is no function it can reach that accepts what it can make.

use crate::agent::plan::{Plan, PlanStep};

/// A plan the operator confirmed, holding exactly what they left checked.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmedPlan {
    plan_id: u64,
    title: String,
    steps: Vec<PlanStep>,
}

impl ConfirmedPlan {
    pub fn plan_id(&self) -> u64 {
        self.plan_id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn steps(&self) -> &[PlanStep] {
        &self.steps
    }

    /// Rebuild the plan from the checkboxes.
    ///
    /// Private: only `App::confirm_plan`, below, can call it. Unchecked steps
    /// and hosts are *absent* from the result rather than flagged, so the
    /// executor has no "enabled" field it could misread.
    fn from_selection(sel: &PlanSelection) -> Option<ConfirmedPlan> {
        let mut steps = Vec::new();
        for (i, step) in sel.plan.steps.iter().enumerate() {
            if !sel.step_on.get(i).copied().unwrap_or(false) {
                continue;
            }
            let hosts: Vec<i64> = step
                .hosts
                .iter()
                .enumerate()
                .filter(|(j, _)| sel.host_on[i].get(*j).copied().unwrap_or(false))
                .map(|(_, id)| *id)
                .collect();
            if hosts.is_empty() {
                continue;
            }
            steps.push(PlanStep {
                summary: step.summary.clone(),
                kind: step.kind.clone(),
                hosts,
            });
        }
        (!steps.is_empty()).then(|| ConfirmedPlan {
            plan_id: sel.plan.id,
            title: sel.plan.title.clone(),
            steps,
        })
    }
}

/// One line of the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Step(usize),
    Host(usize, usize),
}

/// The dialog's state: the plan as proposed, plus what the operator wants.
///
/// These are separate on purpose — the plan is what the model said, and it is
/// never edited, so the transcript and the model's history stay true.
#[derive(Debug, Clone)]
pub struct PlanSelection {
    pub plan: Plan,
    pub step_on: Vec<bool>,
    pub host_on: Vec<Vec<bool>>,
    pub cursor: usize,
    /// First body line in view. Free to leave the cursor behind: a script
    /// taller than the dialog has lines no row sits on, and the operator has
    /// to be able to read them.
    pub scroll: usize,
    /// Whether the view tracks the cursor (it last moved) or the cursor
    /// tracks the view (it was last scrolled). Only the renderer knows the
    /// layout, so it is the one that reconciles the two.
    pub follow: bool,
    /// Written back by the renderer, which is the only thing that knows how
    /// tall the body is and how much of it fits.
    pub max_scroll: usize,
    pub page: usize,
    /// Whether `↵` runs. A dialog the operator opened is armed; one that
    /// opened itself is not, until they touch it. See
    /// `App::maybe_auto_open_plan`.
    pub armed: bool,
}

impl PlanSelection {
    /// Everything starts checked: the operator is approving, not assembling.
    pub fn new(plan: Plan) -> Self {
        let step_on = vec![true; plan.steps.len()];
        let host_on = plan
            .steps
            .iter()
            .map(|s| vec![true; s.hosts.len()])
            .collect();
        PlanSelection {
            plan,
            step_on,
            host_on,
            cursor: 0,
            scroll: 0,
            follow: true,
            max_scroll: 0,
            page: 1,
            armed: true,
        }
    }

    /// A dialog that appeared on its own does not run on the next `↵`.
    pub fn disarm(&mut self) {
        self.armed = false;
    }

    /// Any deliberate touch — a cursor move, a toggle, the pointer entering
    /// the dialog — is proof somebody is looking at it.
    pub fn arm(&mut self) {
        self.armed = true;
    }

    /// The flattened row list the dialog draws and the cursor walks.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (i, step) in self.plan.steps.iter().enumerate() {
            rows.push(Row::Step(i));
            for j in 0..step.hosts.len() {
                rows.push(Row::Host(i, j));
            }
        }
        rows
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let n = self.rows().len();
        if n == 0 {
            return;
        }
        self.cursor = (self.cursor as isize + delta).clamp(0, n as isize - 1) as usize;
        self.follow = true;
    }

    /// Move the view, not the cursor; the renderer brings the cursor along
    /// if a row is in sight.
    pub fn scroll_by(&mut self, delta: isize) {
        let next = self.scroll as isize + delta;
        self.scroll = next.clamp(0, self.max_scroll as isize) as usize;
        self.follow = false;
    }

    /// Toggle whatever the cursor is on — and show it, so a row toggled while
    /// scrolled out of view is not changed unseen.
    pub fn toggle_cursor(&mut self) {
        self.follow = true;
        match self.rows().get(self.cursor).copied() {
            Some(Row::Step(i)) => self.toggle_step(i),
            Some(Row::Host(i, j)) => self.toggle_host(i, j),
            None => {}
        }
    }

    pub fn toggle_step(&mut self, i: usize) {
        if let Some(on) = self.step_on.get_mut(i) {
            *on = !*on;
        }
    }

    /// Toggling a host implies the step: unchecking the last host turns the
    /// step off, and checking one back on turns it on again, so the two can
    /// never disagree about whether anything will run.
    pub fn toggle_host(&mut self, i: usize, j: usize) {
        let Some(row) = self.host_on.get_mut(i) else {
            return;
        };
        let Some(on) = row.get_mut(j) else { return };
        *on = !*on;
        let any = row.iter().any(|b| *b);
        if let Some(step) = self.step_on.get_mut(i) {
            *step = any;
        }
    }

    pub fn set_all(&mut self, on: bool) {
        for s in self.step_on.iter_mut() {
            *s = on;
        }
        for row in self.host_on.iter_mut() {
            for h in row.iter_mut() {
                *h = on;
            }
        }
    }

    /// Whether a host will actually run: its step has to be on too.
    pub fn host_active(&self, i: usize, j: usize) -> bool {
        self.step_on.get(i).copied().unwrap_or(false)
            && self
                .host_on
                .get(i)
                .and_then(|r| r.get(j))
                .copied()
                .unwrap_or(false)
    }

    /// `(steps, distinct hosts)` that Confirm would start.
    ///
    /// Hosts, not step×host runs: seven steps on the same two machines is two
    /// hosts, which is what the operator ticked and what the card says.
    pub fn counts(&self) -> (usize, usize) {
        let mut steps = 0;
        let mut hosts: Vec<i64> = Vec::new();
        for (i, step) in self.plan.steps.iter().enumerate() {
            let before = hosts.len();
            hosts.extend(
                step.hosts
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| self.host_active(i, *j))
                    .map(|(_, id)| *id),
            );
            if hosts.len() > before {
                steps += 1;
            }
        }
        hosts.sort_unstable();
        hosts.dedup();
        (steps, hosts.len())
    }

    pub fn is_empty(&self) -> bool {
        self.counts().0 == 0
    }
}

impl crate::app::App {
    /// The only caller of `ConfirmedPlan::from_selection` in the program.
    pub(super) fn confirm_plan(&mut self) {
        let Some(sel) = self.plan.take() else { return };
        let Some(approved) = ConfirmedPlan::from_selection(&sel) else {
            // Nothing checked: keep the dialog rather than silently doing
            // nothing, which would look like a failure to respond.
            self.plan = Some(sel);
            self.flash("Nothing selected.", super::StatusKind::Warn);
            return;
        };
        let (steps, hosts) = sel.counts();
        self.mode = super::Mode::Normal;
        self.pending_plan = None;
        self.chat
            .set_plan_state(approved.plan_id(), super::chat::PlanState::Ran);
        self.start_execution(approved);
        self.flash(
            format!("Running {steps} step(s) across {hosts} host(s)…"),
            super::StatusKind::Loading,
        );
    }

    pub(super) fn reject_plan(&mut self) {
        if let Some(sel) = self.plan.take() {
            self.chat
                .set_plan_state(sel.plan.id, super::chat::PlanState::Rejected);
        }
        self.pending_plan = None;
        self.mode = super::Mode::Normal;
        self.flash("Plan rejected — nothing ran.", super::StatusKind::Warn);
    }

    /// Open the dialog on the proposal waiting for review.
    pub(super) fn open_plan(&mut self) {
        let Some(plan) = self.pending_plan.clone() else {
            self.flash("No plan is waiting for review.", super::StatusKind::Warn);
            return;
        };
        // Reopening after a hide keeps the checkboxes as they were left; a
        // different plan starts fresh.
        if self.plan.as_ref().map(|s| s.plan.id) != Some(plan.id) {
            self.plan = Some(PlanSelection::new(plan));
        }
        if let Some(sel) = self.plan.as_mut() {
            // Asking for it is already the deliberate act.
            sel.arm();
        }
        self.plan_hidden = false;
        self.mode = super::Mode::ConfirmPlan;
    }

    /// Step out of the dialog without answering it.
    ///
    /// The plan stays pending and the checkboxes keep their state; the card
    /// and `F2` bring it back. This exists because the dialog now opens
    /// itself: it lands on top of the transcript the operator needs in order
    /// to judge it, and the only other way out is `Esc`, which rejects.
    pub(super) fn hide_plan(&mut self) {
        if self.pending_plan.is_none() {
            return;
        }
        self.mode = super::Mode::Normal;
        self.plan_hidden = true;
        self.flash("Plan hidden — F2 brings it back.", super::StatusKind::Warn);
    }

    /// Show a waiting plan by itself, at the first moment it cannot cost
    /// anything.
    ///
    /// A plan is the one thing here that stops the turn until it is answered,
    /// so making the operator discover it wastes the very time the agent saved.
    /// But a modal is an input thief: on the Shells screen it would eat a
    /// keystroke meant for a remote shell, and over a half-typed message it
    /// would swallow the sentence and read the letters as commands — `n`
    /// alone rejects. So it opens only on Chat, with no other dialog up and
    /// nothing in the composer. When a condition fails nothing is lost: the
    /// card and the `F2` cap still advertise it, and this is retried every
    /// frame, so it appears the moment the operator arrives.
    ///
    /// It opens *disarmed* — see `App::key_confirm_plan`.
    ///
    /// Returns whether it opened, so the caller knows to redraw.
    pub fn maybe_auto_open_plan(&mut self) -> bool {
        if self.pending_plan.is_none()
            || self.plan_hidden
            || self.mode != super::Mode::Normal
            || self.screen != super::Screen::Chat
            || self.alert.is_some()
            || !self.chat.draft_text().trim().is_empty()
        {
            return false;
        }
        self.open_plan();
        if let Some(sel) = self.plan.as_mut() {
            sel.disarm();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::plan::StepKind;

    fn plan() -> Plan {
        Plan {
            id: 4,
            title: "tidy".into(),
            steps: vec![
                PlanStep {
                    summary: "clean".into(),
                    kind: StepKind::Scriptlet {
                        script: "apt-get clean".into(),
                    },
                    hosts: vec![1, 2, 3],
                },
                PlanStep {
                    summary: "ship".into(),
                    kind: StepKind::Upload {
                        artifact: "fix.sh".into(),
                    },
                    hosts: vec![1],
                },
            ],
        }
    }

    #[test]
    fn everything_starts_checked() {
        let sel = PlanSelection::new(plan());
        assert_eq!(sel.counts(), (2, 3));
        let c = ConfirmedPlan::from_selection(&sel).unwrap();
        assert_eq!(c.plan_id(), 4);
        assert_eq!(c.steps().len(), 2);
        assert_eq!(c.steps()[0].hosts, vec![1, 2, 3]);
    }

    #[test]
    fn the_rows_interleave_steps_and_their_hosts() {
        let sel = PlanSelection::new(plan());
        assert_eq!(
            sel.rows(),
            vec![
                Row::Step(0),
                Row::Host(0, 0),
                Row::Host(0, 1),
                Row::Host(0, 2),
                Row::Step(1),
                Row::Host(1, 0),
            ]
        );
    }

    /// An unchecked host is absent from the result, not flagged in it.
    #[test]
    fn unchecking_a_host_excludes_exactly_that_host() {
        let mut sel = PlanSelection::new(plan());
        sel.toggle_host(0, 1);
        assert_eq!(sel.counts(), (2, 2));
        let c = ConfirmedPlan::from_selection(&sel).unwrap();
        assert_eq!(c.steps()[0].hosts, vec![1, 3], "host 2 is gone entirely");
        assert_eq!(c.steps()[1].hosts, vec![1]);
    }

    /// Host 1 is in both steps: it is one machine, not two runs' worth.
    #[test]
    fn a_host_in_several_steps_is_counted_once() {
        let mut sel = PlanSelection::new(plan());
        assert_eq!(sel.counts().1, 3, "hosts 1, 2 and 3");
        sel.toggle_host(0, 0);
        assert_eq!(sel.counts().1, 3, "host 1 still runs in the second step");
        sel.toggle_host(1, 0);
        assert_eq!(sel.counts(), (1, 2), "now it runs nowhere");
    }

    #[test]
    fn unchecking_a_step_drops_it_whole() {
        let mut sel = PlanSelection::new(plan());
        sel.toggle_step(0);
        assert_eq!(sel.counts(), (1, 1));
        let c = ConfirmedPlan::from_selection(&sel).unwrap();
        assert_eq!(c.steps().len(), 1);
        assert_eq!(c.steps()[0].summary, "ship");
    }

    /// The two controls must never disagree about whether anything will run.
    #[test]
    fn a_step_follows_its_hosts() {
        let mut sel = PlanSelection::new(plan());
        sel.toggle_host(1, 0);
        assert!(!sel.step_on[1], "the last host off turns the step off");
        sel.toggle_host(1, 0);
        assert!(sel.step_on[1], "and back on again");
    }

    #[test]
    fn a_step_with_no_hosts_left_is_not_confirmed() {
        let mut sel = PlanSelection::new(plan());
        for j in 0..3 {
            sel.toggle_host(0, j);
        }
        let c = ConfirmedPlan::from_selection(&sel).unwrap();
        assert_eq!(c.steps().len(), 1, "the emptied step is gone: {c:?}");
    }

    #[test]
    fn nothing_checked_confirms_nothing() {
        let mut sel = PlanSelection::new(plan());
        sel.set_all(false);
        assert!(sel.is_empty());
        assert_eq!(ConfirmedPlan::from_selection(&sel), None);
    }

    #[test]
    fn the_cursor_walks_the_rows_and_stops_at_the_ends() {
        let mut sel = PlanSelection::new(plan());
        sel.move_cursor(-1);
        assert_eq!(sel.cursor, 0);
        sel.move_cursor(99);
        assert_eq!(sel.cursor, 5);
        sel.toggle_cursor();
        assert!(!sel.host_on[1][0]);
    }

    #[test]
    fn scrolling_is_clamped_and_lets_go_of_the_cursor() {
        let mut sel = PlanSelection::new(plan());
        sel.max_scroll = 7;
        sel.scroll_by(-3);
        assert_eq!(sel.scroll, 0, "not above the top");
        sel.scroll_by(99);
        assert_eq!(sel.scroll, 7, "nor past the end");
        assert!(!sel.follow, "the view moved, so the cursor follows it");
        sel.move_cursor(1);
        assert!(sel.follow, "and moving the cursor takes the view back");
    }

    #[test]
    fn a_host_is_only_active_when_its_step_is() {
        let mut sel = PlanSelection::new(plan());
        sel.toggle_step(0);
        assert!(
            !sel.host_active(0, 0),
            "the host is checked but the step is not"
        );
        assert!(
            sel.host_on[0][0],
            "and its own box is untouched, so it can come back"
        );
        sel.toggle_step(0);
        assert!(sel.host_active(0, 0));
    }
}
