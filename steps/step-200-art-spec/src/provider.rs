//! Provider factory: fake (scripted turns) or openai (via .env).

use std::path::Path;

use pxl_agent::agent::provider::{FakeProvider, ModelProvider, OpenAiProvider};
use pxl_agent::config::{Config, ProviderKind};
use pxl_contract::error::{HarnessError, Result};

/// Environment variable holding the path to the scripted-turns JSON for the
/// fake provider: `[[["assets.write_candidate", {...}], ...], ...]`, one
/// array of calls per model turn.
pub const FAKE_TURNS_ENV: &str = "STEP_200_FAKE_TURNS";

pub fn build(kind: &str, env_path: Option<&Path>) -> Result<Box<dyn ModelProvider>> {
    match kind {
        "fake" => {
            if let Ok(script) = std::env::var(FAKE_TURNS_ENV) {
                if !script.is_empty() {
                    let text = std::fs::read_to_string(&script).map_err(|e| {
                        HarnessError::usage(format!("{FAKE_TURNS_ENV}: cannot read {script}: {e}"))
                    })?;
                    return Ok(Box::new(FakeProvider::from_scripted_json(&text)?));
                }
            }
            Err(
                HarnessError::usage("the fake provider needs scripted turns").with_hint(format!(
                    "set {FAKE_TURNS_ENV} to a JSON file of tool-call turns"
                )),
            )
        }
        "openai" => {
            let path = env_path
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from(".env"));
            let mut cfg = Config::default();
            cfg.provider = ProviderKind::Openai;
            Ok(Box::new(OpenAiProvider::from_dotenv(&cfg, &path)?))
        }
        other => Err(HarnessError::usage(format!("unknown provider '{other}'"))
            .with_hint("use fake or openai")),
    }
}

/// Adapter config for this run. The version lock stays at the workspace
/// default; `pxl_bin` overrides discovery.
pub fn adapter_config(pxl_bin: Option<&Path>) -> Config {
    let mut cfg = Config::default();
    if let Some(bin) = pxl_bin {
        cfg.pxl_bin = bin.to_string_lossy().into_owned();
    }
    cfg
}
