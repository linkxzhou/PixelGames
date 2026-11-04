//! Model providers. One OpenAI-compatible HTTP provider and one fully
//! offline fake provider for tests. Credentials come from the environment,
//! never from the project tree, prompts or logs.

use serde::{Deserialize, Serialize};

use pxl_contract::error::{HarnessError, Result};


use super::types::*;

pub struct OpenAiProvider {
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
    pub timeout_secs: u64,
    pub(crate) retries: u32,
    pub(crate) api_key: Option<String>,
    pub sampling: serde_json::Value,
    last_snapshot: Option<String>,
}

impl OpenAiProvider {
    pub fn from_config(cfg: &crate::config::Config) -> Self {
        Self {
            base_url: cfg.base_url.clone(),
            model: cfg.model.clone(),
            api_key_env: cfg.api_key_env.clone(),
            timeout_secs: cfg.request_timeout_secs,
            retries: cfg.max_provider_retries,
            api_key: None,
            sampling: serde_json::json!({}),
            last_snapshot: None,
        }
    }

    pub fn from_dotenv(cfg: &crate::config::Config, path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|_| HarnessError::provider("cannot read .env provider configuration"))?;
        Self::from_env_text(cfg, &text)
    }

    pub(crate) fn from_env_text(cfg: &crate::config::Config, text: &str) -> Result<Self> {
        let mut values = std::collections::BTreeMap::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (name, value) = line.split_once('=').ok_or_else(|| {
                HarnessError::provider(format!("invalid .env assignment on line {}", index + 1))
            })?;
            let value = value.trim();
            let value = if value.starts_with(['\'', '"']) {
                let quote = value.chars().next().unwrap();
                if value.len() < 2 || !value.ends_with(quote) {
                    return Err(HarnessError::provider(format!(
                        "invalid .env quoting on line {}",
                        index + 1
                    )));
                }
                &value[1..value.len() - 1]
            } else {
                value.split(" #").next().unwrap_or(value).trim_end()
            };
            values.insert(name.trim().to_string(), value.to_string());
        }
        let required = |name: &str| -> Result<String> {
            values
                .get(name)
                .filter(|v| !v.is_empty())
                .cloned()
                .ok_or_else(|| HarnessError::provider(format!("configure {name} in .env")))
        };
        let mut provider = Self::from_config(cfg);
        provider.base_url = required("API_BASE_URL")?;
        if !provider.base_url.starts_with("https://") && !provider.base_url.starts_with("http://") {
            return Err(HarnessError::provider(
                "API_BASE_URL must use http:// or https://",
            ));
        }
        provider.api_key = Some(required("API_KEY")?);
        provider.api_key_env = "API_KEY".into();
        provider.model = required("API_MODEL")?;
        if let Some(value) = values.get("API_TIMEOUT_SECS").filter(|v| !v.is_empty()) {
            provider.timeout_secs = value
                .parse()
                .map_err(|_| HarnessError::provider("invalid API_TIMEOUT_SECS"))?;
            if provider.timeout_secs == 0 {
                return Err(HarnessError::provider("API_TIMEOUT_SECS must be positive"));
            }
        }
        if let Some(value) = values.get("API_MAX_RETRIES").filter(|v| !v.is_empty()) {
            provider.retries = value
                .parse()
                .map_err(|_| HarnessError::provider("invalid API_MAX_RETRIES"))?;
        }
        for (key, field) in [("API_TEMPERATURE", "temperature"), ("API_TOP_P", "top_p")] {
            if let Some(value) = values.get(key).filter(|v| !v.is_empty()) {
                let parsed: f64 = value
                    .parse()
                    .map_err(|_| HarnessError::provider(format!("invalid {key}")))?;
                if !parsed.is_finite() {
                    return Err(HarnessError::provider(format!("invalid {key}")));
                }
                provider.sampling[field] = serde_json::json!(parsed);
            }
        }
        if let Some(value) = values.get("API_SEED").filter(|v| !v.is_empty()) {
            provider.sampling["seed"] = serde_json::json!(value
                .parse::<u64>()
                .map_err(|_| HarnessError::provider("invalid API_SEED"))?);
        }
        Ok(provider)
    }
}

impl ModelProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn describe(&self) -> serde_json::Value {
        serde_json::json!({
            "provider": "openai",
            "model": self.model,
            "snapshot": self.last_snapshot,
            "sampling": self.sampling,
            "sampling_support": "requested; provider support is not assumed",
        })
    }

    fn complete(
        &mut self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> Result<ProviderResponse> {
        let key = self
            .api_key
            .clone()
            .map(Ok)
            .unwrap_or_else(|| std::env::var(&self.api_key_env))
            .map_err(|_| {
                HarnessError::provider(format!(
                    "environment variable {} is not set",
                    self.api_key_env
                ))
                .with_hint("HTTP provider credentials come from the environment")
            })?;
        let mut body = build_request_body(messages, tools, &self.model);
        if let Some(sampling) = self.sampling.as_object() {
            for (key, value) in sampling {
                body[key] = value.clone();
            }
        }
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut last_err: Option<HarnessError> = None;
        for _ in 0..=self.retries {
            match post_json(&url, &key, &body, self.timeout_secs) {
                Ok(json) => match parse_response(&json, &self.model) {
                    Ok(r) => {
                        self.last_snapshot = r.snapshot.clone();
                        return Ok(r);
                    }
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

pub(crate) fn build_request_body(
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
            if let Some(id) = &m.tool_call_id {
                v["tool_call_id"] = serde_json::Value::String(id.clone());
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

pub(crate) fn parse_response(json: &serde_json::Value, model: &str) -> Result<ProviderResponse> {
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
            let id = tc
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            if !name.is_empty() {
                tool_calls.push(ToolCall {
                    name,
                    arguments,
                    id,
                });
            }
        }
    }
    let usage = json.get("usage").cloned().unwrap_or_default();
    let prompt_tokens = usage
        .get("prompt_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
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
        snapshot: json
            .get("model")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

/// Minimal HTTPS POST. Uses the system `curl` for TLS instead of linking a
/// TLS stack into the harness: credentials are passed via stdin, never argv.
fn post_json(
    url: &str,
    key: &str,
    body: &serde_json::Value,
    timeout_secs: u64,
) -> Result<serde_json::Value> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let payload = serde_json::to_string(body)?;
    if key.contains(['\r', '\n']) {
        return Err(HarnessError::provider("invalid credential header"));
    }
    let header = format!("Authorization: Bearer {key}");
    let escaped_header = header.replace('\\', "\\\\").replace('"', "\\\"");
    let config = format!(
        "header = \"{escaped_header}\"\ndata = \"{}\"\n",
        payload
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    );
    let mut child = Command::new("curl")
        .arg("-q")
        .arg("-sS")
        .arg("--fail-with-body")
        .arg("--max-time")
        .arg(timeout_secs.to_string())
        .args(["-H", "Content-Type: application/json"])
        .args(["--config", "-"])
        .arg("--url")
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
        .write_all(config.as_bytes())
        .map_err(|e| HarnessError::provider(format!("curl stdin: {e}")))?;
    let out = child
        .wait_with_output()
        .map_err(|e| HarnessError::provider(format!("curl: {e}")))?;
    if !out.status.success() {
        // Keep (a bounded prefix of) the error body so callers can see why
        // the provider refused the request; never include credentials.
        let body = String::from_utf8_lossy(&out.stdout);
        let mut detail = body.trim().to_string();
        if detail.len() > 400 {
            detail.truncate(400);
            detail.push_str("...");
        }
        let detail = if detail.is_empty() {
            format!("exit {:?}", out.status.code())
        } else {
            format!("exit {:?}: {}", out.status.code(), detail)
        };
        return Err(HarnessError::provider(format!(
            "provider HTTP request failed ({detail})"
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
            "id": if c.id.is_empty() { format!("call_{}", c.name) } else { c.id.clone() },
            "type": "function",
            "function": {"name": c.name, "arguments": c.arguments}
        }))
        .collect::<Vec<_>>())
}
