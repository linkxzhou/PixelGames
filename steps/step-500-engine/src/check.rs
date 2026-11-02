//! `step-500 check`: seven machine-checkable assertions an asset-package and
//! engine/v4 capability sheet must pass before the runtime may load it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use pxl_contract::error::{HarnessError, Result};
use pxl_contract::{
    DigestEntry, GateCheck, GateResult, Gates, Scope, StepEnvelope, StepStatus, Verdict,
};
use pxl_store::digest::digest_file;

use crate::{CheckArgs, STEP_ID, VERSION};

struct Check {
    id: &'static str,
    result: GateResult,
    detail: Option<String>,
}

pub fn check(args: CheckArgs) -> Result<i32> {
    let dir = &args.input;
    let out = args.out.clone().unwrap_or_else(|| dir.clone());
    let package_text = fs::read_to_string(dir.join("asset-package.json")).map_err(|e| {
        HarnessError::io(format!("asset-package.json: {e}")).with_hint("run `step-400 run` first")
    })?;
    let package: Value = serde_json::from_str(&package_text)
        .map_err(|e| HarnessError::schema(format!("asset-package.json: {e}")))?;
    let package_version = package["schema_version"].as_u64().unwrap_or(0);
    // v4 adds compiled scenes, the resource map and release states; v3
    // packages stay readable until they are regenerated.
    if package_version != 3 && package_version != 4 {
        return Err(HarnessError::schema(format!(
            "unsupported asset-package schema_version {package_version}; regenerate it with asset-package/v4"
        )));
    }
    let engine_text = match &args.engine {
        Some(path) => {
            fs::read_to_string(path).map_err(|e| HarnessError::io(format!("engine.json: {e}")))?
        }
        None => crate::ENGINE_JSON.to_string(),
    };
    let engine: Value = serde_json::from_str(&engine_text)
        .map_err(|e| HarnessError::schema(format!("engine.json: {e}")))?;

    let mut checks = Vec::new();
    let mut verdict = Verdict::Pass;

    // 1. Every sheet the package names must exist with the recorded digest.
    let mut sheet_docs: Vec<(String, Value)> = Vec::new();
    let mut missing = Vec::new();
    let mut mismatched = Vec::new();
    for sheet in package["sheets"].as_array().cloned().unwrap_or_default() {
        let id = sheet["id"].as_str().unwrap_or_default().to_string();
        let rel = sheet["path"].as_str().unwrap_or_default();
        let expected = sheet["digest"].as_str().unwrap_or_default();
        let path = dir.join(rel);
        if !path.is_file() {
            missing.push(format!("{id}: {rel}"));
            continue;
        }
        let actual = digest_file(&path)?;
        if actual != expected {
            mismatched.push(format!("{id}: {actual} != {expected}"));
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(doc) = serde_json::from_str::<Value>(&text) {
                sheet_docs.push((id, doc));
            }
        }
    }
    let (result, detail) = if !mismatched.is_empty() {
        (GateResult::Fail, Some(mismatched.join("; ")))
    } else if !missing.is_empty() {
        (GateResult::NotAssessed, Some(missing.join("; ")))
    } else if sheet_docs.is_empty() {
        (
            GateResult::NotAssessed,
            Some("the package names no sheets".into()),
        )
    } else {
        (GateResult::Pass, None)
    };
    checks.push(Check {
        id: "sheet_files_exist",
        result,
        detail,
    });
    verdict = verdict.and(result.into());

    // 2. The palette must fit the engine and define index 0 (transparent).
    let palette_rel = package["palette"]["path"].as_str().unwrap_or_default();
    let palette_path = dir.join(palette_rel);
    let palette_colors = fs::read_to_string(&palette_path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["colors"].as_array().map(Vec::len));
    let palette_max = engine["limits"]["palette_max"].as_u64().unwrap_or(62) as usize;
    let (result, detail) = match palette_colors {
        None => (
            GateResult::NotAssessed,
            Some(format!("cannot read palette at {palette_rel}")),
        ),
        Some(0) => (GateResult::Fail, Some("the palette has no colors".into())),
        Some(n) if n > palette_max => (
            GateResult::Fail,
            Some(format!("{n} colors exceeds the engine limit {palette_max}")),
        ),
        Some(n) => (GateResult::Pass, Some(format!("{n} colors"))),
    };
    checks.push(Check {
        id: "palette_index_in_range",
        result,
        detail,
    });
    verdict = verdict.and(result.into());

    // 3. Every animation an asset advertises must be reachable from the
    //    sheet: its `meta.frameTags` (an array of {name}), or for sheets
    //    without tags the frame keys `<sprite>_<anim>_<dir>_<i>`.
    let mut unresolved = Vec::new();
    for asset in package["assets"].as_array().cloned().unwrap_or_default() {
        let asset_id = asset["id"].as_str().unwrap_or_default();
        let sheet_id = asset["sheet"].as_str().unwrap_or_default();
        let Some((_, doc)) = sheet_docs.iter().find(|(id, _)| id == sheet_id) else {
            unresolved.push(format!("{asset_id}: sheet '{sheet_id}' is absent"));
            continue;
        };
        let mut names: BTreeSet<String> = doc["meta"]["frameTags"]
            .as_array()
            .map(|tags| {
                tags.iter()
                    .filter_map(|t| t["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if names.is_empty() {
            let prefix = format!("{asset_id}_");
            for key in doc["frames"]
                .as_object()
                .map(|m| m.keys())
                .into_iter()
                .flatten()
            {
                let rest = key.strip_prefix(&prefix).unwrap_or(key);
                // move_N_0 -> move_N
                if let Some((head, _)) = rest.rsplit_once('_') {
                    names.insert(head.to_string());
                }
            }
        }
        let matches = |anim: &str| {
            names
                .iter()
                .any(|n| n == anim || n.starts_with(&format!("{anim}_")))
        };
        for anim in asset["anims"]
            .as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            if !matches(&anim) {
                unresolved.push(format!("{asset_id}: anim '{anim}' has no frames"));
            }
        }
    }
    let (result, detail) = if unresolved.is_empty() {
        (GateResult::Pass, None)
    } else {
        (GateResult::Fail, Some(unresolved.join("; ")))
    };
    checks.push(Check {
        id: "anims_in_frame_tags",
        result,
        detail,
    });
    verdict = verdict.and(result.into());

    // 4. Level tile semantics must reference assets the package delivers.
    let mut tile_unknown = Vec::new();
    let mut levels_seen = 0usize;
    let known: BTreeSet<String> = package["assets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e["id"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if let Some(rel) = package["levels"]["path"].as_str() {
        let path = dir.join(rel);
        let files: Vec<PathBuf> = if path.is_dir() {
            fs::read_dir(&path)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
                        .collect()
                })
                .unwrap_or_default()
        } else if path.is_file() {
            vec![path]
        } else {
            Vec::new()
        };
        for file in files {
            levels_seen += 1;
            let Ok(text) = fs::read_to_string(&file) else {
                continue;
            };
            let Ok(level) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            for tile in level["tile_semantics"]
                .as_array()
                .cloned()
                .unwrap_or_default()
            {
                let behaviour = tile["behaviour"].as_str().unwrap_or("");
                // Floor can be a solid engine fill, so only blocking or
                // explicitly non-floor tiles must name a delivered asset.
                let needs_asset = tile["blocks"].as_bool().unwrap_or(false) || behaviour != "floor";
                if !needs_asset {
                    continue;
                }
                if let Some(tile_id) = tile["tile_id"].as_str() {
                    if !known.contains(tile_id) {
                        tile_unknown.push(format!(
                            "{}: tile '{tile_id}'",
                            file.file_name().unwrap_or_default().to_string_lossy()
                        ));
                    }
                }
            }
        }
    }
    let slice_mode = args.scope == "slice";
    let (result, detail) = if levels_seen == 0 {
        (
            GateResult::NotAssessed,
            Some("the package references no level files".into()),
        )
    } else if tile_unknown.is_empty() {
        (GateResult::Pass, Some(format!("{levels_seen} level(s)")))
    } else if slice_mode {
        // A slice proves the pipeline with one asset; the production run is
        // where every referenced tile must be delivered.
        (
            GateResult::Warning,
            Some(format!(
                "{} tile(s) await the production run: {}",
                tile_unknown.len(),
                tile_unknown.join("; ")
            )),
        )
    } else {
        (GateResult::Fail, Some(tile_unknown.join("; ")))
    };
    checks.push(Check {
        id: "level_tiles_in_sheets",
        result,
        detail,
    });
    verdict = verdict.and(result.into());

    // 5. Engine limits: every sheet has frames and the package's declared
    //    limits fit the engine's own.
    let mut limit_issues = Vec::new();
    for (id, doc) in &sheet_docs {
        let frames = doc["frames"].as_object().map(|m| m.len()).unwrap_or(0);
        if frames == 0 {
            limit_issues.push(format!("{id}: zero frames"));
        }
    }
    let declared = &package["engine"];
    let engine_frame_max = engine["limits"]["frame_max"].as_u64().unwrap_or(256);
    if declared["frame_max"].as_u64().unwrap_or(0) > engine_frame_max {
        limit_issues.push(format!(
            "package frame_max {} exceeds the engine limit {engine_frame_max}",
            declared["frame_max"]
        ));
    }
    if declared["palette_max"].as_u64().unwrap_or(0) > palette_max as u64 {
        limit_issues.push(format!(
            "package palette_max {} exceeds the engine limit {palette_max}",
            declared["palette_max"]
        ));
    }
    let (result, detail) = if limit_issues.is_empty() {
        (GateResult::Pass, None)
    } else {
        (GateResult::Fail, Some(limit_issues.join("; ")))
    };
    checks.push(Check {
        id: "engine_limits_respected",
        result,
        detail,
    });
    verdict = verdict.and(result.into());

    // 6. Only the current engine contract is readable by the production intake.
    let contract = engine["contract"].as_str().unwrap_or_default();
    let mode = engine["runtime"]["mode"].as_str().unwrap_or("js");
    let mut contract_issues = Vec::new();
    if contract != "engine/v4" {
        contract_issues.push(format!(
            "unsupported engine contract '{contract}'; regenerate the package with engine/v4"
        ));
    } else {
        if !["js", "rust"].contains(&mode) {
            contract_issues.push(format!("runtime.mode must be js or rust, got '{mode}'"));
        }
        if mode == "js" {
            for field in ["kind", "kernel", "loader", "entry"] {
                if engine["runtime"][field].as_str().unwrap_or("").is_empty() {
                    contract_issues.push(format!("runtime.{field} is required in JS mode"));
                }
            }
            if engine["platforms"].as_array().map(|items| items.len()) != Some(1)
                || engine["platforms"][0] != "browser"
            {
                contract_issues.push("JS mode platforms must be [\"browser\"]".to_string());
            }
        }
        for capability in package["visual"]["required_capabilities"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            let id = capability.as_str().unwrap_or_default();
            if !matches!(id, "pivot" | "duration" | "material" | "emissive") {
                contract_issues.push(format!("unknown required visual capability '{id}'"));
            } else if engine["capabilities"]["visual"][id].as_bool() != Some(true) {
                contract_issues.push(format!("required visual capability '{id}' is unavailable"));
            }
        }
    }
    let (result, detail) = if contract_issues.is_empty() {
        (GateResult::Pass, Some(format!("{contract}, mode {mode}")))
    } else {
        (GateResult::Fail, Some(contract_issues.join("; ")))
    };
    checks.push(Check {
        id: "engine_contract",
        result,
        detail,
    });
    verdict = verdict.and(result.into());

    let (result, detail) = audio_capability(&package, dir, &engine, contract, mode)?;
    checks.push(Check {
        id: "audio_capability",
        result,
        detail,
    });
    verdict = verdict.and(result.into());

    // Publish engine-check.json and the envelope.
    fs::create_dir_all(&out)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", out.display())))?;
    let report = json!({
        "schema_version": 1,
        "package_id": package["package_id"],
        "ok": verdict == Verdict::Pass,
        "checks": checks.iter().map(|c| json!({
            "id": c.id,
            "result": match c.result {
                GateResult::Pass => "pass",
                GateResult::Warning => "warning",
                GateResult::Fail => "fail",
                GateResult::NotAssessed => "not_assessed",
            },
            "detail": c.detail,
        })).collect::<Vec<_>>(),
    });
    write_json(&out.join("engine-check.json"), &report)?;

    let status = match verdict {
        Verdict::Pass => StepStatus::Ok,
        Verdict::NotAssessed => StepStatus::Blocked,
        Verdict::Fail => StepStatus::GateFailed,
        Verdict::Concerns => StepStatus::Ok,
    };
    let envelope = StepEnvelope {
        schema_version: 1,
        step: STEP_ID.to_string(),
        step_version: VERSION.to_string(),
        contract: crate::CONTRACT_ID.to_string(),
        status,
        scope: if slice_mode { Scope::Slice } else { Scope::All },
        inputs: vec![
            DigestEntry {
                path: "asset-package.json".to_string(),
                digest: digest_file(&dir.join("asset-package.json"))?,
            },
            DigestEntry {
                path: "engine.json".to_string(),
                digest: pxl_store::digest::digest_bytes(engine_text.as_bytes()),
            },
        ],
        outputs: vec![DigestEntry {
            path: "engine-check.json".to_string(),
            digest: digest_file(&out.join("engine-check.json"))?,
        }],
        verdict,
        gates: Gates {
            required_artifacts: vec![GateCheck {
                id: "asset-package.json".to_string(),
                result: GateResult::Pass,
                detail: None,
            }],
            quality_checks: checks
                .iter()
                .map(|c| GateCheck {
                    id: c.id.to_string(),
                    result: c.result,
                    detail: c.detail.clone(),
                })
                .collect(),
            not_assessed: checks
                .iter()
                .filter(|c| c.result == GateResult::NotAssessed)
                .map(|c| c.id.to_string())
                .collect(),
        },
        error: None,
    };
    write_json(&out.join("step.json"), &serde_json::to_value(&envelope)?)?;

    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": verdict == Verdict::Pass,
            "cmd": "step-500 check",
            "package_id": package["package_id"],
            "checks": checks.len(),
            "failed": checks.iter().filter(|c| c.result == GateResult::Fail).count(),
            "not_assessed": checks.iter().filter(|c| c.result == GateResult::NotAssessed).count(),
        }))?
    );
    Ok(match verdict {
        Verdict::Pass | Verdict::Concerns => 0,
        Verdict::NotAssessed => 11,
        Verdict::Fail => 5,
    })
}

fn audio_capability(
    package: &Value,
    dir: &Path,
    engine: &Value,
    contract: &str,
    mode: &str,
) -> Result<(GateResult, Option<String>)> {
    let entries = package["audio"].as_array().cloned().unwrap_or_default();
    if entries.is_empty() {
        if package.get("audio_manifest").is_some() {
            return Ok((
                GateResult::Fail,
                Some("audio_manifest is present but the audio collection is empty".into()),
            ));
        }
        return Ok((GateResult::Pass, Some("no audio assets".into())));
    }
    let supported = contract == "engine/v4"
        && engine["capabilities"]["audio"][mode].as_bool() == Some(true)
        && engine["capabilities"]["audio"]["formats"]
            .as_array()
            .is_some_and(|formats| formats.iter().any(|format| format == "wav"));
    let mut missing = Vec::new();
    let mut invalid = Vec::new();
    let mut verify = |rel: &str, expected: &str, invalid: &mut Vec<String>| {
        if !safe_package_path(rel) {
            invalid.push(format!("unsafe audio path '{rel}'"));
            return;
        }
        let path = dir.join(rel);
        if !path.is_file() {
            missing.push(rel.to_string());
            return;
        }
        match digest_file(&path) {
            Ok(actual) if actual == expected => {}
            Ok(actual) => invalid.push(format!("{rel}: {actual} != {expected}")),
            Err(error) => invalid.push(format!("{rel}: {}", error.message)),
        }
    };
    if let (Some(rel), Some(digest)) = (
        package["audio_manifest"]["path"].as_str(),
        package["audio_manifest"]["digest"].as_str(),
    ) {
        verify(rel, digest, &mut invalid);
    } else {
        invalid.push("audio_manifest is missing its path or digest".into());
    }
    for entry in &entries {
        let id = entry["id"].as_str().unwrap_or("audio");
        match (entry["path"].as_str(), entry["digest"].as_str()) {
            (Some(rel), Some(digest)) => verify(rel, digest, &mut invalid),
            _ => invalid.push(format!("{id}: missing path or digest")),
        }
        let frames = entry["frames"].as_u64().unwrap_or(0);
        if let Some(loop_range) = entry.get("loop") {
            let start = loop_range["start_frame"].as_u64().unwrap_or(0);
            let end = loop_range["end_frame_exclusive"].as_u64().unwrap_or(0);
            if start >= end || end > frames {
                invalid.push(format!(
                    "{id}: invalid loop {start}..{end} for {frames} frames"
                ));
            }
        }
    }
    let required = entries
        .iter()
        .any(|entry| entry["required"].as_bool() == Some(true));
    let mut detail = Vec::new();
    if !missing.is_empty() {
        detail.push(format!("missing: {}", missing.join(", ")));
    }
    if !invalid.is_empty() {
        detail.push(format!("integrity: {}", invalid.join("; ")));
    }
    if !supported {
        detail.push(format!("{contract} {mode} mode does not support WAV audio"));
    }
    let result = if !invalid.is_empty() || (!supported && required) {
        GateResult::Fail
    } else if !missing.is_empty() {
        GateResult::NotAssessed
    } else if !supported {
        GateResult::Warning
    } else {
        GateResult::Pass
    };
    if detail.is_empty() {
        detail.push(format!("{} WAV asset(s), {mode} mode", entries.len()));
    }
    Ok((result, Some(detail.join("; "))))
}

fn safe_package_path(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

pub(crate) fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
    text.push('\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", parent.display())))?;
    }
    fs::write(path, text)
        .map_err(|e| HarnessError::io(format!("cannot write {}: {e}", path.display())))
}
