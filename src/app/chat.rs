//! Chat screen state — a stub. The transcript renders for real; nothing calls
//! a model. Seeded from `design/ui_kits/openadmin/data.js`'s nginx-502 session.

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
            ToolStatus::Fail => "exit 1",
            ToolStatus::Empty => "no output",
            ToolStatus::Running => "running",
        }
    }
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
}

#[derive(Debug, Clone, Default)]
pub struct ChatState {
    pub turns: Vec<Turn>,
    pub draft: String,
}

impl ChatState {
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
            draft: String::new(),
        }
    }

    /// Append the composed message. The agent backend is not wired up, so this
    /// records the turn and nothing else.
    pub fn send(&mut self) -> bool {
        let text = self.draft.trim().to_string();
        if text.is_empty() {
            return false;
        }
        self.turns.push(Turn::User(text));
        self.draft.clear();
        true
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
    fn sending_appends_a_turn_and_clears_the_draft() {
        let mut c = ChatState::seeded();
        let before = c.turns.len();
        c.draft = "  restart it  ".into();
        assert!(c.send());
        assert_eq!(c.turns.len(), before + 1);
        assert!(c.draft.is_empty());
        match c.turns.last().unwrap() {
            Turn::User(t) => assert_eq!(t, "restart it"),
            _ => panic!("expected a user turn"),
        }
    }

    #[test]
    fn an_empty_draft_sends_nothing() {
        let mut c = ChatState::seeded();
        let before = c.turns.len();
        c.draft = "   ".into();
        assert!(!c.send());
        assert_eq!(c.turns.len(), before);
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
