use std::fs;
use std::path::Path;

use pxl_contract::error::{HarnessError, Result};
use pxl_contract::knowledge::digest_bytes;
use serde_json::{json, Value};

use crate::LessonsArgs;

pub fn lessons(args: LessonsArgs) -> Result<i32> {
    match args.action.as_str() {
        "analyze" => analyze(&args.runs, &args.out),
        "audit" => audit(&args.runs, &args.out),
        "compare" => {
            let baseline = args
                .baseline
                .as_deref()
                .ok_or_else(|| HarnessError::usage("lessons compare needs --baseline"))?;
            let candidate = args
                .candidate
                .as_deref()
                .ok_or_else(|| HarnessError::usage("lessons compare needs --candidate"))?;
            compare(baseline, candidate, &args.out)
        }
        other => Err(HarnessError::usage(format!(
            "unknown lessons action '{other}'; use analyze or compare"
        ))),
    }
}

fn analyze(runs: &Path, out: &Path) -> Result<i32> {
    let mut event_files = Vec::new();
    if runs.join("events.jsonl").is_file() {
        event_files.push(runs.join("events.jsonl"));
    } else if runs.is_dir() {
        for entry in fs::read_dir(runs)? {
            let path = entry?.path().join("events.jsonl");
            if path.is_file() {
                event_files.push(path);
            }
        }
    }
    event_files.sort();
    let mut candidates = Vec::new();
    let mut diagnostics = Vec::new();
    for path in event_files {
        let bytes = fs::read(&path)?;
        let text = String::from_utf8(bytes.clone())
            .map_err(|_| HarnessError::integrity(format!("{} is not UTF-8", path.display())))?;
        let mut calls = std::collections::BTreeSet::new();
        let mut duplicate = false;
        let mut malformed = false;
        let mut qa = 0usize;
        for (index, line) in text.lines().enumerate() {
            let event: Value = match serde_json::from_str(line) {
                Ok(event) => event,
                Err(_) => {
                    diagnostics.push(json!({"type": "truncated_log", "file": path, "line": index + 1}));
                    malformed = true;
                    break;
                }
            };
            if event["event"] == "tool_started" {
                let id = event["call_id"].as_str().unwrap_or_default().to_string();
                if !id.is_empty() && !calls.insert(id) {
                    duplicate = true;
                }
            }
            if event["event"] == "tool_finished" {
                match event["error_code"].as_str().or_else(|| event["pxl_error_code"].as_str()).unwrap_or_default() {
                    "E_QA" => qa += 1,
                    "E_PROVIDER" | "E_PROVIDER_UNAVAILABLE" => diagnostics.push(json!({"type": "provider_failure", "file": path})),
                    "E_PERMISSION" | "E_PERMISSION_DENIED" => diagnostics.push(json!({"type": "permission_failure", "file": path})),
                    "E_BUDGET" | "E_BUDGET_EXHAUSTED" => diagnostics.push(json!({"type": "budget_failure", "file": path})),
                    "E_SCHEMA" => diagnostics.push(json!({"type": "schema_failure", "file": path})),
                    _ => {}
                }
            }
        }
        if duplicate {
            diagnostics.push(json!({"type": "duplicate_call", "file": path}));
        }
        if malformed || duplicate {
            continue;
        }
        if qa > 0 {
            let attempts = path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("attempts");
            let mut evidence = Vec::new();
            if attempts.is_dir() {
                for attempt in fs::read_dir(&attempts)? {
                    let manifest = attempt?.path().join("attempt-manifest.json");
                    if manifest.is_file() {
                        evidence.push(digest_bytes(&fs::read(&manifest)?));
                    }
                }
            }
            evidence.sort();
            evidence.dedup();
            if evidence.is_empty() {
                diagnostics.push(json!({"type": "missing_candidate_evidence", "file": path, "qa": qa}));
            } else {
                let mut evidence_digests = vec![digest_bytes(&bytes)];
                evidence_digests.extend(evidence);
                candidates.push(json!({
                    "contract": "lesson/v1",
                    "id": format!("run-qa-{}", candidates.len() + 1),
                    "status": "candidate",
                    "applicability": {"failures": ["pixel_qa"]},
                    "summary": "Compare localized pixel diagnostics and preserve protected regions; do not relax thresholds or infer aesthetics from provider failures.",
                    "counter_examples": ["Network, provider, permission and budget failures are not style evidence."],
                    "evidence_digests": evidence_digests,
                }));
            }
        }
    }
    write_json(
        &out.join("lesson-candidates.json"),
        &json!({
            "contract": "lesson-candidates/v1",
            "candidates": candidates,
            "diagnostics": diagnostics,
            "promotion": "manual_review_required"
        }),
    )?;
    println!("{}", serde_json::to_string(&json!({"ok": true, "cmd": "step-100 lessons", "out": out}))?);
    Ok(0)
}

fn audit(manifest: &Path, out: &Path) -> Result<i32> {
    let benchmark: Value = serde_json::from_str(
        &fs::read_to_string(manifest)
            .map_err(|e| HarnessError::io(format!("{}: {e}", manifest.display())))?,
    )
    .map_err(|e| HarnessError::schema(format!("{}: {e}", manifest.display())))?;
    if benchmark["contract"] != "visual-benchmark/v1" {
        return Err(HarnessError::schema("holdout audit needs visual-benchmark/v1"));
    }
    let holdout = benchmark["cases"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|case| case["set"] == "holdout")
        .collect::<Vec<_>>();
    if holdout.len() != 16 {
        return Err(HarnessError::integrity(format!(
            "holdout set must contain 16 independent cases, got {}",
            holdout.len()
        )));
    }
    let ids = holdout
        .iter()
        .filter_map(|case| case["id"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if ids.len() != holdout.len() {
        return Err(HarnessError::integrity("holdout case ids are not unique"));
    }
    let engineering = if holdout.iter().all(|case| case["build_verified"] == true)
        || benchmark["build_verified"] == true
    {
        "PASS"
    } else {
        "NOT_ASSESSED"
    };
    write_json(
        &out.join("holdout-report.json"),
        &json!({
            "contract": "holdout-report/v1",
            "cases": holdout,
            "engineering": engineering,
            "human_review": "NOT_ASSESSED",
            "visual_improvement": "NOT_ASSESSED",
            "statistically_significant": false
        }),
    )?;
    println!("{}", serde_json::to_string(&json!({"ok": true, "cmd": "step-100 lessons audit", "out": out}))?);
    Ok(0)
}

fn compare(baseline: &Path, candidate: &Path, out: &Path) -> Result<i32> {
    let baseline_value = read_metrics(baseline)?;
    let candidate_value = read_metrics(candidate)?;
    let mut deltas = serde_json::Map::new();
    for key in ["lint_first_pass", "repairs", "structural_failures", "cost_tokens", "review_score"] {
        let a = baseline_value[key].as_f64().unwrap_or(0.0);
        let b = candidate_value[key].as_f64().unwrap_or(0.0);
        deltas.insert(key.into(), json!(b - a));
    }
    let report = json!({
        "contract": "experiment-report/v1",
        "baseline": baseline_value,
        "candidate": candidate_value,
        "deltas": deltas,
        "holdout": "NOT_ASSESSED",
        "human_review": "NOT_ASSESSED",
        "statistically_significant": false
    });
    write_json(&out.join("experiment-report.json"), &report)?;
    println!("{}", serde_json::to_string(&json!({"ok": true, "cmd": "step-100 lessons compare", "out": out}))?);
    Ok(0)
}

fn read_metrics(path: &Path) -> Result<Value> {
    let value: Value = serde_json::from_str(
        &fs::read_to_string(path)
            .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?,
    )
    .map_err(|e| HarnessError::schema(format!("{}: {e}", path.display())))?;
    for field in ["condition", "lock"] {
        if value[field].is_null() {
            return Err(HarnessError::schema(format!(
                "{}: experiment input needs {field}",
                path.display()
            )));
        }
    }
    Ok(value)
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    fs::write(path, text).map_err(Into::into)
}
