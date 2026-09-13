//! Talking to the model.
//!
//! `LlmClient` is the seam the rest of the loop is written against: the worker
//! never knows whether it is speaking to a real endpoint or a scripted one, so
//! the whole agent loop is testable without a network.

use super::proto::{
    ChatChunk, ChatRequest, ErrorEnvelope, Message, SseLine, ToolDef, TurnAccumulator,
    parse_sse_line,
};
use crate::config::AgentConfig;
use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How a streamed turn ended.
pub struct TurnOutcome {
    pub turn: TurnAccumulator,
    /// True when the user cancelled mid-stream; `turn` then holds the partial
    /// text, which is still worth showing.
    pub cancelled: bool,
}

pub trait LlmClient: Send {
    /// Run one streaming completion, calling `on_delta` with each text fragment
    /// as it arrives. Returns once the stream ends, the model asks for tools,
    /// or `cancel` is raised.
    fn stream_turn(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        cancel: &AtomicBool,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<TurnOutcome>;
}

// ---- the real one --------------------------------------------------------

pub struct HttpClient {
    agent: ureq::Agent,
    url: String,
    auth: Option<String>,
    model: String,
    /// Empty means the field is not sent at all.
    reasoning_effort: String,
}

/// The `Authorization` value, or `None` when there is no key.
///
/// A local llama.cpp or Ollama needs no key, so the header is omitted rather
/// than sent empty. Its own function because `AgentConfig::key()` consults the
/// environment, and a test that went through it would pass or fail depending on
/// whether the developer happens to export `OPENAI_API_KEY`.
fn auth_header(api_key: &str) -> Option<String> {
    (!api_key.trim().is_empty()).then(|| format!("Bearer {}", api_key.trim()))
}

impl HttpClient {
    /// Takes the whole `AgentConfig` rather than a list of strings: every field
    /// it needs comes from there, and four positional arguments of which two were
    /// adjacent strings — key and model — was one transposition away from
    /// sending the model name as the credential.
    pub fn new(cfg: &AgentConfig) -> Result<Self> {
        let model = cfg.model.trim();
        if model.is_empty() {
            bail!("no model configured");
        }
        let (base_url, api_key, recv_body_secs) =
            (cfg.base_url.as_str(), cfg.key(), cfg.stream_timeout_secs);
        let config = ureq::Agent::config_builder()
            // No global timeout: a streamed turn can legitimately run for
            // minutes. Bound the phases that should never be slow instead, and
            // leave generation to the cancel flag and the backstop below.
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(120)))
            .timeout_recv_body(Some(Duration::from_secs(recv_body_secs)))
            // Surface the server's JSON error body instead of an opaque status.
            .http_status_as_error(false)
            .build();
        Ok(HttpClient {
            agent: config.new_agent(),
            reasoning_effort: cfg.reasoning_effort.trim().to_string(),
            url: format!("{}/chat/completions", base_url.trim_end_matches('/')),
            auth: auth_header(&api_key),
            model: model.to_string(),
        })
    }
}

impl LlmClient for HttpClient {
    fn stream_turn(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        cancel: &AtomicBool,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<TurnOutcome> {
        let body = ChatRequest {
            model: &self.model,
            messages,
            tools,
            stream: true,
            reasoning_effort: Some(self.reasoning_effort.as_str()).filter(|e| !e.is_empty()),
        };

        let mut req = self.agent.post(self.url.as_str());
        if let Some(auth) = &self.auth {
            req = req.header("Authorization", auth);
        }
        let mut resp = req
            .send_json(&body)
            .with_context(|| format!("POST {}", self.url))?;

        let status = resp.status();
        if !status.is_success() {
            // Bounded read: an error body should be small, and a broken
            // endpoint must not be able to stream forever into memory.
            let text = resp
                .body_mut()
                .with_config()
                .limit(64 * 1024)
                .read_to_string()
                .unwrap_or_default();
            bail!("{}", describe_error(status.as_u16(), &text));
        }

        let mut acc = TurnAccumulator::default();
        // `as_reader` is deliberately unlimited — this is the stream.
        let reader = BufReader::new(resp.body_mut().as_reader());
        for line in reader.lines() {
            if cancel.load(Ordering::Relaxed) {
                return Ok(TurnOutcome {
                    turn: acc,
                    cancelled: true,
                });
            }
            let line = line.context("read from the model stream")?;
            match parse_sse_line(&line) {
                SseLine::Ignore => continue,
                SseLine::Done => break,
                SseLine::Data(payload) => {
                    // A chunk we cannot parse is worth reporting, not silently
                    // dropping: it means the endpoint is not speaking the
                    // schema we think it is.
                    let chunk: ChatChunk = serde_json::from_str(&payload)
                        .with_context(|| format!("parse stream chunk: {payload}"))?;
                    if let Some(text) = acc.push(&chunk) {
                        on_delta(&text);
                    }
                }
            }
        }

        Ok(TurnOutcome {
            turn: acc,
            cancelled: false,
        })
    }
}

/// Turn an HTTP failure into something a person can act on.
fn describe_error(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<ErrorEnvelope>(body)
        .map(|e| match e.error.code {
            Some(code) if !code.is_empty() => format!("{} ({code})", e.error.message),
            _ => e.error.message,
        })
        .unwrap_or_else(|_| body.trim().chars().take(300).collect());

    let hint = match status {
        401 | 403 => " — check api_key in config.toml",
        404 => " — check base_url and model",
        429 => " — rate limited; try again shortly",
        500..=599 => " — the endpoint is failing",
        _ => "",
    };
    if detail.is_empty() {
        format!("model request failed: HTTP {status}{hint}")
    } else {
        format!("model request failed: HTTP {status}: {detail}{hint}")
    }
}

// ---- the scripted one, for tests ----------------------------------------

#[cfg(test)]
pub mod scripted {
    use super::*;
    use crate::agent::proto::{FunctionCall, ToolCall};
    use std::sync::Mutex;

    /// One turn the fake will produce.
    #[derive(Clone, Default)]
    pub struct ScriptedTurn {
        pub deltas: Vec<String>,
        pub tool_calls: Vec<ToolCall>,
        /// End this turn as the operator cancelling, after everything above has
        /// been accumulated. The real client can do exactly this: it checks
        /// `cancel` before each SSE line, so a cancel landing after a tool call
        /// has streamed yields `cancelled` *with* calls in the accumulator.
        pub cancelled: bool,
    }

    impl ScriptedTurn {
        pub fn text(s: &str) -> Self {
            ScriptedTurn {
                deltas: vec![s.to_string()],
                tool_calls: Vec::new(),
                cancelled: false,
            }
        }

        /// A tool call the operator cancels as it arrives.
        pub fn call_cancelled(id: &str, name: &str, arguments: serde_json::Value) -> Self {
            ScriptedTurn {
                cancelled: true,
                ..Self::call(id, name, arguments)
            }
        }

        pub fn call(id: &str, name: &str, arguments: serde_json::Value) -> Self {
            ScriptedTurn {
                deltas: Vec::new(),
                cancelled: false,
                tool_calls: vec![ToolCall {
                    id: id.to_string(),
                    kind: "function".to_string(),
                    function: FunctionCall {
                        name: name.to_string(),
                        arguments: arguments.to_string(),
                    },
                }],
            }
        }
    }

    /// Replays a fixed sequence of turns, recording what it was asked.
    pub struct ScriptedClient {
        turns: Mutex<std::collections::VecDeque<ScriptedTurn>>,
        pub seen: Mutex<Vec<Vec<Message>>>,
    }

    impl ScriptedClient {
        pub fn new(turns: Vec<ScriptedTurn>) -> Self {
            ScriptedClient {
                turns: Mutex::new(turns.into()),
                seen: Mutex::new(Vec::new()),
            }
        }

        /// The conversation as it stood on the last request — how a test checks
        /// that tool results were replayed correctly.
        pub fn last_request(&self) -> Vec<Message> {
            self.seen
                .lock()
                .unwrap()
                .last()
                .cloned()
                .unwrap_or_default()
        }

        pub fn request_count(&self) -> usize {
            self.seen.lock().unwrap().len()
        }
    }

    /// So a test can keep a handle for assertions while the worker owns one.
    impl LlmClient for std::sync::Arc<ScriptedClient> {
        fn stream_turn(
            &self,
            messages: &[Message],
            tools: &[ToolDef],
            cancel: &AtomicBool,
            on_delta: &mut dyn FnMut(&str),
        ) -> Result<TurnOutcome> {
            (**self).stream_turn(messages, tools, cancel, on_delta)
        }
    }

    impl LlmClient for ScriptedClient {
        fn stream_turn(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            cancel: &AtomicBool,
            on_delta: &mut dyn FnMut(&str),
        ) -> Result<TurnOutcome> {
            self.seen.lock().unwrap().push(messages.to_vec());
            let scripted = self
                .turns
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| ScriptedTurn::text(""));

            let mut acc = TurnAccumulator::default();
            for d in &scripted.deltas {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(TurnOutcome {
                        turn: acc,
                        cancelled: true,
                    });
                }
                // Go through the same chunk path the real client uses, so the
                // fake cannot drift from the accumulator's behaviour.
                let chunk: ChatChunk = serde_json::from_value(serde_json::json!({
                    "choices": [{"delta": {"content": d}}]
                }))
                .unwrap();
                if let Some(text) = acc.push(&chunk) {
                    on_delta(&text);
                }
            }
            for (i, call) in scripted.tool_calls.iter().enumerate() {
                // The real client checks `cancel` before every SSE line, and a
                // tool call arrives over many of them — so it can return
                // `cancelled` with calls already accumulated. Mirror that here,
                // or the fake cannot express the case that matters most.
                if cancel.load(Ordering::Relaxed) {
                    return Ok(TurnOutcome {
                        turn: acc,
                        cancelled: true,
                    });
                }
                let chunk: ChatChunk = serde_json::from_value(serde_json::json!({
                    "choices": [{"delta": {"tool_calls": [{
                        "index": i,
                        "id": call.id,
                        "function": {
                            "name": call.function.name,
                            "arguments": call.function.arguments,
                        }
                    }]}}]
                }))
                .unwrap();
                acc.push(&chunk);
            }
            Ok(TurnOutcome {
                turn: acc,
                cancelled: scripted.cancelled,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scripted::{ScriptedClient, ScriptedTurn};

    #[test]
    fn error_bodies_become_actionable_messages() {
        let msg = describe_error(
            401,
            r#"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error"}}"#,
        );
        assert!(msg.contains("HTTP 401"), "{msg}");
        assert!(msg.contains("Incorrect API key provided"), "{msg}");
        assert!(msg.contains("config.toml"), "{msg}");

        // The machine-readable code says more than the prose sometimes.
        let msg = describe_error(
            404,
            r#"{"error":{"message":"no such model","code":"model_not_found"}}"#,
        );
        assert!(
            msg.contains("model_not_found"),
            "the code is surfaced too: {msg}"
        );

        // A non-JSON body (a proxy's HTML error page, say) still says something.
        let msg = describe_error(502, "<html>bad gateway</html>");
        assert!(msg.contains("HTTP 502"), "{msg}");
        assert!(msg.contains("endpoint is failing"), "{msg}");

        let msg = describe_error(404, "");
        assert!(msg.contains("base_url"), "{msg}");
    }

    fn cfg(base_url: &str, model: &str) -> AgentConfig {
        AgentConfig {
            base_url: base_url.to_string(),
            model: model.to_string(),
            ..AgentConfig::default()
        }
    }

    #[test]
    fn a_model_is_required() {
        assert!(HttpClient::new(&cfg("https://api.openai.com/v1", "  ")).is_err());
        assert!(HttpClient::new(&cfg("https://api.openai.com/v1", "gpt-x")).is_ok());
    }

    /// A local endpoint needs no key, so the header must be omitted rather than
    /// sent empty.
    #[test]
    fn an_empty_api_key_sends_no_auth_header() {
        assert!(auth_header("").is_none());
        assert!(auth_header("   ").is_none());
        assert_eq!(auth_header("sk-1").as_deref(), Some("Bearer sk-1"));
    }

    #[test]
    fn the_url_is_joined_without_doubling_the_slash() {
        let a = HttpClient::new(&cfg("https://api.openai.com/v1", "m")).unwrap();
        let b = HttpClient::new(&cfg("https://api.openai.com/v1/", "m")).unwrap();
        assert_eq!(a.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(b.url, a.url);
    }

    /// Empty means the field is never sent, so a backend that rejects an effort
    /// string it does not know keeps working exactly as it did.
    #[test]
    fn reasoning_effort_reaches_the_client_only_when_set() {
        let c = HttpClient::new(&cfg("http://localhost:11434/v1", "qwen3.5")).unwrap();
        assert!(c.reasoning_effort.is_empty(), "not sent by default");

        let off = AgentConfig {
            reasoning_effort: "  none  ".to_string(),
            ..cfg("http://localhost:11434/v1", "qwen3.5")
        };
        let c = HttpClient::new(&off).unwrap();
        assert_eq!(
            c.reasoning_effort, "none",
            "trimmed, so a stray space is not a value"
        );
    }

    #[test]
    fn the_scripted_client_streams_text_and_records_the_request() {
        let c = ScriptedClient::new(vec![ScriptedTurn {
            deltas: vec!["Hel".into(), "lo".into()],
            tool_calls: Vec::new(),
            cancelled: false,
        }]);
        let cancel = AtomicBool::new(false);
        let mut seen = String::new();
        let out = c
            .stream_turn(&[Message::user("hi")], &[], &cancel, &mut |d| {
                seen.push_str(d)
            })
            .unwrap();
        assert_eq!(seen, "Hello");
        assert_eq!(out.turn.text, "Hello");
        assert!(!out.cancelled);
        assert_eq!(c.request_count(), 1);
        assert_eq!(c.last_request()[0].content.as_deref(), Some("hi"));
    }

    #[test]
    fn the_scripted_client_produces_tool_calls() {
        let c = ScriptedClient::new(vec![ScriptedTurn::call(
            "call_1",
            "list_hosts",
            serde_json::json!({}),
        )]);
        let cancel = AtomicBool::new(false);
        let out = c
            .stream_turn(&[Message::user("go")], &[], &cancel, &mut |_| {})
            .unwrap();
        assert!(!out.turn.tool_calls().is_empty());
        let calls = out.turn.tool_calls();
        assert_eq!(calls[0].function.name, "list_hosts");
        assert_eq!(calls[0].id, "call_1");
    }

    #[test]
    fn cancelling_mid_stream_returns_the_partial_text() {
        let c = ScriptedClient::new(vec![ScriptedTurn {
            deltas: vec!["one ".into(), "two ".into(), "three".into()],
            tool_calls: Vec::new(),
            cancelled: false,
        }]);
        let cancel = AtomicBool::new(false);
        let mut seen = String::new();
        let out = c
            .stream_turn(&[Message::user("hi")], &[], &cancel, &mut |d| {
                seen.push_str(d);
                // Ask to stop as soon as anything has arrived.
                cancel.store(true, Ordering::Relaxed);
            })
            .unwrap();
        assert!(out.cancelled);
        assert_eq!(out.turn.text, "one ", "the partial turn is kept");
    }
}
