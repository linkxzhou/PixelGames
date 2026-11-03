//! `step-400 run` / `check` / `approve` / `status`.
//!
//! Absorption map (plan §4.5, §7.2): the gates from harness
//! `assets_stage.rs` (`frame_index`, `png_ok`, `find_sheets`,
//! `rules_for_sheet`) collapse into "does every file the build manifest
//! claims exist with the claimed digest"; the fingerprint idea from
//! `asset_queue.rs::input_fingerprint` becomes the revision manifest. Nothing
//! here decodes pixels.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use pxl_contract::error::{HarnessError, Result};
use pxl_contract::{
    DigestEntry, GateCheck, GateResult, Gates, Scope, StepEnvelope, StepStatus, Verdict,
};
use pxl_store::digest::{digest_file, digest_pairs};

use crate::{audio, catalog, scenes, ApproveArgs, CheckArgs, RunArgs, StatusArgs, STEP_ID, VERSION};

/// A recipe as indexed by `step-300 plan`.
struct Recipe {
    id: String,
    out_rel: String,
    manifest_rel: Option<String>,
    ok: bool,
    error: Option<String>,
}

/// Every file one build published, keyed by its path relative to the build
/// directory, plus the QA verdict from the same manifest.
struct BuildFacts {
    files: BTreeMap<String, String>,
    visual_files: BTreeMap<String, String>,
    visual_metadata: Option<Value>,
    qa_ok: bool,
    sprites: Vec<String>,
    sheets: Vec<String>,
    palette: Option<(String, String)>,
    /// P04 stable resource entries the build manifest recorded.
    resources: Vec<Value>,
}

pub fn run(args: RunArgs) -> Result<i32> {
    if !args.input.is_dir() {
        return Err(
            HarnessError::usage(format!("{} is not a directory", args.input.display()))
                .with_hint("run --in takes the step-300 plan output"),
        );
    }
    let index_text = fs::read_to_string(args.input.join("index.json")).map_err(|e| {
        HarnessError::io(format!("index.json: {e}")).with_hint("run `step-300 plan` first")
    })?;
    let index: Value = serde_json::from_str(&index_text)
        .map_err(|e| HarnessError::schema(format!("index.json: {e}")))?;
    let recipes = recipes_of(&index)?;

    let req_text = fs::read_to_string(&args.requirements)
        .map_err(|e| HarnessError::io(format!("{}: {e}", args.requirements.display())))?;
    let requirements: Value = serde_json::from_str(&req_text)
        .map_err(|e| HarnessError::schema(format!("asset-requirements.json: {e}")))?;
    let mut required: Vec<(String, Value)> = requirements["requirements"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|r| r["required"].as_bool().unwrap_or(true))
                .filter_map(|r| r["asset_id"].as_str().map(|id| (id.to_string(), r.clone())))
                .collect()
        })
        .unwrap_or_default();
    // A slice package only promises the slice: the full requirements stay
    // the source of truth for production runs.
    if args.scope == "slice" {
        let pipeline_path = args
            .pipeline
            .as_ref()
            .ok_or_else(|| HarnessError::usage("--scope slice needs --pipeline <pipeline.json>"))?;
        let text = fs::read_to_string(pipeline_path)
            .map_err(|e| HarnessError::io(format!("{}: {e}", pipeline_path.display())))?;
        let pipeline: Value = serde_json::from_str(&text)
            .map_err(|e| HarnessError::schema(format!("pipeline.json: {e}")))?;
        let keep: Vec<String> = pipeline["slice"]["assets"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        required.retain(|(id, _)| keep.contains(id));
        if required.is_empty() {
            return Err(HarnessError::semantic(
                "pipeline.json slice.assets matches no required asset",
            ));
        }
    }
    let audio_requirements = audio::requirements(&args)?;
    if let Some(lock) = &args.catalog_lock {
        if !lock.is_file() {
            return Err(HarnessError::usage(format!("{} is not a file", lock.display()))
                .with_hint("--catalog-lock takes the file written by `step-300 prototype lock`"));
        }
    }

    // 1. Content addressing over every build the index claims. A missing
    //    file or a digest mismatch is a hard integrity problem: the package
    //    must never describe bytes it cannot verify.
    let mut facts: BTreeMap<String, BuildFacts> = BTreeMap::new();
    let mut attempts = Vec::new();
    let mut integrity_errors = Vec::new();
    let mut missing_files: Vec<String> = Vec::new();
    for recipe in &recipes {
        if !recipe.ok {
            attempts.push(json!({
                "asset": recipe.id,
                "issue": recipe.error.clone().unwrap_or_else(|| "recipe failed to build".into()),
                "result": "failed",
            }));
            continue;
        }
        let dir = args.input.join(&recipe.out_rel);
        let Some(manifest_rel) = &recipe.manifest_rel else {
            missing_files.push(format!("{}: build manifest", recipe.id));
            continue;
        };
        let manifest_path = dir.join(manifest_rel);
        let manifest: Value = match fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
        {
            Some(v) => v,
            None => {
                missing_files.push(format!("{}: {}", recipe.id, manifest_path.display()));
                continue;
            }
        };
        let mut files = BTreeMap::new();
        let mut visual_files = BTreeMap::new();
        let mut sprites = Vec::new();
        let mut sheets = Vec::new();
        let mut palette = None;
        for (name, digest) in manifest["outputs"].as_object().cloned().unwrap_or_default() {
            let path = dir.join(&name);
            let expected = digest.as_str().unwrap_or_default().to_string();
            if !path.is_file() {
                missing_files.push(format!("{}: {}", recipe.id, name));
                continue;
            }
            let actual = digest_file(&path)?;
            if actual != expected {
                integrity_errors.push(format!(
                    "{}: {} digest {actual} != manifest {expected}",
                    recipe.id, name
                ));
            }
            files.insert(name.clone(), actual.clone());
            if is_visual_artifact(&name) {
                visual_files.insert(name.clone(), actual.clone());
            }
            if is_channel_artifact(&name) {
                continue;
            }
            if name.ends_with(".pxl.json") {
                if let Some(stem) = name.strip_suffix(".pxl.json") {
                    sprites.push(stem.to_string());
                }
            } else if name.ends_with(".sheet.json") {
                sheets.push(name.trim_end_matches(".sheet.json").to_string());
            } else if name == "palette.json" && palette.is_none() {
                palette = Some((name.clone(), files.get(&name).cloned().unwrap_or_default()));
            }
        }
        for required in [
            "effective-style.json",
            "visual-report.json",
            "visual-metadata.json",
            "preview-manifest.json",
        ] {
            if !files.contains_key(required) {
                missing_files.push(format!("{}: required visual artifact {required}", recipe.id));
            }
        }
        let visual_metadata = match fs::read_to_string(dir.join("visual-metadata.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        {
            Some(value) => Some(value),
            None => {
                integrity_errors.push(format!("{}: invalid visual-metadata.json", recipe.id));
                None
            }
        };
        let qa_ok = manifest["qa"]["ok"].as_bool().unwrap_or(false);
        if !qa_ok {
            attempts.push(json!({
                "asset": recipe.id,
                "issue": "build manifest reports failed QA",
                "result": "failed",
            }));
        }
        // P04/P06: the stable resource entries ride along; the package
        // republishes them as its resource map.
        let resources = manifest["resources"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        facts.insert(
            recipe.id.clone(),
            BuildFacts {
                files,
                visual_files,
                visual_metadata,
                qa_ok,
                sprites,
                sheets,
                palette,
                resources,
            },
        );
    }
    let audio_build = audio::load(
        &index,
        &args.input,
        &mut attempts,
        &mut integrity_errors,
        &mut missing_files,
    )?;

    // 1b. P06: compiled scenes. Every scene is copied into the package and
    //     every scene->resource->sheet->PNG reference must close before the
    //     package claims it.
    let scene_sources = match &args.scenes {
        Some(dir) => Some(scenes::load(dir)?),
        None => None,
    };
    let mut scene_capabilities = std::collections::BTreeSet::new();
    let mut scene_closure_integrity = Vec::new();
    let mut scene_closure_missing = Vec::new();
    if let Some(sources) = &scene_sources {
        // The resource map and sheet paths are needed for closure; collect
        // them from the build facts gathered above.
        let resource_facts: BTreeMap<String, (Vec<Value>, PathBuf)> = facts
            .iter()
            .map(|(recipe_id, build)| {
                (
                    recipe_id.clone(),
                    (build.resources.clone(), args.input.join(recipe_id)),
                )
            })
            .collect();
        let mut sheet_paths: BTreeMap<String, (PathBuf, String)> = BTreeMap::new();
        for (recipe_id, build) in &facts {
            for sheet in &build.sheets {
                let name = format!("{sheet}.sheet.json");
                if let Some(digest) = build.files.get(&name) {
                    sheet_paths.insert(
                        sheet.clone(),
                        (args.input.join(recipe_id).join(&name), digest.clone()),
                    );
                }
            }
        }
        let resources: BTreeMap<String, Value> = scenes::resource_map(&resource_facts)
            .into_iter()
            .filter_map(|value| {
                value
                    .get("resource_id")
                    .and_then(Value::as_str)
                    .map(|id| (id.to_string(), value.clone()))
            })
            .collect();
        let (integrity, missing) = scenes::closure(sources, &resources, &sheet_paths);
        scene_closure_integrity = integrity;
        scene_closure_missing = missing;
        integrity_errors.extend(scene_closure_integrity.clone());
        missing_files.extend(scene_closure_missing.iter().cloned());
        scene_capabilities = scenes::scene_capabilities(sources);
    }

    // 2. Coverage: every required asset id must be delivered by some build.
    let mut delivered_ids: BTreeMap<String, String> = BTreeMap::new();
    for (source, build) in &facts {
        for sprite in &build.sprites {
            delivered_ids.insert(sprite.clone(), source.clone());
        }
    }
    let mut missing = Vec::new();
    let mut manifest_assets = Vec::new();
    for (id, req) in &required {
        let delivered = facts.values().any(|b| b.sprites.iter().any(|s| s == id))
            && delivered_ids.contains_key(id);
        let source = delivered_ids.get(id).cloned();
        let digests: Vec<DigestEntry> = source
            .as_ref()
            .and_then(|recipe_id| facts.get(recipe_id))
            .map(|b| {
                b.files
                    .iter()
                    .filter(|(name, _)| {
                        name.starts_with(&format!("{id}.")) || name == &"palette.json"
                    })
                    .map(|(name, digest)| DigestEntry {
                        path: format!("{}/{}", source.clone().unwrap_or_default(), name),
                        digest: digest.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let status = if delivered {
            "delivered"
        } else {
            missing.push(id.clone());
            attempts.push(json!({
                "asset": id,
                "issue": "required asset has no build output",
                "result": "missing",
            }));
            "missing"
        };
        manifest_assets.push(json!({
            "id": id,
            "required": true,
            "kind": req["kind"],
            "strategy": req["strategy"],
            "recipe": source,
            "status": status,
            "anims": req["anims"],
            "files": digests,
        }));
    }
    let extra: Vec<String> = delivered_ids
        .keys()
        .filter(|id| !required.iter().any(|(r, _)| r == *id))
        .cloned()
        .collect();
    let (audio_coverage, audio_manifest, audio_missing) =
        audio::coverage(audio_build.as_ref(), &audio_requirements);
    for id in &audio_missing {
        attempts.push(json!({
            "asset": id,
            "issue": "required audio has no build output",
            "result": "missing",
        }));
    }
    let asset_manifest = json!({
        "schema_version": 2,
        "assets": manifest_assets,
        "audio": audio_manifest,
    });
    let visual_ready = facts
        .values()
        .filter(|build| {
            ["effective-style.json", "visual-report.json", "visual-metadata.json", "preview-manifest.json"]
                .iter()
                .all(|name| build.visual_files.contains_key(*name))
        })
        .count();
    let visual_channels = facts
        .values()
        .map(|build| {
            build
                .visual_metadata
                .as_ref()
                .and_then(|metadata| metadata["assets"].as_array())
                .map(|assets| {
                    assets
                        .iter()
                        .map(|asset| asset["channels"].as_array().map(Vec::len).unwrap_or(0))
                        .sum::<usize>()
                })
                .unwrap_or(0)
        })
        .sum::<usize>();
    let coverage = json!({
        "schema_version": 2,
        "scope": args.scope,
        "required": required.len(),
        "delivered": required.len() - missing.len(),
        "missing": missing,
        "extra": extra,
        "audio": audio_coverage,
        "scenes": {
            "packaged": scene_sources.as_ref().map(|s| s.len()).unwrap_or(0),
            "closure_failures": scene_closure_integrity.len(),
            "closure_missing": scene_closure_missing,
        },
        "visual": {
            "ready": visual_ready,
            "required": facts.len(),
            "channels": visual_channels,
            "previews": facts.values().all(|build| build.visual_files.contains_key("preview-manifest.json")),
            "review": "not_assessed"
        }
    });

    let verdict = if !integrity_errors.is_empty() {
        Verdict::Fail
    } else if !missing.is_empty()
        || !audio_missing.is_empty()
        || !missing_files.is_empty()
        || facts.values().any(|b| !b.qa_ok)
    {
        Verdict::NotAssessed
    } else {
        Verdict::Pass
    };

    // 3. Publish the package files. package_id hashes the manifest text so
    //    the same inputs always produce the same id. Everything is written
    //    to a staging directory first; only a fully-written candidate is
    //    swapped into place, so a failed run never overwrites the last
    //    usable package.
    let game_id = args.game_id.clone().unwrap_or_else(|| {
        args.requirements
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("game")
            .to_string()
    });
    let staging = args.out.join(".staging");
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| {
            HarnessError::io(format!("cannot clean {}: {e}", staging.display()))
        })?;
    }
    fs::create_dir_all(&staging)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", staging.display())))?;
    write_json(&staging.join("asset-manifest.json"), &asset_manifest)?;
    write_json(&staging.join("coverage.json"), &coverage)?;
    write_attempts(&staging.join("attempts.jsonl"), &attempts)?;

    let packaged_audio = audio::package(&staging, audio_build.as_ref(), &audio_requirements)?;
    let packaged_catalog = catalog::package(args.catalog_lock.as_deref(), &staging)?;
    let mut package = build_package(&game_id, &required, &facts, &args, packaged_audio)?;
    if let Some(entry) = packaged_catalog {
        package["catalog_lock"] = entry;
    }
    // P06: scenes are copied into the package so it stays self-contained.
    if let Some(sources) = &scene_sources {
        let scene_entries = scenes::package(&staging, sources)?;
        package["scenes"] = json!(scene_entries);
    }
    // Scene demands extend the package-level capability union.
    if !scene_capabilities.is_empty() {
        let mut merged: std::collections::BTreeSet<String> = package["required_capabilities"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
        merged.extend(scene_capabilities.iter().cloned());
        package["required_capabilities"] = json!(merged);
    }
    // Experimental until a human approval is bound to this exact revision;
    // approve() is the only writer of "publishable".
    package["release_status"] = json!("experimental");
    write_json(&staging.join("asset-package.json"), &package)?;
    let escapes = scenes::escapes(&staging, &package);
    if !escapes.is_empty() {
        return Err(HarnessError::integrity(format!(
            "package paths escape the package root: {}",
            escapes.join("; ")
        ))
        .with_hint("every packaged path must resolve inside the package directory"));
    }

    let revision = revision_manifest(&staging, &package)?;
    write_json(&staging.join("revision.manifest.json"), &revision)?;
    write_json(
        &staging.join("publish-request.json"),
        &json!({
            "schema_version": 1,
            "package_id": package["package_id"],
            "package_digest": digest_file(&staging.join("asset-package.json"))?,
            "manifest_digest": revision["manifest_digest"],
        }),
    )?;
    publish(&staging, &args.out)?;

    // 4. Envelope. Gate rows mirror the three-value story: artifacts are
    //    machine-checkable, integrity failures are FAIL, missing inputs are
    //    NOT_ASSESSED.
    let mut required_artifacts = Vec::new();
    for (id, _) in &required {
        let ok = delivered_ids.contains_key(id);
        required_artifacts.push(GateCheck {
            id: format!("asset.{id}"),
            result: if ok {
                GateResult::Pass
            } else {
                GateResult::NotAssessed
            },
            detail: None,
        });
    }
    for (id, requirement) in &audio_requirements {
        if !requirement["required"].as_bool().unwrap_or(true) {
            continue;
        }
        let ok = !audio_missing.contains(id);
        required_artifacts.push(GateCheck {
            id: format!("audio.{id}"),
            result: if ok {
                GateResult::Pass
            } else {
                GateResult::NotAssessed
            },
            detail: None,
        });
    }
    if let Some(sources) = &scene_sources {
        for source in sources {
            required_artifacts.push(GateCheck {
                id: format!("scene.{}", source.scene.id),
                result: if scene_closure_integrity.is_empty() {
                    GateResult::Pass
                } else {
                    GateResult::Fail
                },
                detail: None,
            });
        }
    }
    let mut quality = Vec::new();
    for err in &integrity_errors {
        quality.push(GateCheck {
            id: "integrity".into(),
            result: GateResult::Fail,
            detail: Some(err.clone()),
        });
    }
    for err in &missing_files {
        quality.push(GateCheck {
            id: "delivery".into(),
            result: GateResult::NotAssessed,
            detail: Some(err.clone()),
        });
    }
    let status = match verdict {
        Verdict::Pass => StepStatus::Ok,
        Verdict::NotAssessed => StepStatus::Blocked,
        Verdict::Fail => StepStatus::IntegrityError,
        Verdict::Concerns => StepStatus::Ok,
    };
    let mut inputs = vec![
        DigestEntry {
            path: "index.json".to_string(),
            digest: digest_file(&args.input.join("index.json"))?,
        },
        DigestEntry {
            path: "asset-requirements.json".to_string(),
            digest: digest_file(&args.requirements)?,
        },
    ];
    let audio_requirements_path = args
        .audio_requirements
        .clone()
        .unwrap_or_else(|| args.requirements.with_file_name("audio-requirements.json"));
    if audio_requirements_path.is_file() {
        inputs.push(DigestEntry {
            path: "audio-requirements.json".to_string(),
            digest: digest_file(&audio_requirements_path)?,
        });
    }
    if let Some(dir) = &args.scenes {
        // The directory digest freezes the scene inputs into the envelope.
        let mut pairs = Vec::new();
        for source in scene_sources.iter().flatten() {
            let name = source
                .path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            pairs.push((name, digest_file(&source.path)?));
        }
        pairs.sort();
        inputs.push(DigestEntry {
            path: format!("{}/", dir.display()),
            digest: digest_pairs(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
        });
    }
    let envelope = StepEnvelope {
        schema_version: 1,
        step: STEP_ID.to_string(),
        step_version: VERSION.to_string(),
        contract: crate::CONTRACT_ID.to_string(),
        status,
        scope: Scope::All,
        inputs,
        outputs: [
            "asset-manifest.json",
            "coverage.json",
            "asset-package.json",
            "publish-request.json",
        ]
        .iter()
        .filter_map(|n| {
            digest_file(&args.out.join(n)).ok().map(|d| DigestEntry {
                path: n.to_string(),
                digest: d,
            })
        })
        .collect(),
        verdict,
        gates: Gates {
            required_artifacts,
            quality_checks: quality,
            not_assessed: {
                let mut all = missing.clone();
                all.extend(audio_missing.iter().cloned());
                all.extend(missing_files.iter().cloned());
                all
            },
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
            "ok": verdict == Verdict::Pass,
            "cmd": "step-400 run",
            "out": args.out.to_string_lossy(),
            "coverage": coverage["delivered"],
            "required": coverage["required"],
            "missing": coverage["missing"],
            "package_id": package["package_id"],
        }))?
    );
    Ok(match verdict {
        Verdict::Pass | Verdict::Concerns => 0,
        Verdict::NotAssessed => 11,
        Verdict::Fail => 15,
    })
}

/// `check`: recompute every digest the package claims and the revision
/// aggregate. Any mismatch is integrity (15); a missing file is blocked (11).
pub fn check(args: CheckArgs) -> Result<i32> {
    let dir = &args.input;
    let package_text = fs::read_to_string(dir.join("asset-package.json"))
        .map_err(|e| HarnessError::io(format!("asset-package.json: {e}")))
        .map_err(|e| HarnessError::io(format!("{}: run step-400 first", e.message)))?;
    let package: Value = serde_json::from_str(&package_text)
        .map_err(|e| HarnessError::schema(format!("asset-package.json: {e}")))?;
    let package_version = package["schema_version"].as_u64().unwrap_or(0);
    // v4 packages carry scenes, the resource map and release states; v3
    // packages from older runs stay readable until they are regenerated.
    if package_version != 3 && package_version != 4 {
        return Err(HarnessError::schema(format!(
            "unsupported asset-package schema_version {package_version}; regenerate it with asset-package/v4"
        )));
    }
    let mut missing = Vec::new();
    let mut mismatched = Vec::new();
    let mut checked = Vec::new();
    let verify = |rel: &str,
                  expected: &str,
                  missing: &mut Vec<String>,
                  mismatched: &mut Vec<String>,
                  checked: &mut Vec<String>| {
        let path = dir.join(rel);
        if !path.is_file() {
            missing.push(rel.to_string());
            return;
        }
        match digest_file(&path) {
            Ok(actual) if actual == expected => checked.push(rel.to_string()),
            Ok(actual) => mismatched.push(format!("{rel}: {actual} != {expected}")),
            Err(e) => missing.push(format!("{rel}: {}", e.message)),
        }
    };
    if let (Some(path), Some(digest)) = (
        package["palette"]["path"].as_str(),
        package["palette"]["digest"].as_str(),
    ) {
        verify(path, digest, &mut missing, &mut mismatched, &mut checked);
    }
    for sheet in package["sheets"].as_array().cloned().unwrap_or_default() {
        if let (Some(path), Some(digest)) = (sheet["path"].as_str(), sheet["digest"].as_str()) {
            verify(path, digest, &mut missing, &mut mismatched, &mut checked);
        }
        if let (Some(path), Some(digest)) = (
            sheet["palette"]["path"].as_str(),
            sheet["palette"]["digest"].as_str(),
        ) {
            verify(path, digest, &mut missing, &mut mismatched, &mut checked);
        }
    }
    for palette in package["palettes"].as_array().cloned().unwrap_or_default() {
        if let (Some(path), Some(digest)) = (palette["path"].as_str(), palette["digest"].as_str()) {
            verify(path, digest, &mut missing, &mut mismatched, &mut checked);
        }
    }
    for key in ["report", "metadata", "preview_manifest", "effective_style"] {
        if let (Some(path), Some(digest)) = (
            package["visual"][key]["path"].as_str(),
            package["visual"][key]["digest"].as_str(),
        ) {
            verify(path, digest, &mut missing, &mut mismatched, &mut checked);
        }
    }
    for build in package["visual"]["builds"].as_array().cloned().unwrap_or_default() {
        for key in ["report", "metadata", "preview_manifest", "effective_style"] {
            if let (Some(path), Some(digest)) = (
                build[key]["path"].as_str(),
                build[key]["digest"].as_str(),
            ) {
                verify(path, digest, &mut missing, &mut mismatched, &mut checked);
            }
        }
    }
    for asset in package["assets"].as_array().cloned().unwrap_or_default() {
        for channel in asset["channels"].as_array().cloned().unwrap_or_default() {
            for key in ["document", "sheet", "image", "palette"] {
                if let (Some(path), Some(digest)) = (
                    channel[key]["path"].as_str(),
                    channel[key]["digest"].as_str(),
                ) {
                    verify(path, digest, &mut missing, &mut mismatched, &mut checked);
                }
            }
        }
    }
    if let (Some(path), Some(digest)) = (
        package["levels"]["path"].as_str(),
        package["levels"]["digest"].as_str(),
    ) {
        let levels = dir.join(path);
        if !levels.is_dir() {
            missing.push(path.to_string());
        } else {
            let actual = level_directory_digest(&levels)?;
            if actual == digest {
                checked.push(path.to_string());
            } else {
                mismatched.push(format!("{path}: {actual} != {digest}"));
            }
        }
    }
    if let (Some(path), Some(digest)) = (
        package["audio_manifest"]["path"].as_str(),
        package["audio_manifest"]["digest"].as_str(),
    ) {
        verify(path, digest, &mut missing, &mut mismatched, &mut checked);
    }
    for audio in package["audio"].as_array().cloned().unwrap_or_default() {
        if let (Some(path), Some(digest)) = (audio["path"].as_str(), audio["digest"].as_str()) {
            verify(path, digest, &mut missing, &mut mismatched, &mut checked);
        }
    }
    if let (Some(path), Some(digest)) = (
        package["catalog_lock"]["path"].as_str(),
        package["catalog_lock"]["digest"].as_str(),
    ) {
        verify(path, digest, &mut missing, &mut mismatched, &mut checked);
        mismatched.extend(catalog::recheck(dir, &package["catalog_lock"]));
    }
    // P06: scenes and their scene->resource->sheet->PNG closure. A scene
    // whose recorded digest no longer matches is tampering (15); a missing
    // scene file is a delivery gap (11).
    for scene in package["scenes"].as_array().cloned().unwrap_or_default() {
        let id = scene["id"].as_str().unwrap_or_default().to_string();
        if let (Some(path), Some(digest)) = (scene["path"].as_str(), scene["digest"].as_str()) {
            if !scenes::safe_relative(path) {
                mismatched.push(format!("scene {id}: '{path}' is not a safe relative path"));
                continue;
            }
            verify(path, digest, &mut missing, &mut mismatched, &mut checked);
            let scene_file = dir.join(path);
            if scene_file.is_file() {
                if let Ok(text) = fs::read_to_string(&scene_file)
                    .and_then(|t| Ok(serde_json::from_str::<Value>(&t)?))
                {
                    let recorded = scene["instances"].as_u64().unwrap_or(0);
                    let actual = text["instances"].as_array().map(Vec::len).unwrap_or(0);
                    if u64::try_from(actual).unwrap_or(u64::MAX) != recorded {
                        mismatched.push(format!(
                            "scene {id}: claims {recorded} instances but the file has {actual}"
                        ));
                    }
                    let recorded_review = scene["review"].as_str().unwrap_or_default();
                    let actual_review = text["review"].as_str().unwrap_or_default();
                    if recorded_review != actual_review {
                        mismatched.push(format!(
                            "scene {id}: review state changed from '{recorded_review}' to '{actual_review}'"
                        ));
                    }
                }
            }
        }
    }
    // Every resource the package maps must point at a packaged sheet json.
    let packaged_sheets: std::collections::BTreeSet<String> = package["sheets"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|sheet| {
            sheet["path"]
                .as_str()
                .map(|path| path.rsplit('/').next().unwrap_or(path).to_string())
        })
        .collect();
    for resource in package["resources"].as_array().cloned().unwrap_or_default() {
        let id = resource["resource_id"].as_str().unwrap_or_default().to_string();
        let Some(sheet_json) = resource["sheet_json"].as_str() else {
            mismatched.push(format!("resource '{id}' has no sheet_json"));
            continue;
        };
        if !packaged_sheets.contains(sheet_json) {
            // Old packages reference the build tree; that stays readable.
            let inline = dir.join(sheet_json);
            if !inline.is_file() {
                missing.push(format!("resource '{id}': sheet {sheet_json}"));
            }
        }
    }
    // P06: no packaged path may escape the package root.
    for escape in scenes::escapes(dir, &package) {
        mismatched.push(escape);
    }
    // The revision aggregate must still match the files on disk.
    let revision_path = dir.join("revision.manifest.json");
    if let Ok(text) = fs::read_to_string(&revision_path) {
        if let Ok(revision) = serde_json::from_str::<Value>(&text) {
            let recorded = revision["manifest_digest"].as_str().unwrap_or_default();
            let current = recompute_revision(dir)?;
            if recorded != current {
                mismatched.push(format!("revision: {current} != {recorded}"));
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": missing.is_empty() && mismatched.is_empty(),
            "cmd": "step-400 check",
            "checked": checked.len(),
            "missing": missing,
            "mismatched": mismatched,
        }))?
    );
    if !mismatched.is_empty() {
        Ok(15)
    } else if !missing.is_empty() {
        Ok(11)
    } else {
        Ok(0)
    }
}

/// `approve`: bind the manifest digest. The three states live in the
/// revision manifest: pending -> approved, or needs_revision when the
/// package is incomplete.
pub fn approve(args: ApproveArgs) -> Result<i32> {
    let dir = &args.input;
    let revision_path = dir.join("revision.manifest.json");
    if !revision_path.is_file() {
        eprintln!("error [E_NOT_FOUND]: no revision.manifest.json; run step-400 first");
        return Ok(4);
    }
    let current = recompute_revision(dir)?;
    if args.digest != current {
        let err = HarnessError::integrity(format!(
            "digest mismatch: given {}, current {current}",
            args.digest
        ))
        .with_hint("copy manifest_digest from revision.manifest.json");
        eprintln!("error [{}]: {}", err.code.as_str(), err.message);
        if let Some(hint) = &err.hint {
            eprintln!("hint: {hint}");
        }
        return Ok(15);
    }
    let Some(visual_verdict) = crate::visual::review_verdict(dir)? else {
        eprintln!("visual review is NOT_ASSESSED: two independent current-revision reviews are required");
        return Ok(11);
    };
    let mut revision: Value = serde_json::from_str(
        &fs::read_to_string(&revision_path)
            .map_err(|e| HarnessError::io(format!("revision.manifest.json: {e}")))?,
    )
    .map_err(|e| HarnessError::schema(format!("revision.manifest.json: {e}")))?;
    revision["status"] = json!("approved");
    revision["visual_verdict"] = json!(visual_verdict);
    write_json(&revision_path, &revision)?;
    // P06: approval is what marks a v4 package publishable. The package
    // digest changes with the release status, so recompute the revision
    // after the edit and refuse a package whose scenes are not approved.
    let package_path = dir.join("asset-package.json");
    if package_path.is_file() {
        let mut package: Value = serde_json::from_str(
            &fs::read_to_string(&package_path)
                .map_err(|e| HarnessError::io(format!("asset-package.json: {e}")))?,
        )
        .map_err(|e| HarnessError::schema(format!("asset-package.json: {e}")))?;
        if package["schema_version"].as_u64() == Some(4) {
            let mut stale = Vec::new();
            for scene in package["scenes"].as_array().cloned().unwrap_or_default() {
                if scene["review"] != "approved" {
                    stale.push(scene["id"].as_str().unwrap_or_default().to_string());
                }
            }
            if !stale.is_empty() {
                eprintln!(
                    "error [E_SEMANTIC]: scenes not approved by a human reviewer: {}",
                    stale.join(", ")
                );
                eprintln!("hint: approval is never set by this tool; review the scenes at the source and recompile");
                return Ok(4);
            }
            package["release_status"] = json!("publishable");
            write_json(&package_path, &package)?;
            // Bind the new revision aggregate after the status flip.
            let revision: Value = serde_json::from_str(
                &fs::read_to_string(&revision_path)
                    .map_err(|e| HarnessError::io(format!("revision.manifest.json: {e}")))?,
            )
            .map_err(|e| HarnessError::schema(format!("revision.manifest.json: {e}")))?;
            let recomputed = recompute_revision(dir)?;
            let mut updated = revision;
            updated["manifest_digest"] = json!(recomputed);
            write_json(&revision_path, &updated)?;
        }
    }
    write_json(
        &dir.join("approval.json"),
        &json!({
            "schema_version": 1,
            "stage": "assets",
            "digest": current,
            "visual_verdict": visual_verdict,
            "approved_by": "user",
        }),
    )?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": true, "cmd": "step-400 approve", "digest": current,
        }))?
    );
    Ok(0)
}

pub fn status(args: StatusArgs) -> Result<i32> {
    let path = args.input.join("step.json");
    let text = fs::read_to_string(&path).map_err(|e| {
        HarnessError::io(format!("{}: {e}", path.display())).with_hint("run step-400 first")
    })?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| HarnessError::schema(format!("step.json: {e}")))?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(0)
}

// ---- helpers ----

/// Move a fully-written staging directory into place. Existing package
/// files are replaced file-by-file only after the candidate is complete;
/// any error before the swap leaves the previous package untouched.
fn publish(staging: &Path, out: &Path) -> Result<()> {
    fs::create_dir_all(out)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", out.display())))?;
    let mut entries: Vec<PathBuf> = Vec::new();
    collect_files(staging, staging, &mut entries)?;
    entries.sort();
    for rel in &entries {
        let target = out.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                HarnessError::io(format!("cannot create {}: {e}", parent.display()))
            })?;
        }
        fs::copy(staging.join(rel), &target).map_err(|e| {
            HarnessError::io(format!("cannot publish {}: {e}", rel.display()))
        })?;
    }
    fs::remove_dir_all(staging)
        .map_err(|e| HarnessError::io(format!("cannot clean {}: {e}", staging.display())))?;
    Ok(())
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)
        .map_err(|e| HarnessError::io(format!("cannot read {}: {e}", dir.display())))?
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
    Ok(())
}

fn recipes_of(index: &Value) -> Result<Vec<Recipe>> {
    let version = index["schema_version"].as_u64().unwrap_or(0);
    // asset-build/v5 adds the stable `resources` block; the recipes layout is
    // unchanged, so v4 indexes stay readable until they are regenerated.
    if version != 4 && version != 5 {
        return Err(HarnessError::schema(format!(
            "unsupported asset-build schema_version {version}; regenerate it with asset-build/v5"
        )));
    }
    let mut out = Vec::new();
    for entry in index["recipes"].as_array().cloned().unwrap_or_default() {
        let id = entry["id"].as_str().unwrap_or_default().to_string();
        let status = entry["status"].as_str().unwrap_or("parse_error");
        let out_rel = entry["out"]
            .as_str()
            .unwrap_or(&format!("{id}/"))
            .to_string();
        out.push(Recipe {
            id,
            out_rel,
            manifest_rel: entry["manifest"].as_str().map(str::to_string),
            ok: status == "ok",
            error: entry["error"].as_str().map(str::to_string),
        });
    }
    if out.is_empty() {
        return Err(HarnessError::schema("index.json lists no recipes")
            .with_hint("run `step-300 plan` first"));
    }
    Ok(out)
}

fn build_package(
    game_id: &str,
    required: &[(String, Value)],
    facts: &BTreeMap<String, BuildFacts>,
    args: &RunArgs,
    packaged_audio: Option<audio::PackagedAudio>,
) -> Result<Value> {
    // Sheets: every sheet json any build published, addressed relative to the
    // package root (which lives beside the build tree, so ../<build>/<out>).
    let mut sheets = Vec::new();
    let mut palette = None;
    let mut palettes = Vec::new();
    for (recipe_id, build) in facts {
        let sheet_palette = build.palette.as_ref().map(|(name, digest)| {
            json!({
                "path": relative_from(&args.out, &args.input.join(recipe_id).join(name)),
                "digest": digest,
            })
        });
        if let Some(value) = &sheet_palette {
            palettes.push(value.clone());
        }
        for sheet in &build.sheets {
            let name = format!("{sheet}.sheet.json");
            if let Some(digest) = build.files.get(&name) {
                // The package describes the sheet, so it carries the real
                // frame count and the frame tags step-500 verifies against.
                let sheet_path = args.input.join(recipe_id).join(&name);
                let (frames, tags) = sheet_facts(&sheet_path);
                let file = args.input.join(recipe_id).join(&name);
                sheets.push(json!({
                    "id": sheet,
                    "path": relative_from(&args.out, &file),
                    "digest": digest,
                    "frames": frames,
                    "frame_tags": tags,
                    "palette": sheet_palette.clone(),
                }));
            }
        }
        if palette.is_none() {
            if let Some((name, digest)) = &build.palette {
                let file = args.input.join(recipe_id).join(name);
                palette = Some(json!({
                    "path": relative_from(&args.out, &file),
                    "digest": digest,
                }));
            }
        }
    }
    let mut assets = Vec::new();
    let mut capabilities = std::collections::BTreeSet::new();
    for (id, req) in required {
        let Some(recipe_id) = facts
            .iter()
            .find(|(_, b)| b.sprites.iter().any(|s| s == id))
            .map(|(k, _)| k.clone())
        else {
            continue;
        };
        let anims: BTreeMap<String, Value> = req["anims"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|a| {
                let name = a["name"].as_str()?.to_string();
                Some((
                    name,
                    json!({
                        "frames": a["frames"].as_u64().unwrap_or(1),
                        "fps": a["fps"].as_u64().unwrap_or(1),
                        "loop": a["looped"].as_bool().unwrap_or(true),
                    }),
                ))
            })
            .collect();
        let build = &facts[&recipe_id];
        let visual_asset = build.visual_metadata.as_ref().and_then(|metadata| {
            metadata["assets"]
                .as_array()
                .and_then(|assets| assets.iter().find(|asset| asset["asset_id"] == *id))
        });
        if !anims.is_empty() {
            capabilities.insert("duration".to_string());
        }
        if visual_asset.is_some_and(|asset| !asset["pivot"].is_null()) {
            capabilities.insert("pivot".to_string());
        }
        let channels = visual_asset
            .and_then(|asset| asset["channels"].as_array())
            .into_iter()
            .flatten()
            .map(|channel| {
                let channel_id = channel["id"].as_str().unwrap_or_default().to_string();
                capabilities.insert(channel_id.clone());
                json!({
                    "id": channel_id,
                    "document": visual_file_reference(&args, &recipe_id, build, channel["document"].as_str().unwrap_or_default()),
                    "sheet": visual_file_reference(&args, &recipe_id, build, channel["sheet"].as_str().unwrap_or_default()),
                    "image": visual_file_reference(&args, &recipe_id, build, channel["image"].as_str().unwrap_or_default()),
                    "palette": visual_file_reference(&args, &recipe_id, build, channel["palette"].as_str().unwrap_or_default()),
                })
            })
            .collect::<Vec<_>>();
        assets.push(json!({
            "id": id,
            "kind": if anims.is_empty() { "static" } else { "animated" },
            "sheet": recipe_id,
            "anims": anims,
            "role": visual_asset.and_then(|asset| asset["role"].as_str()),
            "pivot": visual_asset.map(|asset| asset["pivot"].clone()),
            "channels": channels,
        }));
    }
    // Levels are copied into the package so it stays self-contained: the
    // engine receives one directory and never reaches outside it.
    let levels = match &args.levels {
        Some(dir) => {
            let dest = args.out.join("levels");
            fs::create_dir_all(&dest)
                .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", dest.display())))?;
            let mut entries: Vec<PathBuf> = fs::read_dir(dir)
                .map_err(|e| HarnessError::io(format!("{}: {e}", dir.display())))?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
                .collect();
            entries.sort();
            let mut pairs: Vec<(String, String)> = Vec::new();
            for entry in entries {
                let name = entry
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                fs::copy(&entry, dest.join(&name))
                    .map_err(|e| HarnessError::io(format!("cannot copy levels/{name}: {e}")))?;
                pairs.push((name, digest_file(&entry)?));
            }
            let digest = digest_pairs(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            Some(json!({ "path": "levels", "digest": digest }))
        }
        None => None,
    };
    let mut identity: Vec<(&str, &str)> = assets
        .iter()
        .filter_map(|asset| asset["id"].as_str().map(|id| (id, "")))
        .collect();
    if let Some(audio) = &packaged_audio {
        identity.extend(
            audio
                .entries
                .iter()
                .filter_map(|entry| Some((entry["id"].as_str()?, entry["digest"].as_str()?))),
        );
    }
    let package_hash = digest_pairs(identity);
    let rev = package_hash
        .strip_prefix("sha256:")
        .map(|s| &s[..12.min(s.len())])
        .unwrap_or("000000000000");
    let engine_palette_max = 62;
    // P06: the stable resource map ordered by resource id, plus the palette
    // family summaries frozen by the intent (read from every build's
    // effective-style.json when present).
    let resource_facts: BTreeMap<String, (Vec<Value>, PathBuf)> = facts
        .iter()
        .map(|(recipe_id, build)| {
            (
                recipe_id.clone(),
                (build.resources.clone(), args.input.join(recipe_id)),
            )
        })
        .collect();
    let resources = scenes::resource_map(&resource_facts);
    let mut families: Vec<Value> = Vec::new();
    {
        let mut seen = std::collections::BTreeSet::new();
        for (recipe_id, _build) in facts {
            let Ok(text) = fs::read_to_string(args.input.join(recipe_id).join("effective-style.json")) else {
                continue;
            };
            let Ok(style) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            for family in style["palette_families"].as_array().cloned().unwrap_or_default() {
                let Some(id) = family["id"].as_str().map(String::from) else {
                    continue;
                };
                if seen.insert(id) {
                    families.push(family);
                }
            }
        }
        families.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    }
    let mut package = json!({
        "schema_version": 4,
        "package_id": format!("{game_id}@{rev}"),
        "engine": {
            "name": "pxl-web",
            "contract": "engine/v4",
            "frame_max": 256,
            "palette_max": engine_palette_max,
        },
        "palette": palette.unwrap_or_else(|| json!({"path": "", "digest": ""})),
        "palettes": palettes,
        "families": families,
        "resources": resources,
        "required_capabilities": capabilities,
        "release_status": "experimental",
        "sheets": sheets,
        "assets": assets,
    });
    if let Some((recipe_id, build)) = facts.iter().next() {
        package["visual"] = json!({
            "report": visual_file_reference(args, recipe_id, build, "visual-report.json"),
            "metadata": visual_file_reference(args, recipe_id, build, "visual-metadata.json"),
            "preview_manifest": visual_file_reference(args, recipe_id, build, "preview-manifest.json"),
            "effective_style": visual_file_reference(args, recipe_id, build, "effective-style.json"),
            "required_capabilities": capabilities,
        });
    }
    let visual_builds = facts
        .iter()
        .map(|(recipe_id, build)| {
            json!({
                "recipe": recipe_id,
                "report": visual_file_reference(args, recipe_id, build, "visual-report.json"),
                "metadata": visual_file_reference(args, recipe_id, build, "visual-metadata.json"),
                "preview_manifest": visual_file_reference(args, recipe_id, build, "preview-manifest.json"),
                "effective_style": visual_file_reference(args, recipe_id, build, "effective-style.json"),
            })
        })
        .collect::<Vec<_>>();
    if !visual_builds.is_empty() {
        package["visual"]["builds"] = json!(visual_builds);
    }
    if let Some(levels) = levels {
        package["levels"] = levels;
    }
    if let Some(audio) = packaged_audio {
        package["audio_manifest"] = audio.manifest;
        package["audio"] = json!(audio.entries);
    }
    Ok(package)
}

/// Frame count and frame-tag names of a published sheet. A sheet that cannot
/// be parsed reports zero frames; step-500 turns that into a failed check.
fn sheet_facts(path: &Path) -> (usize, Vec<String>) {
    let value: Option<Value> = fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let Some(value) = value else {
        return (0, Vec::new());
    };
    let frames = value["frames"].as_object().map(|m| m.len()).unwrap_or(0);
    let mut tags: Vec<String> = value["meta"]["frameTags"]
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    tags.sort();
    (frames, tags)
}

/// Manifest of the package files that are not themselves packages-of-record:
/// the manifest, coverage, the package, and the publish request.
fn revision_manifest(out: &Path, package: &Value) -> Result<Value> {
    let mut files = Vec::new();
    for name in ["asset-manifest.json", "coverage.json", "asset-package.json"] {
        let path = out.join(name);
        if path.is_file() {
            files.push((name.to_string(), digest_file(&path)?));
        }
    }
    files.extend(audio::revision_files(out)?);
    files.extend(catalog::revision_files(out)?);
    files.extend(scenes::revision_files(out)?);
    // digest_pairs is order-sensitive: sort exactly like recompute_revision.
    files.sort();
    let pairs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, d)| (p.as_str(), d.as_str()))
        .collect();
    let status = if package["assets"]
        .as_array()
        .map(|a| a.is_empty())
        .unwrap_or(true)
    {
        "needs_revision"
    } else {
        "pending"
    };
    Ok(json!({
        "schema_version": 1,
        "stage": "assets",
        "status": status,
        "files": files.iter().map(|(p, d)| json!({"path": p, "digest": d})).collect::<Vec<_>>(),
        "manifest_digest": digest_pairs(pairs),
    }))
}

fn level_directory_digest(dir: &Path) -> Result<String> {
    let mut entries = fs::read_dir(dir)
        .map_err(|e| HarnessError::io(format!("{}: {e}", dir.display())))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|v| v.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    entries.sort();
    let mut pairs = Vec::new();
    for path in entries {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        pairs.push((name, digest_file(&path)?));
    }
    Ok(digest_pairs(
        pairs
            .iter()
            .map(|(name, digest)| (name.as_str(), digest.as_str())),
    ))
}

pub(crate) fn recompute_revision(dir: &Path) -> Result<String> {
    let mut files = Vec::new();
    for name in ["asset-manifest.json", "coverage.json", "asset-package.json"] {
        let path = dir.join(name);
        if path.is_file() {
            files.push((name.to_string(), digest_file(&path)?));
        }
    }
    files.extend(audio::revision_files(dir)?);
    files.extend(catalog::revision_files(dir)?);
    files.extend(scenes::revision_files(dir)?);
    files.sort();
    let pairs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, d)| (p.as_str(), d.as_str()))
        .collect();
    Ok(digest_pairs(pairs))
}

fn visual_file_reference(
    args: &RunArgs,
    recipe_id: &str,
    build: &BuildFacts,
    name: &str,
) -> Value {
    json!({
        "path": relative_from(&args.out, &args.input.join(recipe_id).join(name)),
        "digest": build.visual_files.get(name).or_else(|| build.files.get(name)),
    })
}

fn is_channel_artifact(name: &str) -> bool {
    name.contains(".material.")
        || name.contains(".emissive.")
        || name.contains(".shadow.")
        || name == "channel.palette.json"
}

fn is_visual_artifact(name: &str) -> bool {
    matches!(
        name,
        "effective-style.json"
            | "visual-report.json"
            | "visual-metadata.json"
            | "preview-manifest.json"
    ) || is_channel_artifact(name)
}

/// Relative path from `base` to `target`, both made absolute first. Used so
/// the package addresses its sheets and palette whether the build tree sits
/// beside it or one `.slice/` level deeper.
fn relative_from(base: &Path, target: &Path) -> String {
    let harden = |p: &Path| -> PathBuf {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(p))
                .unwrap_or_else(|_| p.to_path_buf())
        }
    };
    let base = harden(base);
    let target = harden(target);
    let base_parts: Vec<std::path::Component> = base.components().collect();
    let target_parts: Vec<std::path::Component> = target.components().collect();
    let common = base_parts
        .iter()
        .zip(&target_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts: Vec<String> = Vec::new();
    for _ in common..base_parts.len() {
        parts.push("..".to_string());
    }
    for component in &target_parts[common..] {
        parts.push(component.as_os_str().to_string_lossy().into_owned());
    }
    parts.join("/")
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
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

fn write_attempts(path: &Path, attempts: &[Value]) -> Result<()> {
    let mut text = String::new();
    for entry in attempts {
        text.push_str(
            &serde_json::to_string(entry)
                .map_err(|e| HarnessError::io(format!("attempts.jsonl: {e}")))?,
        );
        text.push('\n');
    }
    fs::write(path, text)
        .map_err(|e| HarnessError::io(format!("cannot write {}: {e}", path.display())))
}
