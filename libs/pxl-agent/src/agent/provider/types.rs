//! Model providers. One OpenAI-compatible HTTP provider and one fully
//! offline fake provider for tests. Credentials come from the environment,
//! never from the project tree, prompts or logs.

use serde::{Deserialize, Serialize};

use pxl_contract::error::{HarnessError, Result};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    /// Raw JSON arguments as returned by the model.
    pub arguments: String,
    /// Provider-assigned call id linking the tool result back to this
    /// request in the next turn.
    #[serde(default)]
    pub id: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
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
    /// Stable tool_call_id linking a tool result back to the assistant
    /// request (tool role only). OpenAI-compatible providers reject tool
    /// results without it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: text.into(),
            tool_calls: None,
            tool_name: None,
            tool_call_id: None,
        }
    }
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: text.into(),
            tool_calls: None,
            tool_name: None,
            tool_call_id: None,
        }
    }
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: text.into(),
            tool_calls: None,
            tool_name: None,
            tool_call_id: None,
        }
    }
    pub fn tool_result(name: &str, text: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: text.into(),
            tool_calls: None,
            tool_name: Some(name.to_string()),
            tool_call_id: Some(format!("call_{name}")),
        }
    }
}

pub trait ModelProvider {
    fn name(&self) -> &str;
    fn describe(&self) -> serde_json::Value {
        serde_json::json!({"provider": self.name()})
    }
    fn complete(
        &mut self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> Result<ProviderResponse>;
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments object.
    pub parameters: serde_json::Value,
}
