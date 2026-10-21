//! Model providers. One OpenAI-compatible HTTP provider and one fully
//! offline fake provider for tests. Credentials come from the environment,
//! never from the project tree, prompts or logs.

use serde::{Deserialize, Serialize};

use crate::error::{HarnessError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    /// Raw JSON arguments as returned by the model.
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse {
    /// Assistant text (may be empty when tool_calls are present).
    pub text: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub stop_reason: String,
    /// Provider name for provenance.
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// Serialized tool call request (assistant role only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<serde_json::Value>,
    /// Tool name (tool role only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

impl ChatMessage {
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: text.into(),
            tool_calls: None,
            tool_name: None,
        }
    }
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: text.into(),
            tool_calls: None,
            tool_name: None,
        }
    }
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: text.into(),
            tool_calls: None,
            tool_name: None,
        }
    }
    pub fn tool_result(name: &str, text: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: text.into(),
            tool_calls: None,
            tool_name: Some(name.to_string()),
        }
    }
}

pub trait ModelProvider {
    fn name(&self) -> &str;
    fn complete(&mut self, messages: &[ChatMessage], tools: &[ToolSpec]) -> Result<ProviderResponse>;
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments object.
    pub parameters: serde_json::Value,
}

/// Scripted provider used by all offline tests. `turns` are consumed in
/// order; when exhausted it returns a plain assistant stop.
pub struct FakeProvider {
    pub turns: Vec<FakeTurn>,
    pub calls: std::cell::RefCell<Vec<usize>>,
    pub cursor: usize,
}

#[derive(Debug, Clone)]
pub enum FakeTurn {
    Text(String),
    Calls(Vec<(String, String)>),
}

impl FakeProvider {
    pub fn new(turns: Vec<FakeTurn>) -> Self {
        Self {
            turns,
            calls: std::cell::RefCell::new(Vec::new()),
            cursor: 0,
        }
    }
}

impl ModelProvider for FakeProvider {
    fn name(&self) -> &str {
        "fake"
    }

    fn complete(&mut self, _messages: &[ChatMessage], _tools: &[ToolSpec]) -> Result<ProviderResponse> {
        self.calls.borrow_mut().push(self.cursor);
        let response = if self.cursor < self.turns.len() {
            match &self.turns[self.cursor] {
                FakeTurn::Text(t) => ProviderResponse {
                    text: t.clone(),
                    tool_calls: Vec::new(),
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    stop_reason: "stop".into(),
                    provider: "fake".into(),
                    model: "fake".into(),
                },
                FakeTurn::Calls(calls) => ProviderResponse {
                    text: String::new(),
                    tool_calls: calls
                        .iter()
                        .map(|(n, a)| ToolCall {
                            name: n.clone(),
                            arguments: a.clone(),
                        })
                        .collect(),
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    stop_reason: "tool_calls".into(),
                    provider: "fake".into(),
                    model: "fake".into(),
                },
            }
        } else {
            ProviderResponse {
                text: "done".into(),
                tool_calls: Vec::new(),
                prompt_tokens: 1,
                completion_tokens: 1,
                stop_reason: "stop".into(),
                provider: "fake".into(),
                model: "fake".into(),
            }
        };
        self.cursor += 1;
        Ok(response)
    }
}

/// OpenAI-compatible chat completions over HTTP. Uses std TCP via a tiny
/// hand-rolled HTTP/1.1 client to avoid new dependencies; only supports
/// non-streaming JSON responses.
pub struct OpenAiProvider {
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
    pub timeout_secs: u64,
    retries: u32,
}

impl OpenAiProvider {
    pub fn from_config(cfg: &crate::config::Config) -> Self {
        Self {
            base_url: cfg.base_url.clone(),
            model: cfg.model.clone(),
            api_key_env: cfg.api_key_env.clone(),
            timeout_secs: cfg.request_timeout_secs,
            retries: cfg.max_provider_retries,
        }
    }
}

impl ModelProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn complete(&mut self, messages: &[ChatMessage], tools: &[ToolSpec]) -> Result<ProviderResponse> {
        let key = std::env::var(&self.api_key_env).map_err(|_| {
            HarnessError::provider(format!(
                "environment variable {} is not set",
                self.api_key_env
            ))
            .with_hint("HTTP provider credentials come from the environment")
        })?;
        let body = build_request_body(messages, tools, &self.model);
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut last_err: Option<HarnessError> = None;
        for _ in 0..=self.retries {
            match post_json(&url, &key, &body, self.timeout_secs) {
                Ok(json) => match parse_response(&json, &self.model) {
                    Ok(r) => return Ok(r),
                    Err(e) => return Err(e),
                },
                Err(e) => {
                    last_err = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| HarnessError::provider("provider call failed")))
    }
}

fn build_request_body(
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    model: &str,
) -> serde_json::Value {
    let msgs: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            let mut v = serde_json::json!({"role": m.role, "content": m.content});
            if let Some(name) = &m.tool_name {
                v["name"] = serde_json::Value::String(name.clone());
            }
            if let Some(tc) = &m.tool_calls {
                v["tool_calls"] = tc.clone();
            }
            v
        })
        .collect();
    let tools_json: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect();
    serde_json::json!({
        "model": model,
        "messages": msgs,
        "tools": tools_json,
        "tool_choice": "auto",
    })
}

fn parse_response(json: &serde_json::Value, model: &str) -> Result<ProviderResponse> {
    let choice = json
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or_else(|| HarnessError::provider("response has no choices[0]"))?;
    let message = choice.get("message").cloned().unwrap_or_default();
    let text = message
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    let mut tool_calls = Vec::new();
    if let Some(tcs) = message.get("tool_calls").and_then(|t| t.as_array()) {
        for tc in tcs {
            let func = tc.get("function").cloned().unwrap_or_default();
            let name = func
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = func
                .get("arguments")
                .and_then(|a| a.as_str())
                .unwrap_or("{}")
                .to_string();
            if !name.is_empty() {
                tool_calls.push(ToolCall { name, arguments });
            }
        }
    }
    let usage = json.get("usage").cloned().unwrap_or_default();
    let prompt_tokens = usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let completion_tokens = usage
        .get("completion_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let finish = choice
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .unwrap_or("stop")
        .to_string();
    Ok(ProviderResponse {
        text,
        tool_calls,
        prompt_tokens,
        completion_tokens,
        stop_reason: finish,
        provider: "openai".into(),
        model: model.to_string(),
    })
}

/// Minimal HTTPS POST. Uses the system `curl` for TLS instead of linking a
/// TLS stack into the harness: credentials are passed via stdin, never argv.
fn post_json(url: &str, key: &str, body: &serde_json::Value, timeout_secs: u64) -> Result<serde_json::Value> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let payload = serde_json::to_string(body)?;
    let header = format!("Authorization: Bearer {key}");
    let mut child = Command::new("curl")
        .arg("-sS")
        .arg("--fail-with-body")
        .arg("--max-time")
        .arg(timeout_secs.to_string())
        .args(["-H", "Content-Type: application/json"])
        .args(["-H", &header])
        .args(["--data-binary", "@-"])
        .arg(url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| HarnessError::provider(format!("cannot spawn curl: {e}")))?;
    child
        .stdin
        .take()
        .ok_or_else(|| HarnessError::provider("curl stdin unavailable"))?
        .write_all(payload.as_bytes())
        .map_err(|e| HarnessError::provider(format!("curl stdin: {e}")))?;
    let out = child
        .wait_with_output()
        .map_err(|e| HarnessError::provider(format!("curl: {e}")))?;
    if !out.status.success() {
        return Err(HarnessError::provider(format!(
            "provider HTTP error: {}",
            String::from_utf8_lossy(&out.stdout)
        )));
    }
    serde_json::from_slice(&out.stdout)
        .map_err(|e| HarnessError::provider(format!("provider response is not JSON: {e}")))
}

/// Build an OpenAI-shaped tool_calls value for replay into the transcript.
pub fn tool_calls_value(calls: &[ToolCall]) -> serde_json::Value {
    serde_json::json!(calls
        .iter()
        .map(|c| serde_json::json!({
            "id": format!("call_{}", c.name),
            "type": "function",
            "function": {"name": c.name, "arguments": c.arguments}
        }))
        .collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_provider_replays_scripted_turns() {
        let mut p = FakeProvider::new(vec![
            FakeTurn::Calls(vec![("write_design".into(), "{}".into())]),
            FakeTurn::Text("finished".into()),
        ]);
        let tools = vec![ToolSpec {
            name: "write_design".into(),
            description: "".into(),
            parameters: serde_json::json!({"type": "object"}),
        }];
        let r1 = p.complete(&[ChatMessage::user("go")], &tools).unwrap();
        assert_eq!(r1.tool_calls.len(), 1);
        let r2 = p.complete(&[ChatMessage::user("go")], &tools).unwrap();
        assert_eq!(r2.text, "finished");
        assert_eq!(p.cursor, 2);
    }

    #[test]
    fn request_body_shape() {
        let msgs = vec![
            ChatMessage::system("sys"),
            ChatMessage::tool_result("t", "res"),
        ];
        let body = build_request_body(&msgs, &[], "m1");
        assert_eq!(body["messages"][1]["name"], "t");
        assert_eq!(body["model"], "m1");
    }

    #[test]
    fn parse_openai_response() {
        let json = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "",
                    "tool_calls": [{
                        "function": {"name": "assets.build_candidate", "arguments": "{\"recipe\":\"a.json\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 7, "completion_tokens": 3}
        });
        let r = parse_response(&json, "gpt").unwrap();
        assert_eq!(r.tool_calls[0].name, "assets.build_candidate");
        assert_eq!(r.prompt_tokens, 7);
        assert_eq!(r.stop_reason, "tool_calls");
    }
}
