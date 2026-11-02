//! `step-100 migrate`: fold a legacy harness project into the step workspace.
//!
//! Legacy layout (dual-track era):
//!   <project>/revisions/design/<id>/   design package
//!   <project>/revisions/assets/<id>/   old asset tree (spec-less format)
//!   <project>/runs/                    event logs
//!
//! New layout (plan §5.5):
//!   <out>/100-design/      latest design package + pipeline.json + step.json
//!   <out>/legacy-assets/   the old asset tree, archived read-only
//!   <out>/_runs/           imported event logs
//!
//! The old asset tree has no `spec-manifest.json`, so it cannot be turned
//! into step-200 inputs mechanically; it is archived for the artist repair
//! loop instead. Idempotent: existing destination files are kept.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use pxl_contract::error::{HarnessError, Result};
use pxl_contract::{DigestEntry, GateCheck, GateResult, Gates, Scope, StepEnvelope, StepStatus};
use pxl_store::digest::digest_file;

use crate::init::{canonical_json, write_envelope};
use crate::{STEP_ID, VERSION};

pub struct MigrateArgs {
    pub from: PathBuf,
    pub out: PathBuf,
}

pub fn migrate(args: MigrateArgs) -> Result<i32> {
    let from = &args.from;
    if !from.is_dir() {
        return Err(HarnessError::usage(format!(
            "{} is not a directory",
            from.display()
        ))
        .with_hint("migrate --from takes the legacy game project directory"));
    }
    let out = &args.out;
    fs::create_dir_all(out)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", out.display())))?;

    let mut report = json!({ "schema_version": 1, "cmd": "step-100 migrate" });

    // 1. The latest design revision becomes work/100-design.
    let design_src = latest_dir(&from.join("revisions/design"));
    let design_dst = out.join("100-design");
    let design_files = match design_src {
        Some(src) => {
            let copied = copy_tree(&src, &design_dst)?;
            report["design"] = json!({
                "from": src.to_string_lossy(),
                "to": "100-design",
                "files": copied,
            });
            copied
        }
        None => {
            report["design"] = json!(null);
            Vec::new()
        }
    };

    // 2. pipeline.json + step.json so the workspace is a valid step-100 output.
    let game_id = detect_game_id(from, &design_dst);
    let pipeline_path = design_dst.join("pipeline.json");
    if !pipeline_path.is_file() {
        let pipeline = json!({
            "schema_version": 1,
            "contract_version": "pipeline/v1",
            "game_id": game_id,
            "steps": {
                "step-100-design": { "version": VERSION, "contract": "design/v1" },
                "step-200-art-spec": { "version": "0.1.0", "contract": "art-spec/v1" },
                "step-300-asset-build": { "version": "0.1.0", "contract": "asset-build/v1" },
                "step-400-package": { "version": "0.1.0", "contract": "asset-package/v1" },
                "step-500-engine": { "version": "0.1.0", "contract": "engine/v1" },
            },
            "runtime": { "name": game_id, "engine": "macroquad" },
        });
        fs::write(&pipeline_path, canonical_json(&pipeline))
            .map_err(|e| HarnessError::io(format!("pipeline.json: {e}")))?;
    }
    let step_path = design_dst.join("step.json");
    if !step_path.is_file() {
        let outputs = ["game-design.json", "asset-requirements.json", "pipeline.json"]
            .iter()
            .filter_map(|n| {
                digest_file(&design_dst.join(n)).ok().map(|d| DigestEntry {
                    path: n.to_string(),
                    digest: d,
                })
            })
            .collect::<Vec<_>>();
        let envelope = StepEnvelope {
            schema_version: 1,
            step: STEP_ID.to_string(),
            step_version: VERSION.to_string(),
            contract: "pipeline/v1".to_string(),
            status: StepStatus::Ok,
            scope: Scope::All,
            inputs: Vec::new(),
            outputs,
            verdict: pxl_contract::Verdict::Pass,
            gates: Gates {
                required_artifacts: design_files
                    .iter()
                    .map(|rel| GateCheck {
                        id: rel.clone(),
                        result: GateResult::Pass,
                        detail: None,
                    })
                    .collect(),
                quality_checks: Vec::new(),
                not_assessed: Vec::new(),
            },
            error: None,
        };
        write_envelope(&design_dst, &envelope)?;
    }

    // 3. The old asset tree is archived verbatim: it has no step-200 spec
    //    manifest and must not be mistaken for one.
    let assets_src = latest_dir(&from.join("revisions/assets"));
    if let Some(src) = assets_src {
        let copied = copy_tree(&src, &out.join("legacy-assets"))?;
        report["legacy_assets"] = json!({
            "from": src.to_string_lossy(),
            "files": copied,
        });
    } else {
        report["legacy_assets"] = json!(null);
    }

    // 4. Event logs are imported without touching their content.
    let runs_src = from.join("runs");
    if runs_src.is_dir() {
        let copied = copy_tree(&runs_src, &out.join("_runs"))?;
        report["runs"] = json!({ "files": copied });
    } else {
        report["runs"] = json!(null);
    }

    println!(
        "{}",
        serde_json::to_string(&report)
            .map_err(|e| HarnessError::io(format!("report: {e}")))?
    );
    Ok(0)
}

/// The most recent revision directory (names sort chronologically by design).
fn latest_dir(root: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.pop()
}

/// Copy a tree, skipping files that already exist at the destination.
/// Returns the number of files copied.
fn copy_tree(src: &Path, dst: &Path) -> Result<Vec<String>> {
    let mut copied = Vec::new();
    fs::create_dir_all(dst)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", dst.display())))?;
    for entry in fs::read_dir(src)
        .map_err(|e| HarnessError::io(format!("{}: {e}", src.display())))?
        .flatten()
    {
        let path = entry.path();
        let name = entry.file_name();
        let target = dst.join(&name);
        if path.is_dir() {
            let mut nested = copy_tree(&path, &target)?;
            copied.append(&mut nested);
        } else if target.is_file() {
            continue; // idempotent: keep what is already there
        } else {
            fs::copy(&path, &target).map_err(|e| {
                HarnessError::io(format!("cannot copy {}: {e}", path.display()))
            })?;
            copied.push(name.to_string_lossy().into_owned());
        }
    }
    copied.sort();
    Ok(copied)
}

fn detect_game_id(from: &Path, design: &Path) -> String {
    // Prefer the design package's own game id, then the project directory.
    if let Ok(text) = fs::read_to_string(design.join("game-design.json")) {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if let Some(id) = value["game_id"].as_str() {
                return id.to_string();
            }
        }
    }
    from.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("game")
        .to_string()
}
