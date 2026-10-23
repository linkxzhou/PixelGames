//! Run configuration.
//!
//! Layered defaults → `<project>/harness.toml`-style JSON file → CLI flags.
//! Only std + serde_json are used, so the config file is JSON
//! (`harness.config.json`) rather than TOML to keep the dependency set small.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::agent::role::PromptLang;
use crate::error::{HarnessError, Result};

pub const CONFIG_FILE: &str = "harness.config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Path to the approved `pxl` binary. Empty means "resolve from PATH".
    pub pxl_bin: String,
    /// Required pxl version string (exact match on `pxl --version`).
    pub pxl_version: Option<String>,
    /// Optional expected digest of the pxl binary (`sha256:...`).
    pub pxl_binary_digest: Option<String>,
    /// Provider: `fake` (offline) or `openai` (OpenAI-compatible HTTP).
    pub provider: ProviderKind,
    pub model: String,
    pub base_url: String,
    /// Env var name holding the API key. Never stored in the project.
    pub api_key_env: String,
    pub request_timeout_secs: u64,
    pub max_model_turns_per_task: u32,
    pub max_fixes_per_task: u32,
    pub max_provider_retries: u32,
    pub task_wall_clock_secs: u64,
    pub pxl_timeout_secs: u64,
    pub max_tool_output_bytes: usize,
    /// Token budget per run; 0 means "no token ceiling".
    pub token_budget: u64,
    /// Request budget per run; 0 means "no request ceiling".
    pub request_budget: u64,
    /// Per-run wall clock budget in seconds; 0 = none.
    pub wall_clock_budget_secs: u64,
    /// Gate version. Bumping it invalidates verification caches.
    pub gate_version: u32,
    /// When true, design must be approved before assets run.
    pub require_design_approval: bool,
    /// Gate rules used by asset verification.
    pub visual_review: bool,
    /// Role prompt language: `en` (default) or `zh` (`roles/*_zh.md`).
    pub prompt_lang: PromptLang,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Fake,
    Openai,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pxl_bin: String::new(),
            pxl_version: Some("0.1.0".into()),
            pxl_binary_digest: None,
            provider: ProviderKind::Fake,
            model: "gpt-4o-mini".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            request_timeout_secs: 120,
            max_model_turns_per_task: 40,
            max_fixes_per_task: 3,
            max_provider_retries: 2,
            task_wall_clock_secs: 900,
            pxl_timeout_secs: 180,
            max_tool_output_bytes: 256 * 1024,
            token_budget: 0,
            request_budget: 0,
            wall_clock_budget_secs: 0,
            gate_version: 1,
            require_design_approval: true,
            visual_review: false,
            prompt_lang: PromptLang::En,
        }
    }
}

impl Config {
    pub fn load(project: &Path) -> Result<Self> {
        let path = project.join(CONFIG_FILE);
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        let parsed: Config = serde_json::from_value(value).map_err(|e| {
            HarnessError::schema(format!("{CONFIG_FILE}: {e}"))
                .with_hint("compare against `pxl-harness config --defaults`")
        })?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn validate(&self) -> Result<()> {
        if self.max_model_turns_per_task == 0 {
            return Err(HarnessError::schema("max_model_turns_per_task must be > 0"));
        }
        if self.pxl_timeout_secs == 0 {
            return Err(HarnessError::schema("pxl_timeout_secs must be > 0"));
        }
        if self.max_tool_output_bytes < 4096 {
            return Err(HarnessError::schema("max_tool_output_bytes must be >= 4096"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        Config::default().validate().unwrap();
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let v = serde_json::json!({"pxl_bin": "x", "nope": 1});
        let e = serde_json::from_value::<Config>(v).unwrap_err();
        assert!(e.to_string().contains("nope"));
    }

    #[test]
    fn prompt_lang_parses_and_defaults_to_en() {
        assert_eq!(Config::default().prompt_lang, PromptLang::En);
        let c: Config = serde_json::from_value(serde_json::json!({"prompt_lang": "zh"})).unwrap();
        assert_eq!(c.prompt_lang, PromptLang::Zh);
        assert!(serde_json::from_value::<Config>(serde_json::json!({"prompt_lang": "fr"})).is_err());
    }
}
