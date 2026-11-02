//! Model providers. One OpenAI-compatible HTTP provider and one fully
//! offline fake provider for tests. Credentials come from the environment,
//! never from the project tree, prompts or logs.

use serde::{Deserialize, Serialize};

use pxl_contract::error::{HarnessError, Result};


use super::types::*;

#[derive(Debug)]
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

    /// Build turns from a JSON array where each element is an array of
    /// `[tool_name, arguments_json]` pairs. Enables scripted end-to-end
    /// runs without a real model.
    pub fn from_scripted_json(text: &str) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
            pxl_contract::error::HarnessError::usage(format!(
                "fake provider script is not JSON: {e}"
            ))
        })?;
        let mut turns = Vec::new();
        for entry in value.as_array().unwrap_or(&Vec::new()) {
            let mut calls = Vec::new();
            for call in entry.as_array().unwrap_or(&Vec::new()) {
                let name = call[0].as_str().unwrap_or_default().to_string();
                // Arguments may be given as a raw JSON string (already
                // serialized) or as an embedded JSON value; both must reach
                // the tool as a parseable arguments object.
                let args = match &call[1] {
                    serde_json::Value::String(s) => s.clone(),
                    other => serde_json::to_string(other).unwrap_or_default(),
                };
                calls.push((name, args));
            }
            turns.push(FakeTurn::Calls(calls));
        }
        Ok(Self::new(turns))
    }
}

impl ModelProvider for FakeProvider {
    fn name(&self) -> &str {
        "fake"
    }

    fn describe(&self) -> serde_json::Value {
        serde_json::json!({"provider": "fake", "model": "fake", "snapshot": null, "sampling": {}})
    }

    fn complete(
        &mut self,
        _messages: &[ChatMessage],
        _tools: &[ToolSpec],
    ) -> Result<ProviderResponse> {
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
                    snapshot: None,
                },
                FakeTurn::Calls(calls) => ProviderResponse {
                    text: String::new(),
                    tool_calls: calls
                        .iter()
                        .map(|(n, a)| ToolCall {
                            name: n.clone(),
                            arguments: a.clone(),
                            id: String::new(),
                        })
                        .collect(),
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    stop_reason: "tool_calls".into(),
                    provider: "fake".into(),
                    model: "fake".into(),
                    snapshot: None,
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
                snapshot: None,
            }
        };
        self.cursor += 1;
        Ok(response)
    }
}
