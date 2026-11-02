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


use super::helpers::{artifact_pairs, render_outcome};


#[derive(Debug, Clone)]
pub struct AgentTask {
    pub stage: String,
    pub task_id: String,
    pub attempt: u32,
    /// Maximum model turns for this task.
    pub max_turns: u32,
    /// Maximum distinct rounds after a failed gate/repair.
    pub max_fixes: u32,
}

#[derive(Debug, Clone)]
pub struct LoopResult {
    pub turns_used: u32,
    pub stop_reason: String,
    pub final_text: String,
}

pub struct AgentRunner<'a> {
    pub provider: &'a mut dyn ModelProvider,
    pub registry: &'a ToolRegistry,
    pub ctx: &'a ToolContext,
    pub ledger: &'a mut BudgetLedger,
    pub events: &'a mut EventLog,
    /// Directory for immutable rendered tool-result texts (attempt-relative).
    /// `None` disables result persistence (kept for callers without a run dir).
    pub run_dir: Option<&'a std::path::Path>,
}

