//! The language-model providers behind one trait (ADR-0028): three wire
//! formats for five providers. A provider takes the system prompt, the
//! tools and the conversation and returns the model's next turn; the loop
//! (`crate::assist`) runs the tools and calls it again.
//!
//! Each assistant turn keeps the provider's own content (`Turn::raw`), sent
//! back unchanged on the next call: thinking blocks and thought signatures
//! must come back as they went out.

pub mod anthropic;
pub mod gemini;
pub mod openai;

use serde::Serialize;
use serde_json::Value;
use std::future::Future;
use std::time::Duration;

/// A tool the model may call: a name, what it does and its JSON schema.
#[derive(Clone, Debug, Serialize)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

/// A call the model made.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// What a tool answered, for the call with `id`.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub id: String,
    pub name: String,
    pub content: String,
    pub is_error: bool,
}

/// Why a turn ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The model is done for now.
    End,
    /// It called tools.
    Tools,
    /// It ran out of output tokens.
    MaxTokens,
    /// A safety classifier declined.
    Refusal,
}

/// Tokens a turn used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input: u64,
    pub cached: u64,
    pub output: u64,
}

impl std::ops::AddAssign for Usage {
    fn add_assign(&mut self, o: Usage) {
        self.input += o.input;
        self.cached += o.cached;
        self.output += o.output;
    }
}

/// The model's turn: its text, its tool calls, why it stopped, what it
/// used, and its content in the provider's own form, to send back.
#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    pub text: String,
    pub calls: Vec<ToolCall>,
    pub stop: Stop,
    pub usage: Usage,
    pub raw: Value,
}

/// One message of the conversation.
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    User(String),
    Assistant(Turn),
    Results(Vec<ToolResult>),
}

/// A provider call that failed.
#[derive(Clone, Debug, PartialEq)]
pub enum ProviderError {
    /// Worth a retry: rate limited, overloaded, a server error, a timeout.
    Transient(String),
    /// Not: a bad request, a bad key, an unknown model.
    Fatal(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Transient(m) | ProviderError::Fatal(m) => f.write_str(m),
        }
    }
}

/// A language model behind one interface.
pub trait Provider: Sync {
    /// The model's next turn.
    fn turn(
        &self,
        system: &str,
        tools: &[ToolDef],
        msgs: &[Msg],
    ) -> impl Future<Output = Result<Turn, ProviderError>> + Send;
}

/// How long one provider call may take.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(300);

/// POST `body` as JSON with `headers`; the reply as JSON, or a typed error
/// that never carries a header (the keys travel there).
pub(crate) async fn post_json(
    client: &reqwest::Client,
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> Result<Value, ProviderError> {
    let mut req = client.post(url).timeout(CALL_TIMEOUT).json(body);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let resp = req.send().await.map_err(|e| {
        let what = if e.is_timeout() {
            "timed out"
        } else {
            "could not connect"
        };
        ProviderError::Transient(format!("the provider {what}"))
    })?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|_| ProviderError::Transient("the provider's reply broke off".into()))?;
    if !status.is_success() {
        // The provider's own message, short: it names the model or the field.
        let detail: String = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .or_else(|| v.pointer("/message"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default()
            .chars()
            .take(300)
            .collect();
        let msg = format!("the provider answered {status}: {detail}");
        return Err(if status.as_u16() == 429 || status.is_server_error() {
            ProviderError::Transient(msg)
        } else {
            ProviderError::Fatal(msg)
        });
    }
    serde_json::from_str(&text)
        .map_err(|_| ProviderError::Fatal("the provider's reply is not JSON".into()))
}
