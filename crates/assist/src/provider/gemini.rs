//! Google's Gemini `generateContent`. The model's content goes back as it
//! came, thought signatures included.

use super::{Msg, Provider, ProviderError, Stop, ToolCall, ToolDef, Turn, Usage, post_json};
use serde_json::{Value, json};

pub const BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

pub struct Gemini {
    pub client: reqwest::Client,
    pub base: String,
    pub key: String,
    pub model: String,
}

/// The request body for `msgs`.
pub fn body(system: &str, tools: &[ToolDef], msgs: &[Msg]) -> Value {
    let decls: Vec<Value> = tools
        .iter()
        .map(|t| json!({"name": t.name, "description": t.description, "parameters": t.schema}))
        .collect();
    let contents: Vec<Value> = msgs
        .iter()
        .map(|m| match m {
            Msg::User(text) => json!({"role": "user", "parts": [{"text": text}]}),
            Msg::Assistant(turn) => turn.raw.clone(),
            Msg::Results(results) => json!({
                "role": "user",
                "parts": results.iter().map(|r| json!({"functionResponse": {
                    "name": r.name,
                    "response": {"result": r.content, "is_error": r.is_error},
                }})).collect::<Vec<_>>(),
            }),
        })
        .collect();
    json!({
        "systemInstruction": {"parts": [{"text": system}]},
        "contents": contents,
        "tools": [{"functionDeclarations": decls}],
    })
}

/// The turn in a `generateContent` reply.
pub fn parse(reply: &Value) -> Result<Turn, ProviderError> {
    let cand = reply
        .pointer("/candidates/0")
        .ok_or_else(|| ProviderError::Fatal("the reply has no candidate".into()))?;
    let content = cand
        .get("content")
        .cloned()
        .unwrap_or_else(|| json!({"role": "model", "parts": []}));
    let mut text = String::new();
    let mut calls = Vec::new();
    for (i, part) in content
        .get("parts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if part.get("thought").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if let Some(t) = part.get("text").and_then(Value::as_str) {
            text.push_str(t);
        }
        if let Some(call) = part.get("functionCall") {
            let name = call
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            calls.push(ToolCall {
                // Gemini has no call ids; the name and place stand in for one.
                id: format!("{name}-{i}"),
                name,
                input: call.get("args").cloned().unwrap_or(Value::Null),
            });
        }
    }
    let stop = match cand.get("finishReason").and_then(Value::as_str) {
        Some("MAX_TOKENS") => Stop::MaxTokens,
        Some("SAFETY" | "PROHIBITED_CONTENT" | "BLOCKLIST" | "SPII") => Stop::Refusal,
        _ if !calls.is_empty() => Stop::Tools,
        _ => Stop::End,
    };
    let n = |k: &str| {
        reply
            .pointer(&format!("/usageMetadata/{k}"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    Ok(Turn {
        text,
        calls,
        stop,
        usage: Usage {
            input: n("promptTokenCount"),
            cached: n("cachedContentTokenCount"),
            output: n("candidatesTokenCount") + n("thoughtsTokenCount"),
        },
        raw: content,
    })
}

impl Provider for Gemini {
    async fn turn(
        &self,
        system: &str,
        tools: &[ToolDef],
        msgs: &[Msg],
    ) -> Result<Turn, ProviderError> {
        let url = format!(
            "{}/models/{}:generateContent",
            self.base.trim_end_matches('/'),
            self.model
        );
        let reply = post_json(
            &self.client,
            &url,
            &[("x-goog-api-key", &self.key)],
            &body(system, tools, msgs),
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
    fn the_body_declares_functions_and_answers_them() {
        let tools = [ToolDef {
            name: "check_song",
            description: "d",
            schema: json!({"type": "object"}),
        }];
        let raw = json!({"role": "model", "parts": [{"functionCall": {"name": "check_song", "args": {"song": "x"}}, "thoughtSignature": "sig"}]});
        let turn = Turn {
            text: String::new(),
            calls: vec![],
            stop: Stop::Tools,
            usage: Usage::default(),
            raw: raw.clone(),
        };
        let msgs = [
            Msg::User("req".into()),
            Msg::Assistant(turn),
            Msg::Results(vec![ToolResult {
                id: "check_song-0".into(),
                name: "check_song".into(),
                content: "{}".into(),
                is_error: false,
            }]),
        ];
        let b = body("SPEC", &tools, &msgs);
        assert_eq!(b["systemInstruction"]["parts"][0]["text"], "SPEC");
        assert_eq!(
            b["tools"][0]["functionDeclarations"][0]["name"],
            "check_song"
        );
        assert_eq!(b["contents"][1], raw, "the signature goes back");
        assert_eq!(
            b["contents"][2]["parts"][0]["functionResponse"]["name"],
            "check_song"
        );
    }

    #[test]
    fn a_reply_parses_calls_and_skips_thoughts() {
        let reply = json!({
            "candidates": [{"finishReason": "STOP", "content": {"role": "model", "parts": [
                {"text": "thinking...", "thought": true},
                {"text": "Checking."},
                {"functionCall": {"name": "check_song", "args": {"song": "tempo 120"}}}
            ]}}],
            "usageMetadata": {"promptTokenCount": 9000, "cachedContentTokenCount": 6000, "candidatesTokenCount": 30, "thoughtsTokenCount": 70}
        });
        let t = parse(&reply).expect("parses");
        assert_eq!(t.text, "Checking.");
        assert_eq!(t.stop, Stop::Tools);
        assert_eq!(t.calls[0].input, json!({"song": "tempo 120"}));
        assert_eq!(
            t.usage,
            Usage {
                input: 9000,
                cached: 6000,
                output: 100
            }
        );
    }
}
