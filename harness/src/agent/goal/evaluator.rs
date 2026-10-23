//! Independent goal evaluator (structural port of miniagent
//! `goal/evaluator.rs`). Runs on its own `ModelProvider` (may be a cheaper
//! model than the conversation model); compiles the completion condition
//! into a checklist once, then judges yes/no per turn.
//!
//! Determinism: the evaluator prompt is plain text and the yes/no verdict is
//! parsed strictly (`yes` / `no` on the first non-empty line); anything else
//! is an evaluator error, never a "no".

use crate::agent::provider::{ChatMessage, ModelProvider};
use crate::agent::goal::models::EvalResult;
use crate::error::Result;

pub struct GoalEvaluator {
    /// Label identifying the evaluator for events ("none" when disabled).
    pub name: String,
    provider: std::cell::RefCell<Option<Box<dyn ModelProvider>>>,
    /// Budget guard: max evaluator calls before the harness gives up.
    pub max_evals: u32,
}

const SYSTEM_PROMPT: &str = "You are a strict acceptance evaluator. You judge whether an agent has met a completion condition. Answer with a single line starting with 'yes' or 'no'. When 'no', list each unmet item on its own line prefixed with '- '. Never produce anything else.";

impl GoalEvaluator {
    pub fn new(provider: Box<dyn ModelProvider>, max_evals: u32) -> Self {
        Self {
            name: provider.name().to_string(),
            provider: std::cell::RefCell::new(Some(provider)),
            max_evals,
        }
    }

    /// First pass: compile the condition into a short checklist. On any
    /// provider failure the caller keeps using the previous cache (the
    /// condition as one single criterion is the fallback).
    pub fn compile_criteria(&self, condition: &str) -> (Vec<String>, Option<String>) {
        let mut slot = self.provider.borrow_mut();
        let Some(provider) = slot.as_mut() else {
            return (vec![condition.to_string()], None);
        };
        let messages = vec![
            ChatMessage::system(SYSTEM_PROMPT),
            ChatMessage::user(format!(
                "Break this completion condition into at most 8 short, verifiable checklist items. One per line, prefixed with '- '. Condition:\n{condition}"
            )),
        ];
        match provider.complete(&messages, &[]) {
            Ok(resp) if !resp.text.trim().is_empty() => {
                let items: Vec<String> = resp
                    .text
                    .lines()
                    .map(str::trim)
                    .filter(|l| l.starts_with('-'))
                    .map(|l| l.trim_start_matches('-').trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
                if items.is_empty() {
                    // Model answered, but nothing parseable: fall back to the
                    // whole condition as one item and stop re-compiling.
                    (vec![condition.to_string()], None)
                } else {
                    (items, None)
                }
            }
            Ok(_) => (vec![condition.to_string()], None),
            // Provider errors must NOT poison the cache (miniagent P0-2).
            Err(e) => (Vec::new(), Some(e.message)),
        }
    }

    /// Judge the current transcript against the checklist.
    pub fn evaluate(
        &self,
        condition: &str,
        criteria: &[String],
        transcript: &[ChatMessage],
    ) -> Result<EvalResult> {
        let mut slot = self.provider.borrow_mut();
        let Some(provider) = slot.as_mut() else {
            return Ok(EvalResult::err("evaluator unavailable"));
        };
        let mut messages = vec![
            ChatMessage::system(SYSTEM_PROMPT),
            ChatMessage::user(format!(
                "Completion condition:\n{condition}\n\nChecklist:\n{}",
                criteria
                    .iter()
                    .map(|c| format!("- {c}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )),
        ];
        // Snapshot the tail of the conversation (bounded).
        let tail: Vec<ChatMessage> = transcript
            .iter()
            .rev()
            .take(12)
            .rev()
            .cloned()
            .collect();
        messages.extend(tail);
        let resp = provider.complete(&messages, &[])?;
        let text = resp.text.trim();
        let first = text.lines().next().unwrap_or_default().to_lowercase();
        if first.starts_with("yes") {
            return Ok(EvalResult {
                yes: true,
                error: None,
                reason: text.to_string(),
                unmet: Vec::new(),
            });
        }
        if first.starts_with("no") {
            let unmet: Vec<String> = text
                .lines()
                .skip(1)
                .map(str::trim)
                .filter(|l| l.starts_with('-'))
                .map(|l| l.trim_start_matches('-').trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            return Ok(EvalResult::no("evaluator says no", unmet));
        }
        // Unparseable output is an evaluator error, not a "no".
        Ok(EvalResult::err(format!(
            "evaluator output did not start with yes/no: {}",
            text.chars().take(120).collect::<String>()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::provider::{FakeProvider, FakeTurn};

    #[test]
    fn parses_yes_and_no_with_unmet_items() {
        let provider = FakeProvider::new(vec![
            FakeTurn::Text("yes\nall good".into()),
            FakeTurn::Text("no\n- item one missing\n- item two missing".into()),
        ]);
        let ev = GoalEvaluator::new(Box::new(provider), 10);
        let cond = "all required assets built";
        let yes = ev.evaluate(cond, &[cond.into()], &[]).unwrap();
        assert!(yes.yes);
        assert!(yes.error.is_none());
        let no = ev.evaluate(cond, &[cond.into()], &[]).unwrap();
        assert!(!no.yes);
        assert_eq!(no.unmet.len(), 2);
        assert_eq!(no.unmet[0], "item one missing");
    }

    #[test]
    fn garbage_output_is_evaluator_error() {
        let provider = FakeProvider::new(vec![FakeTurn::Text(
            "maybe? I think so probably".into(),
        )]);
        let ev = GoalEvaluator::new(Box::new(provider), 10);
        let result = ev.evaluate("c", &["c".to_string()], &[]).unwrap();
        assert!(result.error.is_some());
        assert!(!result.yes);
    }

    #[test]
    fn compile_criteria_falls_back_on_parse_failure() {
        let provider = FakeProvider::new(vec![FakeTurn::Text("cant help".into())]);
        let ev = GoalEvaluator::new(Box::new(provider), 10);
        let (items, err) = ev.compile_criteria("cond");
        assert!(err.is_none());
        assert_eq!(items, vec!["cond".to_string()]);
    }
}
