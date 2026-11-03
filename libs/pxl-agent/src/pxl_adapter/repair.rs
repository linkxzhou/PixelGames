//! Typed `PxlAdapter`. The agent never sees shell strings; it calls domain
//! tools like `assets.build_candidate` which route here.
//!
//! Contract notes verified against the real CLI:
//! - `pxl --json ...` prints one JSON object on stdout; logs go to stderr.
//! - `build` may leave outputs even when QA fails (exit 5): callers must
//!   treat the attempt dir as partial until checks pass.
//! - `schema` prints a *bare* JSON schema (no ok/error wrapper).
//! - exit codes: 1 E_IO, 2 E_USAGE, 3 E_SCHEMA, 4 E_SEMANTIC, 5 E_QA,
//!   6 E_EXISTS; timeouts / non-JSON are infrastructure errors.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::process::{run, ProcOutput, ProcSpec};
use pxl_contract::error::{HarnessError, Result};
use pxl_store::digest::{digest_file, sha256_hex};


pub(crate) fn check_failed(check: &Value) -> bool {
    check.get("pass").and_then(Value::as_bool) == Some(false)
        && !matches!(
            check.get("severity").and_then(Value::as_str),
            Some("warning" | "info")
        )
}

/// Render failed checks with full value/limit detail. The CLI prints
/// `{id, pass, severity, value, limit, samples}` objects;
/// never degrade a failure to a bare `"{id}: failed"`.
pub fn render_failed_checks(checks: &[Value], max: usize) -> Vec<String> {
    checks
        .iter()
        .filter(|c| check_failed(c))
        .take(max)
        .map(|c| {
            let id = c.get("id").and_then(|v| v.as_str()).unwrap_or("check");
            let value = c.get("value");
            let limit = c.get("limit");
            let value = match value {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            };
            match limit {
                Some(Value::String(l)) => format!("{id}: value={value}, limit={l}"),
                Some(other) if !other.is_null() => {
                    format!("{id}: value={value}, limit={}", other)
                }
                _ => format!("{id}: value={value}"),
            }
        })
        .collect()
}

/// Map pxl error codes onto the harness repair taxonomy.
pub fn repair_class(error_code: &str) -> RepairClass {
    match error_code {
        "E_USAGE" | "E_SCHEMA" => RepairClass::CandidateRevision,
        "E_SEMANTIC" => RepairClass::CandidateRevision,
        "E_QA" => RepairClass::AssetFix,
        "E_EXISTS" => RepairClass::IsolationConflict,
        "E_IO" => RepairClass::Environment,
        _ => RepairClass::Infrastructure,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairClass {
    /// Fix the recipe/spec content (usage/schema/semantic).
    CandidateRevision,
    /// Fix pixel content until QA passes.
    AssetFix,
    /// Check directory isolation or resume reconciliation.
    IsolationConflict,
    /// Environment / permissions: report, no silent retry.
    Environment,
    /// Timeout, crash, non-JSON: infrastructure error.
    Infrastructure,
}
