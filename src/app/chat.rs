//! Chat screen state.
//!
//! The transcript the operator reads. The conversation the *model* sees lives
//! in the worker (`crate::agent`) and is a different shape — it has tool-call
//! ids and no spinner state, while this one has the reverse — so neither is
//! rebuilt from the other.

use tui_input::Input;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Running is part of the vocabulary; no backend emits it yet.
pub enum ToolStatus {
    Ok,
    Fail,
    Empty,
    Running,
}

impl ToolStatus {
    pub fn glyph(self) -> &'static str {
        match self {
            ToolStatus::Ok => "✓",
            ToolStatus::Fail => "×",
            ToolStatus::Empty => "○",
            ToolStatus::Running => "…",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ToolStatus::Ok => "ok",
            // Not "exit 1": the real code is in the output, and
            // inventing one that contradicts it is worse than saying less.
            ToolStatus::Fail => "failed",
            ToolStatus::Empty => "no output",
            ToolStatus::Running => "running",
        }
    }
}

/// Where a proposed plan stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Rejected/Ran are set by the plan dialog, next commit
pub enum PlanState {
    Proposed,
    Rejected,
    Ran,
}

#[derive(Debug, Clone)]
pub enum Turn {
    User(String),
    Assistant(String),
    Tool {
        name: String,
        arg: String,
        status: ToolStatus,
        out: Vec<String>,
    },
    /// A plan the model proposed. The card is the operator's way back into the
    /// confirmation dialog; it never runs anything by itself.
    Plan {
        id: u64,
        title: String,
        steps: usize,
        hosts: usize,
        state: PlanState,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ChatState {
    pub turns: Vec<Turn>,
    /// The composer. An `Input` like the form fields, so the line the operator
    /// is composing edits the same way as everything else.
    pub draft: Input,
    /// Index of the assistant turn currently being streamed into, if any.
    pub streaming: Option<usize>,
    /// Lines scrolled up from the bottom. 0 pins to the newest, which is where
    /// it stays unless the operator deliberately looks back.
    pub scroll: usize,
    /// The largest `scroll` the last render could honour — written back by the
    /// renderer, which is the only thing that knows how tall the transcript is.
    pub max_scroll: usize,
}

impl ChatState {
    #[cfg(test)]
    pub fn seeded() -> Self {
        let t = |name: &str, arg: &str, status, out: &[&str]| Turn::Tool {
            name: name.to_string(),
            arg: arg.to_string(),
            status,
            out: out.iter().map(|s| s.to_string()).collect(),
        };
        ChatState {
            turns: vec![
                Turn::User("nginx on web-01 is throwing 502s since the deploy. find out why.".into()),
                t(
                    "ssh",
                    "web-01 · journalctl -u nginx -n 50",
                    ToolStatus::Ok,
                    &[
                        "connect() failed (111: Connection refused) while connecting to upstream",
                        "upstream: \"http://127.0.0.1:8080/api/v2/items\"",
                    ],
                ),
                Turn::Assistant(
                    "nginx is fine — it cannot reach the upstream on :8080. Checking whether the app service is listening.".into(),
                ),
                t("ssh", "web-01 · ss -lntp | grep 8080", ToolStatus::Empty, &["(no output)"]),
                t(
                    "ssh",
                    "web-01 · systemctl status api",
                    ToolStatus::Fail,
                    &[
                        "● api.service - Items API",
                        "     Active: failed (Result: exit-code)",
                        "   Process: 2214 ExecStart=/srv/api/bin/server (code=exited, status=1/FAILURE)",
                        "api[2214]: FATAL: config key \"DATABASE_URL\" missing",
                    ],
                ),
                Turn::Assistant(
                    "Root cause: api.service on web-01 is down — it exits at boot because DATABASE_URL is missing from its environment file. The deploy replaced /srv/api/.env and dropped that key.\n\nI can restore it from web-02, which is running the same release, then restart the unit. Want me to apply it?".into(),
                ),
            ],
            ..Default::default()
        }
    }

    /// Take the composed message, if there is one, and record it as a turn.
    ///
    /// Returns the text so the caller can hand it to the worker — the previous
    /// signature returned only `bool` and dropped it.
    pub fn take_draft(&mut self) -> Option<String> {
        let text = self.draft.value().trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.turns.push(Turn::User(text.clone()));
        self.draft.reset();
        self.scroll = 0;
        Some(text)
    }

    /// Append streamed text to the assistant turn in flight, starting one if
    /// this is the first delta.
    pub fn push_delta(&mut self, text: &str) {
        match self.streaming {
            Some(i) => {
                if let Some(Turn::Assistant(body)) = self.turns.get_mut(i) {
                    body.push_str(text);
                    return;
                }
                // The turn went away underneath us; start a fresh one.
                self.streaming = None;
                self.push_delta(text);
            }
            None => {
                self.turns.push(Turn::Assistant(text.to_string()));
                self.streaming = Some(self.turns.len() - 1);
            }
        }
    }

    /// Close the streaming turn, dropping it if the model said nothing.
    pub fn finish_stream(&mut self) {
        if let Some(i) = self.streaming.take()
            && matches!(self.turns.get(i), Some(Turn::Assistant(b)) if b.trim().is_empty())
        {
            self.turns.remove(i);
        }
    }

    /// Mark the newest card for `id`, once the operator has decided.
    #[allow(dead_code)] // called by the plan dialog, next commit
    pub fn set_plan_state(&mut self, id: u64, state: PlanState) {
        for turn in self.turns.iter_mut().rev() {
            if let Turn::Plan {
                id: pid, state: s, ..
            } = turn
                && *pid == id
            {
                *s = state;
                return;
            }
        }
    }

    pub fn scroll_by(&mut self, delta: isize) {
        let next = self.scroll as isize + delta;
        self.scroll = next.clamp(0, self.max_scroll as isize) as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_transcript_has_the_expected_shape() {
        let c = ChatState::seeded();
        assert_eq!(c.turns.len(), 6);
        assert!(matches!(c.turns[0], Turn::User(_)));
        assert!(matches!(
            c.turns[3],
            Turn::Tool {
                status: ToolStatus::Empty,
                ..
            }
        ));
    }

    #[test]
    fn taking_the_draft_records_a_turn_and_yields_the_text() {
        let mut c = ChatState::seeded();
        let before = c.turns.len();
        c.draft = Input::new("  restart it  ".into());
        assert_eq!(c.take_draft().as_deref(), Some("restart it"));
        assert_eq!(c.turns.len(), before + 1);
        assert!(c.draft.value().is_empty());
        match c.turns.last().unwrap() {
            Turn::User(t) => assert_eq!(t, "restart it"),
            _ => panic!("expected a user turn"),
        }
    }

    #[test]
    fn an_empty_draft_sends_nothing() {
        let mut c = ChatState::seeded();
        let before = c.turns.len();
        c.draft = Input::new("   ".into());
        assert!(c.take_draft().is_none());
        assert_eq!(c.turns.len(), before);
    }

    #[test]
    fn deltas_accumulate_into_one_assistant_turn() {
        let mut c = ChatState::default();
        c.push_delta("Look");
        c.push_delta("ing…");
        assert_eq!(c.turns.len(), 1);
        match &c.turns[0] {
            Turn::Assistant(t) => assert_eq!(t, "Looking…"),
            _ => panic!("expected an assistant turn"),
        }
        c.finish_stream();
        assert!(c.streaming.is_none());
        // A second stream is a second turn.
        c.push_delta("Done.");
        assert_eq!(c.turns.len(), 2);
    }

    /// A turn that streamed nothing — the model went straight to a tool call —
    /// must not leave a blank gap in the transcript.
    #[test]
    fn an_empty_stream_leaves_no_turn_behind() {
        let mut c = ChatState::default();
        c.push_delta("");
        c.finish_stream();
        assert!(c.turns.is_empty(), "{:?}", c.turns);
    }

    #[test]
    fn a_plan_card_can_be_marked_decided() {
        let mut c = ChatState::default();
        c.turns.push(Turn::Plan {
            id: 1,
            title: "fix".into(),
            steps: 2,
            hosts: 3,
            state: PlanState::Proposed,
        });
        c.set_plan_state(1, PlanState::Ran);
        match &c.turns[0] {
            Turn::Plan { state, .. } => assert_eq!(*state, PlanState::Ran),
            _ => panic!("expected a plan card"),
        }
        // An unknown id is simply ignored.
        c.set_plan_state(99, PlanState::Rejected);
    }

    #[test]
    fn scrolling_is_clamped_to_what_was_drawn() {
        let mut c = ChatState {
            max_scroll: 5,
            ..Default::default()
        };
        c.scroll_by(3);
        assert_eq!(c.scroll, 3);
        c.scroll_by(10);
        assert_eq!(c.scroll, 5, "cannot scroll past the top");
        c.scroll_by(-99);
        assert_eq!(c.scroll, 0, "nor below the newest line");
    }

    #[test]
    fn status_glyphs_are_single_cell() {
        for s in [
            ToolStatus::Ok,
            ToolStatus::Fail,
            ToolStatus::Empty,
            ToolStatus::Running,
        ] {
            assert_eq!(s.glyph().chars().count(), 1);
            assert!(!s.label().is_empty());
        }
    }
}
