//! Shared contract types for the step pipeline: the frozen exit-code table,
//! the four-value verdict, and the `step.json` envelope.
//!
//! Source of the rules: `plan/v3.0/pxl-step-pipeline-plan.md` §5.
//! The verdict vocabulary follows Claude-Code-Game-Studios
//! `.claude/docs/templates/5-verdict.md` — "could not check" is never PASS.

use serde::{Deserialize, Serialize};

/// The unified error type and the frozen exit-code table, absorbed verbatim
/// from `harness/src/error.rs` in M2 (plan §5.4).
pub mod error;
pub mod knowledge;
pub mod palette_family;
pub mod plan;
pub mod policy;
pub mod prototype;
pub mod tileset;
pub mod visual;

pub use knowledge::digest_bytes;


#[cfg(test)]
mod visual_tests;

/// Frozen exit codes (plan §5.4). `step-300-asset-build` restricts itself to
/// 0-6 so old callers keep working.
pub mod exit_code {
    pub const OK: i32 = 0;
    pub const IO: i32 = 1;
    pub const USAGE: i32 = 2;
    pub const SCHEMA: i32 = 3;
    pub const SEMANTIC: i32 = 4;
    pub const GATE_FAILED: i32 = 5;
    pub const EXISTS: i32 = 6;
    pub const WAITING_APPROVAL: i32 = 10;
    pub const BLOCKED: i32 = 11;
    pub const FAILED: i32 = 12;
    pub const BUDGET_EXHAUSTED: i32 = 13;
    pub const LOCK: i32 = 14;
    pub const INTEGRITY: i32 = 15;
    pub const PROVIDER: i32 = 16;
}

/// Four-value verdict. Priority: `FAIL > CONCERNS > NOT_ASSESSED > PASS`
/// (plan §5.3). Non-empty `not_assessed` in any gate forces `NotAssessed`,
/// and `NotAssessed` outranks `Pass`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    /// Lowest priority: everything required exists and every check passed.
    Pass,
    /// Warnings a human explicitly accepted.
    Concerns,
    /// Something required was missing or could not be checked. Never a pass.
    NotAssessed,
    /// An error check failed. Highest priority.
    Fail,
}

impl Verdict {
    /// Combine two verdicts; the more severe one wins.
    pub fn and(self, other: Verdict) -> Verdict {
        fn severity(verdict: Verdict) -> u8 {
            match verdict {
                Verdict::Pass => 0,
                Verdict::NotAssessed => 1,
                Verdict::Concerns => 2,
                Verdict::Fail => 3,
            }
        }
        if severity(self) >= severity(other) {
            self
        } else {
            other
        }
    }
}

/// One `path` + `digest` row of the envelope. Paths are workspace-relative;
/// digests are `sha256:<hex>` over the file bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestEntry {
    pub path: String,
    pub digest: String,
}

/// A single gate row. `result` is four-valued like the verdict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCheck {
    pub id: String,
    pub result: GateResult,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateResult {
    Pass,
    Warning,
    Fail,
    NotAssessed,
}

impl From<GateResult> for Verdict {
    fn from(result: GateResult) -> Verdict {
        match result {
            GateResult::Pass => Verdict::Pass,
            GateResult::Warning => Verdict::Concerns,
            GateResult::Fail => Verdict::Fail,
            GateResult::NotAssessed => Verdict::NotAssessed,
        }
    }
}

/// The `step.json` envelope every step writes at the root of its `--out`
/// (plan §5.2, schema `steps/contracts/step.schema.json`). Hard rule: no
/// timestamps, no durations, no absolute paths, no host names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEnvelope {
    pub schema_version: u32,
    pub step: String,
    pub step_version: String,
    pub contract: String,
    pub status: StepStatus,
    #[serde(default = "default_scope")]
    pub scope: Scope,
    pub inputs: Vec<DigestEntry>,
    pub outputs: Vec<DigestEntry>,
    pub verdict: Verdict,
    pub gates: Gates,
    #[serde(default)]
    pub error: Option<serde_json::Value>,
}

fn default_scope() -> Scope {
    Scope::All
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    All,
    Slice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Ok,
    Draft,
    SchemaError,
    SemanticError,
    GateFailed,
    Blocked,
    WaitingApproval,
    Failed,
    BudgetExhausted,
    IntegrityError,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Gates {
    pub required_artifacts: Vec<GateCheck>,
    pub quality_checks: Vec<GateCheck>,
    pub not_assessed: Vec<String>,
}

impl Gates {
    /// Fold every row into one verdict. `not_assessed` non-empty forces
    /// `NotAssessed`; a `Fail` row outranks everything.
    pub fn verdict(&self) -> Verdict {
        let mut verdict = Verdict::Pass;
        for check in self.required_artifacts.iter().chain(&self.quality_checks) {
            verdict = verdict.and(check.result.into());
        }
        if !self.not_assessed.is_empty() {
            verdict = verdict.and(Verdict::NotAssessed);
        }
        verdict
    }
}

impl StepEnvelope {
    /// The canonical JSON text of this envelope (pretty, trailing newline).
    pub fn to_json_string(&self) -> serde_json::Result<String> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_merge_uses_the_frozen_priority_for_every_pair() {
        let verdicts = [
            Verdict::Pass,
            Verdict::NotAssessed,
            Verdict::Concerns,
            Verdict::Fail,
        ];
        let expected = [
            [Verdict::Pass, Verdict::NotAssessed, Verdict::Concerns, Verdict::Fail],
            [Verdict::NotAssessed, Verdict::NotAssessed, Verdict::Concerns, Verdict::Fail],
            [Verdict::Concerns, Verdict::Concerns, Verdict::Concerns, Verdict::Fail],
            [Verdict::Fail, Verdict::Fail, Verdict::Fail, Verdict::Fail],
        ];
        for (i, left) in verdicts.into_iter().enumerate() {
            for (j, right) in verdicts.into_iter().enumerate() {
                assert_eq!(left.and(right), expected[i][j]);
                assert_eq!(right.and(left), expected[i][j]);
                assert_eq!(left.and(Verdict::Pass).and(right), expected[i][j]);
            }
            assert_eq!(left.and(left), left);
        }
    }

    #[test]
    fn missing_required_evidence_is_not_hidden_by_accepted_concerns() {
        let gates = Gates {
            required_artifacts: vec![GateCheck {
                id: "visual-review.json".into(),
                result: GateResult::NotAssessed,
                detail: None,
            }],
            quality_checks: vec![GateCheck {
                id: "style".into(),
                result: GateResult::Warning,
                detail: None,
            }],
            not_assessed: vec!["visual-review.json".into()],
        };
        assert_eq!(gates.verdict(), Verdict::Concerns);
        assert_eq!(gates.not_assessed, vec!["visual-review.json"]);
    }

    #[test]
    fn not_assessed_rows_force_not_assessed() {
        let gates = Gates {
            required_artifacts: vec![GateCheck {
                id: "spec-manifest.json".into(),
                result: GateResult::Pass,
                detail: None,
            }],
            quality_checks: vec![],
            not_assessed: vec!["qa.palette".into()],
        };
        assert_eq!(gates.verdict(), Verdict::NotAssessed);
    }

    #[test]
    fn clean_gates_pass() {
        let gates = Gates::default();
        assert_eq!(gates.verdict(), Verdict::Pass);
    }
}
