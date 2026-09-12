//! The OpenAI Chat Completions wire format.
//!
//! Chat Completions rather than the Responses API because this schema is what
//! vLLM, Ollama, llama.cpp, OpenRouter and Azure all speak — a tool holding root
//! credentials to a fleet should be able to talk to a model on the same LAN.
//!
//! Only the subset OpenAdmin uses is modelled. Every response type is tolerant
//! of unknown fields so a server that returns extras, or a newer API revision,
//! does not break deserialization.

use serde::{Deserialize, Serialize};

// ---- requests ------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [Message],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub tools: &'a [ToolDef],
    pub stream: bool,
}

/// One message in the conversation.
///
/// The same struct serves all four roles; the optional fields are what differ,
/// and every one is omitted when absent so the body matches the documented
/// shape exactly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    /// Absent on an assistant turn that is nothing but tool calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Set only on `role: "tool"`, naming the call being answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Message {
    pub fn system(text: impl Into<String>) -> Self {
        Message::plain(Role::System, text)
    }

    pub fn user(text: impl Into<String>) -> Self {
        Message::plain(Role::User, text)
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Message::plain(Role::Assistant, text)
    }

    fn plain(role: Role, text: impl Into<String>) -> Self {
        Message {
            role,
            content: Some(text.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// The assistant turn that requested tool calls. It must be replayed
    /// verbatim before the results, or the server cannot match them up.
    pub fn assistant_tool_calls(text: Option<String>, calls: Vec<ToolCall>) -> Self {
        Message {
            role: Role::Assistant,
            content: text.filter(|t| !t.is_empty()),
            tool_calls: calls,
            tool_call_id: None,
        }
    }

    /// The answer to one tool call.
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Message {
            role: Role::Tool,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// A JSON *string*, not an object — parse it, never pattern-match it.
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionDef {
    pub name: &'static str,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl ToolDef {
    pub fn function(
        name: &'static str,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        ToolDef {
            kind: "function",
            function: FunctionDef {
                name,
                description: description.into(),
                parameters,
            },
        }
    }
}

// ---- streaming responses -------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ChatChunk {
    #[serde(default)]
    pub choices: Vec<ChunkChoice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkChoice {
    #[serde(default)]
    pub delta: Delta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Delta {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallDelta>,
}

/// A fragment of a tool call. `id` and `name` arrive once, `arguments` arrives
/// in pieces that must be concatenated in order, keyed by `index`.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallDelta {
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub function: Option<FunctionCallDelta>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FunctionCallDelta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}

/// The error body a server returns instead of a stream.
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ErrorBody {
    pub message: String,
    #[serde(default)]
    pub code: Option<String>,
}

// ---- accumulating a streamed turn ---------------------------------------

/// Reassembles a stream of chunks into one assistant turn.
///
/// Tool-call arguments arrive split across arbitrarily many chunks — often
/// mid-token, sometimes mid-UTF-8-escape — so they are concatenated as bytes and
/// only parsed once the stream ends.
#[derive(Debug, Default)]
pub struct TurnAccumulator {
    pub text: String,
    calls: Vec<PartialCall>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Default, Clone)]
struct PartialCall {
    id: String,
    name: String,
    arguments: String,
}

impl TurnAccumulator {
    /// Fold one chunk in, returning the text delta it carried (if any) so the
    /// caller can stream it to the UI.
    pub fn push(&mut self, chunk: &ChatChunk) -> Option<String> {
        let mut delta_text = None;
        for choice in &chunk.choices {
            if let Some(text) = &choice.delta.content
                && !text.is_empty()
            {
                self.text.push_str(text);
                delta_text = Some(text.clone());
            }
            for call in &choice.delta.tool_calls {
                if self.calls.len() <= call.index {
                    self.calls.resize(call.index + 1, PartialCall::default());
                }
                let slot = &mut self.calls[call.index];
                if let Some(id) = &call.id {
                    slot.id.push_str(id);
                }
                if let Some(f) = &call.function {
                    if let Some(name) = &f.name {
                        slot.name.push_str(name);
                    }
                    if let Some(args) = &f.arguments {
                        slot.arguments.push_str(args);
                    }
                }
            }
            if let Some(reason) = &choice.finish_reason {
                self.finish_reason = Some(reason.clone());
            }
        }
        delta_text
    }

    pub fn has_tool_calls(&self) -> bool {
        self.calls.iter().any(|c| !c.name.is_empty())
    }

    /// The completed tool calls, in the order the model emitted them.
    pub fn tool_calls(&self) -> Vec<ToolCall> {
        self.calls
            .iter()
            .filter(|c| !c.name.is_empty())
            .map(|c| ToolCall {
                id: c.id.clone(),
                kind: "function".to_string(),
                function: FunctionCall {
                    name: c.name.clone(),
                    // An argument-less call legitimately streams nothing; the
                    // server still expects valid JSON back in the replay.
                    arguments: if c.arguments.is_empty() {
                        "{}".to_string()
                    } else {
                        c.arguments.clone()
                    },
                },
            })
            .collect()
    }

    /// The assistant message to append to the history before any tool results.
    pub fn into_message(self) -> Message {
        let calls = self.tool_calls();
        Message::assistant_tool_calls(Some(self.text), calls)
    }
}

/// One line of an SSE stream, already stripped of its `data: ` prefix.
pub enum SseLine {
    /// A chunk to feed the accumulator.
    Data(String),
    /// The `[DONE]` sentinel.
    Done,
    /// A comment, a blank keep-alive, or an event name — skip it.
    Ignore,
}

/// Classify one raw line from the stream.
pub fn parse_sse_line(line: &str) -> SseLine {
    let line = line.trim_end_matches('\r');
    let Some(payload) = line.strip_prefix("data:") else {
        // Blank lines separate events; `:` starts a comment; `event:` names the
        // event, which Chat Completions does not use.
        return SseLine::Ignore;
    };
    let payload = payload.trim_start();
    if payload == "[DONE]" {
        SseLine::Done
    } else if payload.is_empty() {
        SseLine::Ignore
    } else {
        SseLine::Data(payload.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(json: &str) -> ChatChunk {
        serde_json::from_str(json).expect("chunk should parse")
    }

    #[test]
    fn a_plain_user_message_serializes_without_the_optional_fields() {
        let json = serde_json::to_string(&Message::user("hi")).unwrap();
        assert_eq!(json, r#"{"role":"user","content":"hi"}"#);
    }

    #[test]
    fn a_tool_result_carries_its_call_id() {
        let json = serde_json::to_string(&Message::tool_result("call_1", "ok")).unwrap();
        assert_eq!(
            json,
            r#"{"role":"tool","content":"ok","tool_call_id":"call_1"}"#
        );
    }

    /// An assistant turn that only called tools has no content at all; sending
    /// `"content": ""` is not the same thing to every server.
    #[test]
    fn an_assistant_turn_of_pure_tool_calls_omits_content() {
        let m = Message::assistant_tool_calls(
            Some(String::new()),
            vec![ToolCall {
                id: "call_1".into(),
                kind: "function".into(),
                function: FunctionCall {
                    name: "list_hosts".into(),
                    arguments: "{}".into(),
                },
            }],
        );
        let json = serde_json::to_string(&m).unwrap();
        assert!(!json.contains("\"content\""), "{json}");
        assert!(json.contains(r#""name":"list_hosts""#), "{json}");
    }

    #[test]
    fn text_deltas_accumulate_and_are_reported_one_at_a_time() {
        let mut acc = TurnAccumulator::default();
        assert_eq!(
            acc.push(&chunk(r#"{"choices":[{"delta":{"content":"Hel"}}]}"#)),
            Some("Hel".to_string())
        );
        assert_eq!(
            acc.push(&chunk(r#"{"choices":[{"delta":{"content":"lo"}}]}"#)),
            Some("lo".to_string())
        );
        assert_eq!(acc.text, "Hello");
        assert!(!acc.has_tool_calls());
    }

    /// The load-bearing case: arguments arrive in fragments that mean nothing
    /// on their own and are only valid JSON once concatenated.
    #[test]
    fn tool_call_arguments_are_reassembled_from_fragments() {
        // Built with `json!` so the fragments are escaped by serde, not by hand.
        let frag_chunk = |args: &str| -> ChatChunk {
            serde_json::from_value(serde_json::json!({
                "choices": [{"delta": {"tool_calls": [
                    {"index": 0, "function": {"arguments": args}}
                ]}}]
            }))
            .unwrap()
        };

        let mut acc = TurnAccumulator::default();
        acc.push(&chunk(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_9","function":{"name":"run_readonly","arguments":""}}]}}]}"#,
        ));
        for frag in [r#"{"host"#, r#"":"web-01""#, r#","command":"ls"}"#] {
            acc.push(&frag_chunk(frag));
        }

        assert!(acc.has_tool_calls());
        let calls = acc.tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_9");
        assert_eq!(calls[0].function.name, "run_readonly");
        assert_eq!(
            calls[0].function.arguments,
            r#"{"host":"web-01","command":"ls"}"#
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&calls[0].function.arguments).expect("reassembled JSON is valid");
        assert_eq!(parsed["host"], "web-01");
        assert_eq!(parsed["command"], "ls");
    }

    /// Parallel tool calls arrive interleaved and are keyed by index, not order.
    #[test]
    fn interleaved_parallel_tool_calls_stay_separate() {
        let mut acc = TurnAccumulator::default();
        acc.push(&chunk(
            r#"{"choices":[{"delta":{"tool_calls":[
                {"index":0,"id":"a","function":{"name":"list_hosts","arguments":"{"}},
                {"index":1,"id":"b","function":{"name":"list_artifacts","arguments":"{"}}
            ]}}]}"#,
        ));
        acc.push(&chunk(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"}"}}]}}]}"#,
        ));
        acc.push(&chunk(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"}"}}]}}]}"#,
        ));
        let calls = acc.tool_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "a");
        assert_eq!(calls[0].function.name, "list_hosts");
        assert_eq!(calls[1].id, "b");
        assert_eq!(calls[1].function.name, "list_artifacts");
    }

    #[test]
    fn a_tool_call_with_no_arguments_still_replays_valid_json() {
        let mut acc = TurnAccumulator::default();
        acc.push(&chunk(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"z","function":{"name":"list_hosts"}}]}}]}"#,
        ));
        assert_eq!(acc.tool_calls()[0].function.arguments, "{}");
    }

    #[test]
    fn the_finish_reason_is_captured() {
        let mut acc = TurnAccumulator::default();
        acc.push(&chunk(r#"{"choices":[{"delta":{},"finish_reason":null}]}"#));
        assert_eq!(acc.finish_reason, None);
        acc.push(&chunk(
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
        ));
        assert_eq!(acc.finish_reason.as_deref(), Some("tool_calls"));
    }

    /// A server returning extra fields, or an empty chunk, must not break us.
    #[test]
    fn unknown_fields_and_empty_chunks_are_tolerated() {
        let mut acc = TurnAccumulator::default();
        acc.push(&chunk(
            r#"{"id":"x","object":"chat.completion.chunk","created":1,"model":"m",
                "system_fingerprint":"fp","choices":[],"usage":{"total_tokens":3}}"#,
        ));
        assert_eq!(acc.text, "");
        acc.push(&chunk(r#"{}"#));
        assert_eq!(acc.text, "");
    }

    #[test]
    fn sse_lines_are_classified() {
        assert!(matches!(parse_sse_line("data: [DONE]"), SseLine::Done));
        assert!(matches!(parse_sse_line("data:[DONE]"), SseLine::Done));
        assert!(matches!(parse_sse_line(""), SseLine::Ignore));
        assert!(matches!(parse_sse_line(": keep-alive"), SseLine::Ignore));
        assert!(matches!(parse_sse_line("event: message"), SseLine::Ignore));
        match parse_sse_line("data: {\"a\":1}\r") {
            SseLine::Data(p) => assert_eq!(p, "{\"a\":1}"),
            _ => panic!("expected data"),
        }
    }

    #[test]
    fn an_error_envelope_parses() {
        let e: ErrorEnvelope = serde_json::from_str(
            r#"{"error":{"message":"model not found","type":"invalid_request_error","code":"model_not_found"}}"#,
        )
        .unwrap();
        assert_eq!(e.error.message, "model not found");
        assert_eq!(e.error.code.as_deref(), Some("model_not_found"));
    }
}
