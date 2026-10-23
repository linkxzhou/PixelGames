//! Goal runtime (structural port of miniagent `goal/runtime.rs`):
//! per-turn accounting, evaluation gating, auto-continue decisions and the
//! evaluator failure circuit breaker. Batch-mode: debounce is always off
//! (force), continuation is bounded by `max_continuations`.

use crate::agent::goal::evaluator::GoalEvaluator;
use crate::agent::goal::models::{
    save_goal, EvalResult, GoalRecord, GoalStatus,
};
use crate::agent::provider::ChatMessage;

/// Circuit breaker: consecutive evaluator *errors* (not "no" verdicts)
/// before the goal is force-paused.
const MAX_CONSECUTIVE_EVAL_ERRORS: u32 = 3;

pub struct GoalRuntime {
    pub record: GoalRecord,
    pub evaluator: Option<GoalEvaluator>,
    /// Continuations already issued for this goal.
    pub continuation_count: u32,
    /// Hard cap on auto-continuations.
    pub max_continuations: u32,
    consecutive_eval_errors: u32,
    eval_count: u32,
    /// Persisted on every mutation when set.
    store_dir: Option<std::path::PathBuf>,
}

impl GoalRuntime {
    pub fn new(
        record: GoalRecord,
        evaluator: Option<GoalEvaluator>,
        max_continuations: u32,
        store_dir: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            record,
            evaluator,
            continuation_count: 0,
            max_continuations,
            consecutive_eval_errors: 0,
            eval_count: 0,
            store_dir,
        }
    }

    fn persist(&self) {
        if let Some(dir) = &self.store_dir {
            let _ = save_goal(dir, &self.record);
        }
    }

    /// Compile-and-cache the checklist for the completion condition.
    fn ensure_criteria(&mut self) -> Vec<String> {
        let Some(cond) = self.record.completion_condition.clone() else {
            return Vec::new();
        };
        let up_to_date = self.record.criteria_source.as_deref() == Some(cond.as_str())
            && !self.record.criteria.is_empty();
        if up_to_date {
            return self.record.criteria.clone();
        }
        let Some(evaluator) = &self.evaluator else {
            return Vec::new();
        };
        let (items, error) = evaluator.compile_criteria(&cond);
        if error.is_some() {
            // Keep the old cache; retry next turn (never poison).
            return self.record.criteria.clone();
        }
        if !items.is_empty() {
            self.record.criteria = items.clone();
            self.record.criteria_source = Some(cond);
            self.persist();
        }
        items
    }

    /// Evaluate at end of turn. `Some(result)` when an evaluation happened.
    /// yes → the goal is marked complete internally (with evidence).
    pub fn maybe_evaluate(&mut self, transcript: &[ChatMessage]) -> Option<EvalResult> {
        if self.record.status != GoalStatus::Active {
            return None;
        }
        if self.record.completion_condition.as_deref().unwrap_or("").is_empty() {
            return None;
        }
        if self.eval_count >= self.evaluator.as_ref()?.max_evals {
            return None;
        }
        let cond = self.record.completion_condition.clone()?;
        let criteria = self.ensure_criteria();
        let criteria = if criteria.is_empty() {
            vec![cond.clone()]
        } else {
            criteria
        };
        self.eval_count += 1;
        let result = self
            .evaluator
            .as_ref()?
            .evaluate(&cond, &criteria, transcript)
            .ok()?;
        if result.error.is_some() {
            self.consecutive_eval_errors += 1;
            if self.consecutive_eval_errors >= MAX_CONSECUTIVE_EVAL_ERRORS {
                self.transition(GoalStatus::Paused, "evaluator_error_circuit_breaker");
            }
            self.persist();
            return Some(result);
        }
        self.consecutive_eval_errors = 0;
        if result.yes {
            self.mark_complete(result.reason.clone());
        }
        self.persist();
        Some(result)
    }

    fn mark_complete(&mut self, evidence: String) {
        if !self.record.status.can_transition_to(GoalStatus::Complete) {
            return;
        }
        self.record.status = GoalStatus::Complete;
        self.record.evidence = Some(evidence);
        self.record.completed_at = Some(now_unix());
        self.record.updated_at = now_unix();
        self.record
            .record_history(GoalStatus::Complete, "evaluator_yes", None);
        self.persist();
    }

    fn transition(&mut self, target: GoalStatus, reason: &str) {
        let current = self.record.status;
        if current == target || !current.can_transition_to(target) {
            return;
        }
        self.record.status = target;
        self.record.updated_at = now_unix();
        self.record.record_history(target, reason, None);
        self.persist();
    }

    /// Should the runner issue another continuation turn? Batch mode: no
    /// debounce, but the continuation cap applies. Without an evaluator the
    /// condition can never be judged, so continuation is disabled (v2 loop
    /// semantics: the conversation model alone decides when to stop).
    pub fn should_auto_continue(&self) -> bool {
        self.record.status == GoalStatus::Active
            && self.continuation_count < self.max_continuations
            && self.record.completion_condition.is_some()
            && self.evaluator.is_some()
    }

    /// Prompt injected for a continuation turn.
    pub fn continuation_prompt(&self, unmet: &[String]) -> String {
        let mut text = format!(
            "The goal is NOT complete yet.\nObjective: {}\n",
            self.record.objective
        );
        if !unmet.is_empty() {
            text.push_str("Unmet acceptance items:\n");
            for item in unmet {
                text.push_str(&format!("- {item}\n"));
            }
        }
        text.push_str("Continue working toward the goal. Do not restate the plan; act on the next unmet item.");
        text
    }

    pub fn account(&mut self, tokens: u64, seconds: u64) {
        self.record.tokens_used += tokens;
        self.record.time_used_seconds += seconds;
        self.persist();
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::goal::models::GoalRecord;
    use crate::agent::provider::{FakeProvider, FakeTurn};

    fn transcript() -> Vec<ChatMessage> {
        vec![ChatMessage::user("start"), ChatMessage::assistant("working")]
    }

    #[test]
    fn yes_marks_complete_and_stops_continuation() {
        let provider = FakeProvider::new(vec![
            FakeTurn::Text("- item one".into()),
            FakeTurn::Text("yes".into()),
        ]);
        let evaluator = GoalEvaluator::new(Box::new(provider), 10);
        let mut rt = GoalRuntime::new(
            GoalRecord::new("objective", Some("cond".into())),
            Some(evaluator),
            5,
            None,
        );
        let result = rt.maybe_evaluate(&transcript()).unwrap();
        assert!(result.yes);
        assert_eq!(rt.record.status, GoalStatus::Complete);
        assert!(!rt.should_auto_continue());
        assert!(rt.record.evidence.is_some());
    }

    #[test]
    fn no_continues_with_unmet_items() {
        let provider = FakeProvider::new(vec![
            FakeTurn::Text("- item one".into()),
            FakeTurn::Text("no\n- item one".into()),
        ]);
        let evaluator = GoalEvaluator::new(Box::new(provider), 10);
        let mut rt = GoalRuntime::new(
            GoalRecord::new("objective", Some("cond".into())),
            Some(evaluator),
            5,
            None,
        );
        let result = rt.maybe_evaluate(&transcript()).unwrap();
        assert!(!result.yes);
        assert_eq!(rt.record.status, GoalStatus::Active);
        assert!(rt.should_auto_continue());
        let prompt = rt.continuation_prompt(&result.unmet);
        assert!(prompt.contains("item one"));
    }

    #[test]
    fn evaluator_error_circuit_breaker_pauses() {
        let provider = FakeProvider::new(vec![
            FakeTurn::Text("- item one".into()), // compile ok
            FakeTurn::Text("garbage".into()),     // eval error 1
            FakeTurn::Text("garbage".into()),     // eval error 2
            FakeTurn::Text("garbage".into()),     // eval error 3 → pause
        ]);
        let evaluator = GoalEvaluator::new(Box::new(provider), 10);
        let mut rt = GoalRuntime::new(
            GoalRecord::new("objective", Some("cond".into())),
            Some(evaluator),
            5,
            None,
        );
        for _ in 0..3 {
            let r = rt.maybe_evaluate(&transcript()).unwrap();
            assert!(r.error.is_some());
        }
        assert_eq!(rt.record.status, GoalStatus::Paused);
        assert!(!rt.should_auto_continue());
    }

    #[test]
    fn continuation_cap_stops_loop() {
        let provider = FakeProvider::new(vec![
            FakeTurn::Text("- item one".into()),
            FakeTurn::Text("no\n- item one".into()),
        ]);
        let evaluator = GoalEvaluator::new(Box::new(provider), 10);
        let mut rt = GoalRuntime::new(
            GoalRecord::new("objective", Some("cond".into())),
            Some(evaluator),
            1,
            None,
        );
        rt.maybe_evaluate(&transcript()).unwrap();
        rt.continuation_count = 1;
        assert!(!rt.should_auto_continue());
    }

    #[test]
    fn disabled_evaluator_is_a_noop() {
        let mut rt = GoalRuntime::new(
            GoalRecord::new("objective", Some("cond".into())),
            None,
            5,
            None,
        );
        assert!(rt.maybe_evaluate(&transcript()).is_none());
        // Without an evaluator the goal can never be judged complete, so
        // auto-continuation stays off (v2 loop semantics).
        assert!(!rt.should_auto_continue());
    }
}
