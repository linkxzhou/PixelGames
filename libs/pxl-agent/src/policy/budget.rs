//! Budget ledger. Reserved before a call, settled after; persisted per run so
//! resume does not reset spending. Prices are unknown unless configured, and
//! unknown cost is never reported as 0.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use pxl_contract::error::{HarnessError, Result};

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
    /// Last event seq included in `usage` (crash-consistency anchor).
    #[serde(default)]
    pub settled_seq: u64,
    /// True when the previous session ended with started-but-unfinished calls.
    #[serde(default)]
    pub has_unknown_usage: bool,
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

    /// Apply explicit ceilings to a ledger (0 keeps "no ceiling" for that
    /// dimension). A non-zero argument never loosens a stricter limit that a
    /// resumed ledger already carries; it only tightens or fills a gap.
    pub fn set_limits(&mut self, token_limit: u64, request_limit: u64, wall_clock_secs: u64) {
        fn tighten(current: u64, wanted: u64) -> u64 {
            match (current, wanted) {
                (_, 0) => current,
                (0, wanted) => wanted,
                (current, wanted) => current.min(wanted),
            }
        }
        self.token_limit = tighten(self.token_limit, token_limit);
        self.request_limit = tighten(self.request_limit, request_limit);
        self.wall_clock_limit_ms = tighten(
            self.wall_clock_limit_ms,
            wall_clock_secs.saturating_mul(1000),
        );
    }

    /// True when at least one ceiling is active.
    pub fn has_limits(&self) -> bool {
        self.token_limit > 0 || self.request_limit > 0 || self.wall_clock_limit_ms > 0
    }

    /// Human-readable limit summary for provenance and warnings.
    pub fn limits_json(&self) -> serde_json::Value {
        serde_json::json!({
            "token_limit": self.token_limit,
            "request_limit": self.request_limit,
            "wall_clock_limit_ms": self.wall_clock_limit_ms,
            "limit_usd_cents": self.limit_usd_cents,
            "note": "0 means no ceiling for that dimension; cost in USD is unknown unless configured",
        })
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

    /// Highest event seq already accounted for in this snapshot. On resume,
    /// replay only `finished` events after it so usage is never double-counted
    /// or lost.
    pub fn settled_seq(&self) -> u64 {
        self.settled_seq
    }

    pub fn note(&mut self, text: impl Into<String>) {
        self.notes.push(text.into());
    }

    /// Atomic write (temp file + rename) bound to the last settled event seq:
    /// a crash can leave either the old or the new snapshot, never a torn one.
    pub fn save(&self, run_dir: &Path) -> Result<()> {
        let path = run_dir.join(BUDGET_FILE);
        let text = serde_json::to_string_pretty(self)?;
        atomic_write(&path, &text)
    }

    pub fn load(run_dir: &Path) -> Result<Self> {
        let path = run_dir.join(BUDGET_FILE);
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Rebuild usage from the append-only event log after `settled_seq`.
    /// Only `model_turn_finished` carries provider token usage; the copies in
    /// `candidate_written` are parent-turn duplicates and must not sum.
    pub fn reconcile_from_events(&mut self, events: &[pxl_store::storage::LoggedEvent]) {
        for ev in events {
            if ev.seq <= self.settled_seq {
                continue;
            }
            match &ev.event {
                pxl_store::storage::Event::ModelTurnFinished {
                    prompt_tokens,
                    completion_tokens,
                    elapsed_ms,
                    ..
                } => {
                    self.usage.prompt_tokens += *prompt_tokens;
                    self.usage.completion_tokens += *completion_tokens;
                    self.usage.requests += 1;
                    self.usage.elapsed_ms += *elapsed_ms;
                    self.settled_seq = ev.seq;
                }
                pxl_store::storage::Event::ToolFinished { elapsed_ms, .. } => {
                    self.usage.tool_calls += 1;
                    self.usage.elapsed_ms += *elapsed_ms;
                    self.settled_seq = ev.seq;
                }
                pxl_store::storage::Event::AutoCheckFinished { elapsed_ms, .. } => {
                    self.usage.tool_calls += 1;
                    self.usage.elapsed_ms += *elapsed_ms;
                    self.settled_seq = ev.seq;
                }
                _ => {}
            }
        }
    }

    /// Calls started but never finished: usage is unknown, never zero.
    pub fn interrupted_calls(events: &[pxl_store::storage::LoggedEvent]) -> Vec<String> {
        let mut finished = std::collections::HashSet::new();
        for ev in events {
            match &ev.event {
                pxl_store::storage::Event::ModelTurnFinished {
                    task_id,
                    turn,
                    attempt,
                    ..
                } => {
                    finished.insert(format!("model:{task_id}:{attempt}:{turn}"));
                }
                pxl_store::storage::Event::ToolFinished { tool, call_id, .. } => {
                    finished.insert(format!("tool:{tool}:{call_id}"));
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for ev in events {
            match &ev.event {
                pxl_store::storage::Event::ModelTurnStarted {
                    task_id,
                    turn,
                    attempt,
                    ..
                } => {
                    let key = format!("model:{task_id}:{attempt}:{turn}");
                    if !finished.contains(&key) {
                        out.push(format!(
                            "model turn {turn} of {task_id} attempt {attempt} (usage unknown)"
                        ));
                    }
                }
                pxl_store::storage::Event::ToolStarted { tool, call_id, .. } => {
                    let key = format!("tool:{tool}:{call_id}");
                    if !finished.contains(&key) {
                        out.push(format!("tool call {key} (usage unknown)"));
                    }
                }
                _ => {}
            }
        }
        out
    }
}

/// Write via a temp file then rename, so readers never see a partial file.
pub fn atomic_write(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| HarnessError::io(format!("{}: {e}", parent.display())))?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| HarnessError::io(format!("{}: {e}", tmp.display())))?;
    fs::rename(&tmp, path).map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
    Ok(())
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

    #[test]
    fn set_limits_fills_gaps_and_only_tightens() {
        let mut b = BudgetLedger::default();
        assert!(!b.has_limits());
        b.set_limits(1000, 0, 60);
        assert_eq!(b.token_limit, 1000);
        assert_eq!(b.request_limit, 0);
        assert_eq!(b.wall_clock_limit_ms, 60_000);
        assert!(b.has_limits());
        // A looser request never raises an existing ceiling; zero is a no-op.
        b.set_limits(5000, 10, 0);
        assert_eq!(b.token_limit, 1000);
        assert_eq!(b.request_limit, 10);
        assert_eq!(b.wall_clock_limit_ms, 60_000);
        // A stricter request tightens.
        b.set_limits(400, 0, 30);
        assert_eq!(b.token_limit, 400);
        assert_eq!(b.wall_clock_limit_ms, 30_000);
    }

    #[test]
    fn request_limit_blocks_the_next_call() {
        let mut b = BudgetLedger::default();
        b.set_limits(0, 2, 0);
        b.settle(1, 1, 1);
        assert!(b.check().is_ok());
        b.settle(1, 1, 1);
        assert!(b.check().is_err());
    }

    fn ev(seq: u64, event: pxl_store::storage::Event) -> pxl_store::storage::LoggedEvent {
        pxl_store::storage::LoggedEvent {
            seq,
            ts_ms: 0,
            event,
        }
    }

    #[test]
    fn reconcile_replays_only_after_settled_seq() {
        use pxl_store::storage::Event;
        let events = vec![
            ev(
                0,
                Event::ModelTurnFinished {
                    stage: "assets".into(),
                    task_id: "t".into(),
                    turn: 0,
                    stop_reason: "tool_calls".into(),
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    attempt: 1,
                    elapsed_ms: 100,
                },
            ),
            ev(
                1,
                Event::ToolFinished {
                    stage: "assets".into(),
                    task_id: "t".into(),
                    attempt: 1,
                    tool: "assets.build_candidate".into(),
                    ok: false,
                    exit_code: Some(5),
                    artifact_digests: vec![],
                    error_code: None,
                    call_id: "c1".into(),
                    result_digest: "sha256:r".into(),
                    elapsed_ms: 40,
                    pxl_error_code: Some("E_QA".into()),
                },
            ),
            ev(
                2,
                Event::ModelTurnFinished {
                    stage: "assets".into(),
                    task_id: "t".into(),
                    turn: 1,
                    stop_reason: "stop".into(),
                    prompt_tokens: 20,
                    completion_tokens: 7,
                    attempt: 1,
                    elapsed_ms: 200,
                },
            ),
        ];
        // A snapshot settled at seq 0 replays only seq 1 and 2.
        let mut b = BudgetLedger {
            settled_seq: 0,
            ..Default::default()
        };
        b.usage.prompt_tokens = 10;
        b.usage.completion_tokens = 5;
        b.usage.requests = 1;
        b.usage.elapsed_ms = 100;
        b.reconcile_from_events(&events);
        assert_eq!(b.usage.requests, 2, "only one new model turn");
        assert_eq!(b.usage.prompt_tokens, 30);
        assert_eq!(b.usage.completion_tokens, 12);
        assert_eq!(b.usage.tool_calls, 1);
        assert_eq!(b.usage.elapsed_ms, 340);
        assert_eq!(b.settled_seq, 2);
        // Re-running is idempotent: no double counting.
        b.reconcile_from_events(&events);
        assert_eq!(b.usage.requests, 2);
        assert_eq!(b.usage.prompt_tokens, 30);
    }

    #[test]
    fn zero_snapshot_rebuilds_full_run_and_reports_unknown() {
        use pxl_store::storage::Event;
        // Simulates the historical run: 78 finished model turns totalling
        // 1,545,488 tokens, plus one started turn with no finished event.
        let mut events = Vec::new();
        let mut seq = 0u64;
        for turn in 0..78u32 {
            events.push(ev(
                seq,
                Event::ModelTurnStarted {
                    stage: "assets".into(),
                    task_id: "asset_package".into(),
                    turn,
                    attempt: 1,
                },
            ));
            seq += 1;
            // 77 turns of 19,809 tokens plus a final turn that absorbs the
            // remainder so the total is exactly 1,545,488.
            let (prompt, completion) = if turn == 77 {
                (1_545_488 - 77 * 19_809, 0)
            } else {
                (19_053, 756)
            };
            events.push(ev(
                seq,
                Event::ModelTurnFinished {
                    stage: "assets".into(),
                    task_id: "asset_package".into(),
                    turn,
                    stop_reason: "tool_calls".into(),
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                    attempt: 1,
                    elapsed_ms: 0,
                },
            ));
            seq += 1;
        }
        // Last turn started but never finished.
        events.push(ev(
            seq,
            Event::ModelTurnStarted {
                stage: "assets".into(),
                task_id: "asset_package".into(),
                turn: 78,
                attempt: 1,
            },
        ));
        let mut b = BudgetLedger::default();
        b.reconcile_from_events(&events);
        assert_eq!(b.usage.requests, 78);
        let total = b.usage.prompt_tokens + b.usage.completion_tokens;
        assert_eq!(total, 1_545_488, "rebuilt total: {total}");
        let unknown = BudgetLedger::interrupted_calls(&events);
        assert_eq!(unknown.len(), 1, "{unknown:?}");
        assert!(unknown[0].contains("usage unknown"));
        assert!(unknown[0].contains("turn 78"));
    }

    #[test]
    fn save_is_atomic_and_survives_reload() {
        let dir = std::env::temp_dir().join(format!("pxl_h_atomic_budget_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut b = BudgetLedger::default();
        b.settle(5, 2, 11);
        b.settled_seq = 3;
        b.save(&dir).unwrap();
        let loaded = BudgetLedger::load(&dir).unwrap();
        assert_eq!(loaded.tokens(), 7);
        assert_eq!(loaded.settled_seq(), 3);
        assert!(!dir.join("budget.json.tmp").exists());
    }
}
