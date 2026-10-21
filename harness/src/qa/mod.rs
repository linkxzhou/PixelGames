//! Gate runner and reports. Gates check real artifacts; agents cannot edit
//! gate results.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::digest::digest_value;
use crate::error::Result;

pub const GATE_REPORT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Passed,
    Failed,
    Blocked,
    NotRun,
}

impl GateStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            GateStatus::Passed => "passed",
            GateStatus::Failed => "failed",
            GateStatus::Blocked => "blocked",
            GateStatus::NotRun => "not_run",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCheck {
    pub id: String,
    pub passed: bool,
    pub value: String,
    pub limit: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateReport {
    pub report_version: u32,
    pub gate_version: u32,
    pub stage: String,
    pub gate_id: String,
    pub status: GateStatus,
    pub input_digest: String,
    pub checks: Vec<GateCheck>,
    #[serde(default)]
    pub metrics: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    pub tool_versions: BTreeMap<String, String>,
    pub executed_ms: u64,
}

impl GateReport {
    pub fn passed(&self) -> bool {
        self.status == GateStatus::Passed
    }

    pub fn failed_checks(&self) -> Vec<&GateCheck> {
        self.checks.iter().filter(|c| !c.passed).collect()
    }

    pub fn digest(&self) -> Result<String> {
        digest_value(self)
    }

    pub fn summary_line(&self) -> String {
        format!(
            "{} {}: {} ({}/{} checks passed)",
            self.stage,
            self.gate_id,
            self.status.as_str(),
            self.checks.iter().filter(|c| c.passed).count(),
            self.checks.len()
        )
    }
}

/// Structured failure fed back to the agent for targeted repair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateIssue {
    pub check_id: String,
    pub message: String,
}

pub fn issues_from_report(report: &GateReport) -> Vec<GateIssue> {
    report
        .failed_checks()
        .iter()
        .map(|c| GateIssue {
            check_id: c.id.clone(),
            message: if c.failures.is_empty() {
                format!("{} (value={}, limit={})", c.id, c.value, c.limit)
            } else {
                format!("{}: {}", c.id, c.failures.join("; "))
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_summary_and_issues() {
        let report = GateReport {
            report_version: GATE_REPORT_VERSION,
            gate_version: 1,
            stage: "design".into(),
            gate_id: "design.gates".into(),
            status: GateStatus::Failed,
            input_digest: "sha256:x".into(),
            checks: vec![
                GateCheck {
                    id: "ids.unique".into(),
                    passed: true,
                    value: "ok".into(),
                    limit: "unique".into(),
                    failures: vec![],
                },
                GateCheck {
                    id: "levels.spawn_in_bounds".into(),
                    passed: false,
                    value: "10,20".into(),
                    limit: "inside 16x16 grid".into(),
                    failures: vec!["level_1 spawn (10,20) outside grid".into()],
                },
            ],
            metrics: BTreeMap::new(),
            evidence: vec![],
            tool_versions: BTreeMap::new(),
            executed_ms: 5,
        };
        assert!(!report.passed());
        let issues = issues_from_report(&report);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("outside grid"));
        assert!(report.digest().unwrap().starts_with("sha256:"));
    }
}
