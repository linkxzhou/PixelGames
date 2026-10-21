//! Budget ledger. Reserved before a call, settled after; persisted per run so
//! resume does not reset spending. Prices are unknown unless configured, and
//! unknown cost is never reported as 0.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{HarnessError, Result};

pub const BUDGET_FILE: &str = "budget.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub requests: u64,
    pub tool_calls: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BudgetLedger {
    #[serde(default)]
    pub usage: Usage,
    #[serde(default)]
    pub token_limit: u64,
    #[serde(default)]
    pub request_limit: u64,
    #[serde(default)]
    pub wall_clock_limit_ms: u64,
    #[serde(default)]
    pub spent_usd_cents: Option<u64>,
    #[serde(default)]
    pub limit_usd_cents: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl BudgetLedger {
    pub fn from_config(cfg: &crate::config::Config) -> Self {
        Self {
            token_limit: cfg.token_budget,
            request_limit: cfg.request_budget,
            wall_clock_limit_ms: cfg.wall_clock_budget_secs.saturating_mul(1000),
            ..Self::default()
        }
    }

    pub fn tokens(&self) -> u64 {
        self.usage.prompt_tokens + self.usage.completion_tokens
    }

    /// Hard check before a provider call.
    pub fn check(&self) -> Result<()> {
        if self.token_limit > 0 && self.tokens() >= self.token_limit {
            return Err(HarnessError::budget(format!(
                "token budget exhausted: {} >= {}",
                self.tokens(),
                self.token_limit
            ))
            .with_hint("resume with a raised token_budget or reduce scope"));
        }
        if self.request_limit > 0 && self.usage.requests >= self.request_limit {
            return Err(HarnessError::budget(format!(
                "request budget exhausted: {} >= {}",
                self.usage.requests, self.request_limit
            )));
        }
        if self.wall_clock_limit_ms > 0 && self.usage.elapsed_ms >= self.wall_clock_limit_ms {
            return Err(HarnessError::budget("wall-clock budget exhausted"));
        }
        if let (Some(spent), Some(limit)) = (self.spent_usd_cents, self.limit_usd_cents) {
            if spent >= limit {
                return Err(HarnessError::budget(format!(
                    "cost limit reached: {spent} >= {limit} cents"
                )));
            }
        }
        Ok(())
    }

    pub fn settle(&mut self, prompt: u64, completion: u64, elapsed_ms: u64) {
        self.usage.prompt_tokens += prompt;
        self.usage.completion_tokens += completion;
        self.usage.requests += 1;
        self.usage.elapsed_ms += elapsed_ms;
    }

    pub fn settle_tool(&mut self, elapsed_ms: u64) {
        self.usage.tool_calls += 1;
        self.usage.elapsed_ms += elapsed_ms;
    }

    pub fn save(&self, run_dir: &Path) -> Result<()> {
        let path = run_dir.join(BUDGET_FILE);
        let text = serde_json::to_string_pretty(self)?;
        fs::write(&path, text).map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
        Ok(())
    }

    pub fn load(run_dir: &Path) -> Result<Self> {
        let path = run_dir.join(BUDGET_FILE);
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&text)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_limit_blocks_and_persists() {
        let dir = std::env::temp_dir().join(format!("pxl_h_budget_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut b = BudgetLedger {
            token_limit: 100,
            ..Default::default()
        };
        b.settle(60, 30, 5);
        b.save(&dir).unwrap();
        let mut loaded = BudgetLedger::load(&dir).unwrap();
        assert_eq!(loaded.tokens(), 90);
        assert!(loaded.check().is_ok());
        loaded.settle(10, 0, 0);
        assert_eq!(loaded.tokens(), 100);
        loaded.save(&dir).unwrap();
        let full = BudgetLedger::load(&dir).unwrap();
        assert!(full.check().is_err());
    }
}
