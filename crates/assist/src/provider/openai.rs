//! OpenAI-compatible chat completions: Mistral, OpenRouter (GLM and other
//! vendors' models through one key) and a self-hosted model (Ollama,
//! llama.cpp's server, vLLM, LM Studio) at a base URL, with or without a key.

use super::{Msg, Provider, ProviderError, Stop, ToolCall, ToolDef, Turn, Usage, post_json};
use serde_json::{Value, json};

pub struct OpenAi {
    pub client: reqwest::Client,
    /// The API's base, as `https://api.mistral.ai/v1`; `/chat/completions` is added.
    pub base: String,
    /// None for a self-hosted model that takes no key.
    pub key: Option<String>,
    pub model: String,
}

/// The request body for `msgs`.
pub fn body(model: &str, system: &str, tools: &[ToolDef], msgs: &[Msg]) -> Value {
    let tools: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({"type": "function", "function": {
                "name": t.name, "description": t.description, "parameters": t.schema,
            }})
        })
        .collect();
    let mut messages = vec![json!({"role": "system", "content": system})];
    for m in msgs {
        match m {
            Msg::User(text) => messages.push(json!({"role": "user", "content": text})),
            Msg::Assistant(turn) => messages.push(turn.raw.clone()),
            Msg::Results(results) => messages.extend(results.iter().map(|r| {
                json!({"role": "tool", "tool_call_id": r.id, "name": r.name, "content": r.content})
            })),
        }
    }
    json!({"model": model, "messages": messages, "tools": tools, "tool_choice": "auto"})
}

/// The turn in a chat completion.
pub fn parse(reply: &Value) -> Result<Turn, ProviderError> {
    let choice = reply
        .pointer("/choices/0")
        .ok_or_else(|| ProviderError::Fatal("the reply has no choice".into()))?;
    let msg = choice.get("message").cloned().unwrap_or(Value::Null);
    let text = msg
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let calls = msg
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .map(|c| {
                    let args = c.pointer("/function/arguments");
                    // Arguments are a JSON string; one that does not parse goes
                    // to the tool as it is, and the tool says so.
                    let input = match args {
                        Some(Value::String(s)) => {
                            serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone()))
                        }
                        Some(v) => v.clone(),
                        None => Value::Null,
                    };
                    ToolCall {
                        id: c
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        name: c
                            .pointer("/function/name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        input,
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let stop = match choice.get("finish_reason").and_then(Value::as_str) {
        Some("tool_calls") => Stop::Tools,
        Some("length") => Stop::MaxTokens,
        Some("content_filter") => Stop::Refusal,
        _ if !calls.is_empty() => Stop::Tools,
        _ => Stop::End,
    };
    let n = |p: &str| reply.pointer(p).and_then(Value::as_u64).unwrap_or(0);
    Ok(Turn {
        text,
        calls,
        stop,
        usage: Usage {
            input: n("/usage/prompt_tokens"),
            cached: n("/usage/prompt_tokens_details/cached_tokens"),
            output: n("/usage/completion_tokens"),
        },
        raw: msg,
    })
}

impl Provider for OpenAi {
    async fn turn(
        &self,
        system: &str,
        tools: &[ToolDef],
        msgs: &[Msg],
    ) -> Result<Turn, ProviderError> {
        let url = format!("{}/chat/completions", self.base.trim_end_matches('/'));
        let auth = self.key.as_ref().map(|k| format!("Bearer {k}"));
        let mut headers = vec![("X-Title", "algo-synth")];
        if let Some(a) = auth.as_deref() {
            headers.push(("Authorization", a));
        }
        let reply = post_json(
            &self.client,
            &url,
            &headers,
            &body(&self.model, system, tools, msgs),
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
    fn the_body_has_the_system_first_and_a_tool_message_per_result() {
        let tools = [ToolDef {
            name: "check_song",
            description: "d",
            schema: json!({"type": "object"}),
        }];
        let raw = json!({"role": "assistant", "content": null, "tool_calls": [
            {"id": "c1", "type": "function", "function": {"name": "check_song", "arguments": "{\"song\":\"x\"}"}}]});
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
                id: "c1".into(),
                name: "check_song".into(),
                content: "{}".into(),
                is_error: false,
            }]),
        ];
        let b = body("mistral-large-latest", "SPEC", &tools, &msgs);
        assert_eq!(
            b["messages"][0],
            json!({"role": "system", "content": "SPEC"})
        );
        assert_eq!(b["messages"][2], raw);
        assert_eq!(b["messages"][3]["role"], "tool");
        assert_eq!(b["messages"][3]["tool_call_id"], "c1");
        assert_eq!(b["tools"][0]["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn a_completion_parses_its_calls_even_with_bad_arguments() {
        let reply = json!({
            "choices": [{"finish_reason": "tool_calls", "message": {"role": "assistant", "content": "ok", "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "check_song", "arguments": "{\"song\":\"tempo 120\"}"}},
                {"id": "c2", "type": "function", "function": {"name": "render_song", "arguments": "{not json"}}
            ]}}],
            "usage": {"prompt_tokens": 9000, "completion_tokens": 40, "prompt_tokens_details": {"cached_tokens": 8000}}
        });
        let t = parse(&reply).expect("parses");
        assert_eq!(t.stop, Stop::Tools);
        assert_eq!(t.calls[0].input, json!({"song": "tempo 120"}));
        assert_eq!(t.calls[1].input, json!("{not json"));
        assert_eq!(
            t.usage,
            Usage {
                input: 9000,
                cached: 8000,
                output: 40
            }
        );
        let end =
            parse(&json!({"choices": [{"finish_reason": "stop", "message": {"content": "done"}}]}))
                .expect("parses");
        assert_eq!((end.stop, end.text.as_str()), (Stop::End, "done"));
    }
}
