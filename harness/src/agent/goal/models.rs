//! Goal state machines (structural port of miniagent `goal/models.rs`).
//! A goal is created by the harness (from a frozen contract) or by the
//! generic agent mode; the model can never mark one complete itself —
//! an independent evaluator judges the completion condition.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Paused,
    Complete,
}

impl GoalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            GoalStatus::Active => "active",
            GoalStatus::Paused => "paused",
            GoalStatus::Complete => "complete",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(GoalStatus::Active),
            "paused" => Some(GoalStatus::Paused),
            "complete" => Some(GoalStatus::Complete),
            _ => None,
        }
    }

    pub fn can_transition_to(&self, target: GoalStatus) -> bool {
        match (self, target) {
            (GoalStatus::Active, GoalStatus::Complete) => true,
            (GoalStatus::Active, GoalStatus::Paused) => true,
            (GoalStatus::Paused, GoalStatus::Active) => true,
            (GoalStatus::Paused, GoalStatus::Complete) => true,
            _ => false,
        }
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One goal tracked across turns. `objective` is what the agent is pursuing;
/// `completion_condition` is the measurable, verifier-facing condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalRecord {
    pub goal_id: String,
    pub objective: String,
    #[serde(default)]
    pub completion_condition: Option<String>,
    pub status: GoalStatus,
    #[serde(default)]
    pub criteria: Vec<String>,
    #[serde(default)]
    pub criteria_source: Option<String>,
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub history: Vec<serde_json::Value>,
    #[serde(default)]
    pub tokens_used: u64,
    #[serde(default)]
    pub time_used_seconds: u64,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub completed_at: Option<u64>,
}

impl GoalRecord {
    pub fn new(objective: impl Into<String>, completion_condition: Option<String>) -> Self {
        let now = now_unix();
        Self {
            goal_id: format!("g{now}"),
            objective: objective.into(),
            completion_condition,
            status: GoalStatus::Active,
            criteria: Vec::new(),
            criteria_source: None,
            evidence: None,
            history: Vec::new(),
            tokens_used: 0,
            time_used_seconds: 0,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }

    pub fn record_history(&mut self, to: GoalStatus, reason: &str, extra: Option<(&str, serde_json::Value)>) {
        let mut entry = serde_json::json!({
            "at": now_unix(),
            "to": to.as_str(),
            "reason": reason,
        });
        if let Some((k, v)) = extra {
            entry[k] = v;
        }
        self.history.push(entry);
        if self.history.len() > 64 {
            let start = self.history.len() - 64;
            self.history = self.history.split_off(start);
        }
    }

    /// Public view rendered into tool results (never leaks internals).
    pub fn public_view(&self) -> serde_json::Value {
        serde_json::json!({
            "goal_id": self.goal_id,
            "objective": self.objective,
            "completion_condition": self.completion_condition,
            "status": self.status.as_str(),
            "criteria": self.criteria,
            "evidence": self.evidence,
            "tokens_used": self.tokens_used,
            "time_used_seconds": self.time_used_seconds,
        })
    }
}

/// Result of one evaluator pass.
#[derive(Debug, Clone)]
pub struct EvalResult {
    pub yes: bool,
    /// True when the evaluator itself failed (timeout/parse error) — must
    /// not be conflated with a "no" verdict.
    pub error: Option<String>,
    pub reason: String,
    pub unmet: Vec<String>,
}

impl EvalResult {
    pub fn no(reason: impl Into<String>, unmet: Vec<String>) -> Self {
        Self {
            yes: false,
            error: None,
            reason: reason.into(),
            unmet,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            yes: false,
            error: Some(message.into()),
            reason: String::new(),
            unmet: Vec::new(),
        }
    }
}

/// Load / save a goal record at `<dir>/goal.json`.
pub fn load_goal(dir: &std::path::Path) -> Option<GoalRecord> {
    let path = dir.join("goal.json");
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_goal(dir: &std::path::Path, record: &GoalRecord) -> crate::error::Result<()> {
    let path = dir.join("goal.json");
    std::fs::create_dir_all(dir)
        .map_err(|e| crate::error::HarnessError::io(format!("{}: {e}", dir.display())))?;
    let tmp = dir.join("goal.json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(record)?)
        .map_err(|e| crate::error::HarnessError::io(format!("{}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| crate::error::HarnessError::io(format!("{}: {e}", path.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_transitions() {
        assert!(GoalStatus::Active.can_transition_to(GoalStatus::Complete));
        assert!(GoalStatus::Paused.can_transition_to(GoalStatus::Active));
        assert!(!GoalStatus::Complete.can_transition_to(GoalStatus::Active));
        assert_eq!(GoalStatus::parse("paused"), Some(GoalStatus::Paused));
        assert_eq!(GoalStatus::parse("bogus"), None);
    }

    #[test]
    fn goal_roundtrip_and_history_cap() {
        let dir = std::env::temp_dir().join(format!("pxl_h_goal_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = GoalRecord::new("deliver assets", Some("all required assets built".into()));
        for i in 0..80 {
            g.record_history(GoalStatus::Paused, &format!("r{i}"), None);
        }
        assert_eq!(g.history.len(), 64);
        save_goal(&dir, &g).unwrap();
        let loaded = load_goal(&dir).unwrap();
        assert_eq!(loaded.objective, "deliver assets");
        assert_eq!(loaded.status, GoalStatus::Active);
    }
}
