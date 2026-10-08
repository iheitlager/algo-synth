//! Anthropic's Messages API. The system prompt (the language and the
//! catalog) is cached: it is the same for every request, so after the first
//! the model reads it from the cache. Thinking is adaptive at an explicit
//! effort; a refusal falls back to another model on the server
//! (`fallbacks: "default"`).

use super::{Msg, Provider, ProviderError, Stop, ToolCall, ToolDef, Turn, Usage, post_json};
use serde_json::{Value, json};

pub const URL: &str = "https://api.anthropic.com/v1/messages";
/// Output room per turn: a song with its SynthDefs, and the thinking.
const MAX_TOKENS: u32 = 16_000;

pub struct Anthropic {
    pub client: reqwest::Client,
    pub url: String,
    pub key: String,
    pub model: String,
    pub effort: String,
}

/// The request body for `msgs`; the cached prefix is the tools and the
/// system prompt, both the same on every request.
pub fn body(model: &str, effort: &str, system: &str, tools: &[ToolDef], msgs: &[Msg]) -> Value {
    let tools: Vec<Value> = tools
        .iter()
        .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.schema}))
        .collect();
    let messages: Vec<Value> = msgs
        .iter()
        .map(|m| match m {
            Msg::User(text) => json!({"role": "user", "content": text}),
            Msg::Assistant(turn) => json!({"role": "assistant", "content": turn.raw}),
            Msg::Results(results) => json!({
                "role": "user",
                "content": results.iter().map(|r| json!({
                    "type": "tool_result",
                    "tool_use_id": r.id,
                    "content": r.content,
                    "is_error": r.is_error,
                })).collect::<Vec<_>>(),
            }),
        })
        .collect();
    json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "thinking": {"type": "adaptive"},
        "output_config": {"effort": effort},
        "fallbacks": "default",
        "system": [{"type": "text", "text": system, "cache_control": {"type": "ephemeral"}}],
        "tools": tools,
        "tool_choice": {"type": "auto"},
        "messages": messages,
    })
}

/// The turn in a Messages API reply.
pub fn parse(reply: &Value) -> Result<Turn, ProviderError> {
    let content = reply
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::Fatal("the reply has no content".into()))?;
    let mut text = String::new();
    let mut calls = Vec::new();
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => text.push_str(block.get("text").and_then(Value::as_str).unwrap_or("")),
            Some("tool_use") => calls.push(ToolCall {
                id: str_at(block, "id"),
                name: str_at(block, "name"),
                input: block.get("input").cloned().unwrap_or(Value::Null),
            }),
            _ => {}
        }
    }
    let stop = match reply.get("stop_reason").and_then(Value::as_str) {
        Some("tool_use") => Stop::Tools,
        Some("max_tokens") => Stop::MaxTokens,
        Some("refusal") => Stop::Refusal,
        _ => Stop::End,
    };
    let n = |k: &str| {
        reply
            .pointer(&format!("/usage/{k}"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let cached = n("cache_read_input_tokens");
    Ok(Turn {
        text,
        calls,
        stop,
        usage: Usage {
            input: n("input_tokens") + n("cache_creation_input_tokens") + cached,
            cached,
            output: n("output_tokens"),
        },
        raw: Value::Array(content.clone()),
    })
}

fn str_at(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

impl Provider for Anthropic {
    async fn turn(
        &self,
        system: &str,
        tools: &[ToolDef],
        msgs: &[Msg],
    ) -> Result<Turn, ProviderError> {
        let reply = post_json(
            &self.client,
            &self.url,
            &[
                ("x-api-key", &self.key),
                ("anthropic-version", "2023-06-01"),
                ("anthropic-beta", "server-side-fallback-2026-07-01"),
            ],
            &body(&self.model, &self.effort, system, tools, msgs),
        )
        .await?;
        parse(&reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ToolResult;

    #[test]
    fn the_body_caches_the_system_prompt_and_replays_turns_as_they_came() {
        let tools = [ToolDef {
            name: "check_song",
            description: "d",
            schema: json!({"type": "object"}),
        }];
        let raw = json!([{"type": "thinking", "thinking": "", "signature": "s"},
            {"type": "tool_use", "id": "t1", "name": "check_song", "input": {"song": "x"}}]);
        let turn = Turn {
            text: String::new(),
            calls: vec![],
            stop: Stop::Tools,
            usage: Usage::default(),
            raw: raw.clone(),
        };
        let msgs = [
            Msg::User("make it busier".into()),
            Msg::Assistant(turn),
            Msg::Results(vec![ToolResult {
                id: "t1".into(),
                name: "check_song".into(),
                content: "{}".into(),
                is_error: false,
            }]),
        ];
        let b = body("claude-opus-5-5", "high", "SPEC", &tools, &msgs);
        assert_eq!(b["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(b["system"][0]["text"], "SPEC");
        assert_eq!(b["thinking"]["type"], "adaptive");
        assert_eq!(b["output_config"]["effort"], "high");
        assert_eq!(
            b["tool_choice"]["type"], "auto",
            "no forced tool choice on Opus 5.5"
        );
        assert_eq!(b["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(
            b["messages"][1]["content"], raw,
            "the turn goes back unchanged"
        );
        assert_eq!(b["messages"][2]["content"][0]["tool_use_id"], "t1");
    }

    #[test]
    fn a_reply_parses_into_text_calls_and_usage() {
        let reply = json!({
            "content": [
                {"type": "thinking", "thinking": "", "signature": "s"},
                {"type": "text", "text": "Let me check."},
                {"type": "tool_use", "id": "t1", "name": "check_song", "input": {"song": "tempo 120"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 100, "cache_read_input_tokens": 7000, "cache_creation_input_tokens": 0, "output_tokens": 50}
        });
        let t = parse(&reply).expect("parses");
        assert_eq!(t.text, "Let me check.");
        assert_eq!(
            t.calls,
            [ToolCall {
                id: "t1".into(),
                name: "check_song".into(),
                input: json!({"song": "tempo 120"})
            }]
        );
        assert_eq!(t.stop, Stop::Tools);
        assert_eq!(
            t.usage,
            Usage {
                input: 7100,
                cached: 7000,
                output: 50
            }
        );
        assert_eq!(
            t.raw.as_array().map(Vec::len),
            Some(3),
            "thinking kept to send back"
        );
        assert_eq!(
            parse(&json!({"content": [], "stop_reason": "refusal"}))
                .expect("parses")
                .stop,
            Stop::Refusal
        );
    }
}
