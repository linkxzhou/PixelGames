//! Minimal agent loop: model turn → tool calls → results → repeat, with
//! bounded turns, budget settlement and event logging.
//!
//! v3: read-only tools in one turn run in parallel; the todo block is
//! re-injected before every model call; `run_with_goal` adds the harness
//! acceptance loop (evaluator-judged continuation, never model-judged).

use crate::agent::provider::{ChatMessage, ModelProvider, ProviderResponse, ToolSpec};
use crate::agent::tools::{ToolContext, ToolOutcome, ToolRegistry};
use crate::policy::budget::BudgetLedger;
use pxl_contract::error::{HarnessError, Result};
use pxl_store::storage::{Event, EventLog};


pub(crate) fn artifact_pairs(o: &ToolOutcome) -> Vec<(String, String)> {
    o.artifact
        .as_ref()
        .map(|a| vec![(a.path.clone(), a.digest.clone())])
        .unwrap_or_default()
}

pub(crate) fn render_outcome(o: &ToolOutcome) -> String {
    let flag = if o.ok { "ok" } else { "failed" };
    let mut text = format!("[{flag}] {}", o.summary);
    if !o.data.is_null() {
        let compact = serde_json::to_string(&o.data).unwrap_or_default();
        if !compact.is_empty() && compact != "null" {
            text.push('\n');
            text.push_str(&compact);
        }
    }
    text
}

