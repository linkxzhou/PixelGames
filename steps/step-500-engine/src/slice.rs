//! `step-500 slice`: the vertical-slice gate.
//!
//! Four-value verdict after Claude-Code-Game-Studios
//! `vertical-slice-report.md` (plan §1.4, §4.7): PROCEED / PIVOT / KILL /
//! NOT_ASSESSED, priority NOT_ASSESSED over everything but an explicit KILL.
//! Missing evidence is never a pass.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use pxl_contract::error::{HarnessError, Result};
use pxl_contract::{
    DigestEntry, GateCheck, GateResult, Gates, Scope, StepEnvelope, StepStatus, Verdict,
};
use pxl_store::digest::digest_file;

use crate::check::write_json;
use crate::{SliceArgs, STEP_ID, VERSION};

/// The artifacts the verdict needs, relative to the work root.
const REQUIRED: &[(&str, &str)] = &[
    (
        "200-art-spec/.slice/spec-manifest.json",
        "step-200 slice run",
    ),
    ("300-build/.slice/index.json", "step-300 slice build"),
    (
        "400-package/.slice/asset-package.json",
        "step-400 slice package",
    ),
    (
        "500-engine/.slice/engine-check.json",
        "step-500 slice check",
    ),
    (
        "500-engine/.slice/engine-build.json",
        "step-500 browser build",
    ),
    (
        "500-engine/.slice/playtest/report.json",
        "step-500 playtest",
    ),
];

pub fn slice(args: SliceArgs) -> Result<i32> {
    let work = &args.work;
    if !work.is_dir() {
        return Err(
            HarnessError::usage(format!("{} is not a directory", work.display()))
                .with_hint("slice --work takes the workspace root holding the .slice/ runs"),
        );
    }

    // 1. Every required artifact must exist. Missing evidence is
    //    NOT_ASSESSED, never PASS.
    let mut missing = Vec::new();
    for (rel, why) in REQUIRED {
        if !work.join(rel).is_file() {
            missing.push(format!("{rel} ({why})"));
        }
    }

    // 2. The three questions: loop playable, art scale ok, repair rounds.
    let engine_check = read_json(&work.join("500-engine/.slice/engine-check.json"));
    let build_index = read_json(&work.join("300-build/.slice/index.json"));
    let spec_manifest = read_json(&work.join("200-art-spec/.slice/spec-manifest.json"));
    let attempts = attempts_of(&work.join("400-package/.slice/attempts.jsonl"));
    let playtest = read_json(&work.join("500-engine/.slice/playtest/report.json"));
    let build_digest = digest_file(&work.join("500-engine/.slice/engine-build.json")).ok();
    let playtest_fresh = playtest
        .as_ref()
        .and_then(|report| report["build_digest"].as_str())
        .zip(build_digest.as_deref())
        .map(|(reported, actual)| reported == actual)
        .unwrap_or(false);
    if work
        .join("500-engine/.slice/playtest/report.json")
        .is_file()
        && !playtest_fresh
    {
        missing
            .push("500-engine/.slice/playtest/report.json (stale engine-build digest)".to_string());
    } else if playtest
        .as_ref()
        .map(|report| report["status"] == "NOT_ASSESSED")
        .unwrap_or(false)
    {
        missing.push("500-engine/.slice/playtest/report.json (playtest not assessed)".to_string());
    }

    let loop_playable = engine_check
        .as_ref()
        .map(|v| v["ok"] == true)
        .unwrap_or(false)
        && playtest
            .as_ref()
            .map(|report| report["status"] == "PASS")
            .unwrap_or(false)
        && playtest_fresh;
    let art_scale_ok = spec_manifest
        .as_ref()
        .map(|v| {
            v["assets"]
                .as_array()
                .map(|a| !a.is_empty() && a.iter().all(|e| e["status"] == "ok"))
                .unwrap_or(false)
        })
        .unwrap_or(false)
        && build_index
            .as_ref()
            .map(|v| {
                v["recipes"]
                    .as_array()
                    .map(|r| !r.is_empty() && r.iter().all(|e| e["status"] == "ok"))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
    let repair_rounds = attempts.len();

    let questions = json!([
        { "id": "loop.playable", "answer": loop_playable,
          "evidence": "500-engine/.slice/engine-check.json" },
        { "id": "art.scale_ok", "answer": art_scale_ok,
          "evidence": "300-build/.slice/index.json" },
        { "id": "pipeline.repair_rounds", "answer": repair_rounds,
          "evidence": "400-package/.slice/attempts.jsonl" },
    ]);

    // 3. The verdict.
    let (verdict, next) = if !missing.is_empty() {
        (
            "NOT_ASSESSED",
            "complete the slice run, then re-run step-500 slice",
        )
    } else if repair_rounds > 3 {
        // More than three repair rounds means the spec, not the pixels, is
        // wrong (plan §6 引用 19).
        ("PIVOT", "step-100 run --until gates (fix the requirements)")
    } else if loop_playable && art_scale_ok {
        ("PROCEED", "step-200 run --scope all")
    } else {
        (
            "PIVOT",
            "step-500 check --in 400-package/.slice (repair the package)",
        )
    };

    let report = json!({
        "schema_version": 1,
        "verdict": verdict,
        "questions": questions,
        "missing": missing,
        "next": next,
    });
    fs::create_dir_all(&args.out)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", args.out.display())))?;
    write_json(&args.out.join("slice-report.json"), &report)?;

    let mapped = match verdict {
        "PROCEED" => Verdict::Pass,
        "PIVOT" => Verdict::Fail,
        "KILL" => Verdict::Fail,
        _ => Verdict::NotAssessed,
    };
    let envelope = StepEnvelope {
        schema_version: 1,
        step: STEP_ID.to_string(),
        step_version: VERSION.to_string(),
        contract: "slice/v1".to_string(),
        status: match mapped {
            Verdict::Pass => StepStatus::Ok,
            Verdict::NotAssessed => StepStatus::Blocked,
            _ => StepStatus::GateFailed,
        },
        scope: Scope::Slice,
        inputs: REQUIRED
            .iter()
            .filter_map(|(rel, _)| {
                let path = work.join(rel);
                path.is_file()
                    .then(|| digest_file(&path).ok())
                    .flatten()
                    .map(|d| DigestEntry {
                        path: rel.to_string(),
                        digest: d,
                    })
            })
            .collect(),
        outputs: vec![DigestEntry {
            path: "slice-report.json".to_string(),
            digest: digest_file(&args.out.join("slice-report.json"))?,
        }],
        verdict: mapped,
        gates: Gates {
            required_artifacts: REQUIRED
                .iter()
                .map(|(rel, _)| GateCheck {
                    id: rel.to_string(),
                    result: if work.join(rel).is_file() {
                        GateResult::Pass
                    } else {
                        GateResult::NotAssessed
                    },
                    detail: None,
                })
                .collect(),
            quality_checks: Vec::new(),
            not_assessed: missing.clone(),
        },
        error: None,
    };
    write_json(
        &args.out.join("step.json"),
        &serde_json::to_value(&envelope)?,
    )?;

    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": verdict == "PROCEED",
            "cmd": "step-500 slice",
            "verdict": verdict,
            "repair_rounds": repair_rounds,
            "missing": missing,
            "next": next,
        }))?
    );
    Ok(match verdict {
        "PROCEED" => 0,
        "NOT_ASSESSED" => 11,
        _ => 5,
    })
}

fn read_json(path: &Path) -> Option<Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}

/// One line per failed attempt; an empty or absent file means zero rounds.
fn attempts_of(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .collect()
        })
        .unwrap_or_default()
}

// Keep PathBuf in the import set for future artifact globbing.
#[allow(dead_code)]
fn _p(_p: PathBuf) {}
