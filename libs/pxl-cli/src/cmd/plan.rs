use std::fs;
use std::path::{Path, PathBuf};

use pxl_core::{ErrorCode, PxlError};
use pxl_format::Recipe;
use serde_json::{json, Value};

use super::args::{BuildArgs, PlanArgs};
use super::build::{build_cmd, manifest_file_name};
use crate::error::{CmdError, CmdResult};
use crate::util::{self, Ctx};

/// `plan` is the batch entry between steps (plan/v3.0/pxl-step-pipeline-plan.md
/// §4.3). It discovers the recipes under the step-200 output, builds each one
/// with the same pipeline as `build` (staging, QA, publish, .build.json
/// manifest), and writes one `index.json`. A failed recipe never aborts the
/// batch; the worst status wins the exit code.
pub(super) fn plan_cmd(ctx: &Ctx, args: PlanArgs) -> CmdResult<()> {
    let input = PathBuf::from(&args.input);
    if !input.is_dir() {
        return Err(
            PxlError::usage(format!("{} is not a directory", input.display()))
                .with_hint("plan --in takes the step-200 output directory")
                .into(),
        );
    }
    if input.join("spec-manifest.json").is_file() {
        super::visual::VisualContext::load(&input, true)?;
    }
    let mut recipes = discover_recipes(&input)?;
    if let Some(only) = &args.asset {
        recipes.retain(|(id, _)| id == only);
        if recipes.is_empty() {
            return Err(PxlError::semantic(format!("no recipe matches id '{only}'"))
                .with_hint("the id is the recipe stem or a spec-manifest entry id")
                .into());
        }
    }
    if recipes.is_empty() {
        return Err(
            PxlError::semantic(format!("no *.recipe.json under {}", input.display()))
                .with_hint("expected <id>/<id>.recipe.json or a spec-manifest.json listing recipes")
                .into(),
        );
    }
    recipes.sort();
    if args.baseline {
        return super::baseline::baseline_cmd(ctx, &args, &input, &recipes);
    }

    // P04: resolve the palette families once, before any recipe builds, so a
    // stale family digest fails the whole batch with the family id instead of
    // surfacing per recipe. The registry loads and verifies every palette the
    // intent froze (byte digest + semantic digest).
    if input.join("spec-manifest.json").is_file() {
        if let Some(context) = super::visual::VisualContext::load(&input, true)? {
            if !context.families.is_empty() && context.families.len() != context.intent.palette_families.len() {
                return Err(PxlError::semantic(
                    "resolved palette family count does not match the intent",
                )
                .into());
            }
        }
    }

    let audio_spec = audio_spec_path(&input)?;
    let out_root = PathBuf::from(&args.out);
    let mut entries = Vec::new();
    let mut failures = 0usize;
    for (id, recipe_rel) in &recipes {
        let entry = if args.dry_run {
            dry_run_one(&input, recipe_rel, id)
        } else {
            build_one(ctx, &args, &input, &out_root, recipe_rel, id)
        };
        if entry.get("status").and_then(Value::as_str) != Some("ok") {
            failures += 1;
        }
        entries.push(entry);
    }
    let recipe_failures = failures;
    let ok_count = entries.len() - recipe_failures;
    if args.dry_run {
        let audio = audio_spec.as_ref().map(|path| {
            let entry = dry_run_audio(path);
            if entry["status"] != "ok" {
                failures += 1;
            }
            entry
        });
        let checked = entries.len() + usize::from(audio.is_some());
        let mut body = util::ok_base("plan");
        body.insert("dry_run".into(), json!(true));
        body.insert("checked".into(), json!(checked));
        body.insert("ok".into(), json!(failures == 0));
        body.insert("ok_count".into(), json!(checked - failures));
        body.insert("recipes".into(), json!(entries));
        if let Some(audio) = audio {
            body.insert("audio".into(), audio);
        }
        util::emit_ok(ctx, &Value::Object(body));
        return if failures == 0 {
            Ok(())
        } else {
            Err(PxlError::semantic(format!(
                "plan --dry-run: {failures} of {checked} build item(s) failed to parse"
            ))
            .with_hint("read the recipes[] and audio entries for the first error per item")
            .into())
        };
    }

    let mut index = json!({
        "schema_version": 5,
        "scope": args.scope,
        "recipes": entries,
    });
    if let Some(spec) = audio_spec {
        let audio = build_audio(ctx, &spec, &out_root);
        if audio["status"] != "ok" {
            failures += 1;
        }
        index["audio"] = audio;
    }
    let mut text = serde_json::to_string_pretty(&index)
        .map_err(|e| PxlError::io(format!("index.json: {e}")))?;
    text.push('\n');
    // P04: the publish index is the batch's commit record, so it is written
    // last and atomically — a failure above leaves the previous usable index
    // in place instead of a half-new, half-old set of references. Writing one
    // file atomically is a rename; the batch's per-recipe trees keep their own
    // manifests as before.
    let index_path = out_root.join("index.json");
    let staging_path = out_root.join("index.json.new");
    util::write_out(
        staging_path.to_str().unwrap_or("index.json.new"),
        text.as_bytes(),
        true,
        false,
    )?;
    fs::rename(&staging_path, &index_path)
        .map_err(|e| PxlError::io(format!("cannot publish index.json: {e}")))?;
    let mut required = index["recipes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| pxl_contract::GateCheck {
            id: format!("recipe:{}", entry["id"].as_str().unwrap_or_default()),
            result: if entry["status"] == "ok" {
                pxl_contract::GateResult::Pass
            } else {
                pxl_contract::GateResult::Fail
            },
            detail: entry["error"].as_str().map(str::to_string),
        })
        .collect::<Vec<_>>();
    if let Some(audio) = index.get("audio") {
        required.push(pxl_contract::GateCheck {
            id: "audio".into(),
            result: if audio["status"] == "ok" {
                pxl_contract::GateResult::Pass
            } else {
                pxl_contract::GateResult::Fail
            },
            detail: audio["error"].as_str().map(str::to_string),
        });
    }
    let gates = pxl_contract::Gates {
        required_artifacts: required,
        quality_checks: vec![pxl_contract::GateCheck {
            id: "asset.semantic_compile".into(),
            result: if failures == 0 {
                pxl_contract::GateResult::Pass
            } else {
                pxl_contract::GateResult::Fail
            },
            detail: None,
        }],
        not_assessed: Vec::new(),
    };
    let inputs = if input.join("spec-manifest.json").is_file() {
        vec![pxl_contract::DigestEntry {
            path: "spec-manifest.json".into(),
            digest: util::hash_of(
                &fs::read(input.join("spec-manifest.json"))
                    .map_err(|e| PxlError::io(format!("spec-manifest.json: {e}")))?,
            ),
        }]
    } else {
        recipes
            .iter()
            .filter_map(|(_, recipe)| {
                fs::read(input.join(recipe))
                    .ok()
                    .map(|bytes| pxl_contract::DigestEntry {
                        path: recipe.clone(),
                        digest: util::hash_of(&bytes),
                    })
            })
            .collect()
    };
    let step = pxl_contract::StepEnvelope {
        schema_version: 1,
        step: "step-300-asset-build".into(),
        step_version: pxl_core::PXL_VERSION.into(),
        contract: "asset-build/v5".into(),
        status: if failures == 0 {
            pxl_contract::StepStatus::Ok
        } else {
            pxl_contract::StepStatus::Failed
        },
        scope: if args.scope == "slice" {
            pxl_contract::Scope::Slice
        } else {
            pxl_contract::Scope::All
        },
        inputs,
        outputs: vec![pxl_contract::DigestEntry {
            path: "index.json".into(),
            digest: util::hash_of(text.as_bytes()),
        }],
        verdict: gates.verdict(),
        gates,
        error: None,
    };
    let step_text = step
        .to_json_string()
        .map_err(|e| PxlError::io(format!("step.json: {e}")))?;
    fs::write(out_root.join("step.json"), step_text)
        .map_err(|e| PxlError::io(format!("step.json: {e}")))?;

    let mut body = util::ok_base("plan");
    body.insert("out".into(), json!(args.out));
    body.insert("scope".into(), json!(args.scope));
    body.insert("built".into(), json!(ok_count));
    body.insert("failed".into(), json!(failures));
    body.insert("index".into(), json!(index_path.to_string_lossy()));
    if let Some(audio) = index.get("audio") {
        body.insert("audio".into(), audio.clone());
    }
    util::emit_ok(ctx, &Value::Object(body));
    if failures == 0 {
        Ok(())
    } else {
        let total = index["recipes"].as_array().map_or(0, Vec::len)
            + usize::from(index.get("audio").is_some());
        Err(CmdError::new(
            PxlError::qa(format!("plan: {failures} of {total} build item(s) failed"))
                .with_hint("read index.json; every failed entry carries its error"),
        )
        .with_extra(json!({"cmd": "plan", "failed": failures, "built": ok_count})))
    }
}

fn audio_spec_path(input: &Path) -> CmdResult<Option<PathBuf>> {
    let manifest = input.join("spec-manifest.json");
    if !manifest.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&manifest)
        .map_err(|e| PxlError::io(format!("spec-manifest.json: {e}")))?;
    let value: Value = pxl_format::parse_json(&text, "spec-manifest")
        .map_err(|e| PxlError::schema(format!("{}: {}", manifest.display(), e.message)))?;
    let Some(audio) = value.get("audio").filter(|audio| !audio.is_null()) else {
        return Ok(None);
    };
    let spec = audio
        .get("spec")
        .and_then(Value::as_str)
        .ok_or_else(|| PxlError::schema("spec-manifest audio.spec is required"))?;
    if spec != "audio/audio.spec.json" {
        return Err(PxlError::schema(format!(
            "unsupported audio spec path '{spec}'; expected audio/audio.spec.json"
        ))
        .into());
    }
    Ok(Some(input.join(spec)))
}

fn dry_run_audio(path: &Path) -> Value {
    match pxl_audio::validate::validate_spec(path) {
        Ok(validated) => json!({
            "contract": pxl_audio::limits::SPEC_CONTRACT,
            "status": "ok",
            "sfx": validated.spec.sfx.len(),
            "bgm": validated.spec.bgm.len(),
            "dependency_bytes": validated.dependency_bytes,
        }),
        Err(error) => json!({
            "contract": pxl_audio::limits::SPEC_CONTRACT,
            "status": "failed",
            "error": error.message,
        }),
    }
}

fn build_audio(ctx: &Ctx, spec: &Path, out_root: &Path) -> Value {
    let options = pxl_audio::render::RenderOptions {
        out: out_root.join("audio"),
        overwrite: true,
        strict: ctx.strict,
        seed_override: ctx.seed,
        only: None,
        adapter: None,
    };
    match pxl_audio::render::render_spec(spec, &options) {
        Ok(outcome) => json!({
            "contract": pxl_audio::limits::SPEC_CONTRACT,
            "status": "ok",
            "out": "audio/",
            "manifest": pxl_audio::render::MANIFEST_FILE,
            "entries": outcome.manifest.entries.len(),
            "warnings": outcome.warnings,
        }),
        Err(error) => json!({
            "contract": pxl_audio::limits::SPEC_CONTRACT,
            "status": "failed",
            "out": "audio/",
            "error": error.message,
            "audio_error": format!("{:?}", error.kind).to_lowercase(),
        }),
    }
}

/// Collect `(id, recipe path relative to input)` pairs. A spec-manifest.json
/// wins when present (the step-200 contract); otherwise the layout
/// `<id>/<id>.recipe.json` and top-level `*.recipe.json` are scanned.
fn discover_recipes(input: &Path) -> CmdResult<Vec<(String, String)>> {
    let manifest = input.join("spec-manifest.json");
    if manifest.is_file() {
        let text = fs::read_to_string(&manifest)
            .map_err(|e| PxlError::io(format!("spec-manifest.json: {e}")))?;
        let value: Value = pxl_format::parse_json(&text, "spec-manifest")
            .map_err(|e| PxlError::schema(format!("{}: {}", manifest.display(), e.message)))?;
        let mut out = Vec::new();
        for asset in value
            .get("assets")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            if let Some(recipe) = asset.get("recipe").and_then(Value::as_str) {
                let id = asset.get("id").and_then(Value::as_str).unwrap_or_else(|| {
                    Path::new(recipe)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("asset")
                });
                out.push((id.to_string(), recipe.to_string()));
            }
        }
        if out.is_empty() {
            return Err(PxlError::semantic("spec-manifest.json lists no recipes")
                .with_hint("every entry needs an `id` and a `recipe` path")
                .into());
        }
        return Ok(out);
    }
    let mut out = Vec::new();
    let mut push_dir = |dir: &Path, depth: u32| -> CmdResult<()> {
        let entries = fs::read_dir(dir)
            .map_err(|e| PxlError::io(format!("cannot read {}: {e}", dir.display())))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|n| n.ends_with(".recipe.json"))
            {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("asset")
                    .trim_end_matches(".recipe")
                    .to_string();
                let rel = path
                    .strip_prefix(input)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                let _ = depth;
                out.push((stem, rel));
            }
        }
        Ok(())
    };
    push_dir(input, 0)?;
    // One level of <id>/ subdirectories, the step-200 layout.
    let children: Vec<PathBuf> = fs::read_dir(input)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    for child in children {
        push_dir(&child, 1)?;
    }
    Ok(out)
}

/// Parse the recipe and check every input it names, writing nothing. Returns
/// the entry for index.json (or the dry-run report).
fn dry_run_one(input: &Path, recipe_rel: &str, id: &str) -> Value {
    match validate_recipe(input, recipe_rel) {
        Ok(mut report) => {
            report["id"] = json!(id);
            report["recipe"] = json!(recipe_rel);
            report["status"] = json!("ok");
            report
        }
        Err(err) => json!({
            "id": id,
            "recipe": recipe_rel,
            "status": if err.code() == ErrorCode::Qa { "qa_failed" } else { "parse_error" },
            "error": err.err.message,
            "code": err.err.code.as_str(),
            "diagnostics": [{"check_id": "semantic_compile", "expected": "valid in-memory document", "actual": err.err.message, "hint": err.err.hint}],
        }),
    }
}

fn validate_recipe(input: &Path, recipe_rel: &str) -> CmdResult<Value> {
    let recipe_path = input.join(recipe_rel);
    let text = fs::read_to_string(&recipe_path)
        .map_err(|e| PxlError::io(format!("cannot read {}: {e}", recipe_path.display())))?;
    let recipe: Recipe = pxl_format::parse_json(&text, "recipe")
        .map_err(|e| PxlError::schema(format!("{}: {}", recipe_path.display(), e.message)))?;
    if recipe.gen_version != pxl_core::GEN_VERSION {
        return Err(PxlError::usage(format!(
            "unsupported gen_version {}; expected {}",
            recipe.gen_version,
            pxl_core::GEN_VERSION
        ))
        .into());
    }
    let recipe_dir = recipe_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let visual = super::visual::VisualContext::for_recipe(&recipe_path)?;
    // Same threshold source as the real build, so a dry-run verdict equals
    // the build verdict.
    let thresholds = visual.as_ref().map(|context| context.qa_thresholds());
    let mut palette = util::load_palette(&recipe.palette, &recipe_dir)?;
    palette.check_text_encodable()?;
    if let Some(context) = visual.as_ref() {
        context.check_palette_budget(&palette)?;
    }
    let seed = recipe.seed;
    let stored_palette = Path::new(&recipe.palette)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("palette.json");
    let mut sprites = Vec::new();
    let mut checks = Vec::new();
    let mut warnings = Vec::new();
    let mut docs = Vec::new();
    let mut visual_reports = Vec::new();
    for sprite in &recipe.sprites {
        let built = super::sprite::build_sprite(
            sprite,
            &palette,
            stored_palette,
            seed,
            &recipe,
            &recipe_dir,
            &recipe_path,
            visual.as_ref(),
        )?;
        palette = built.palette;
        if let Some(report) = built.visual {
            visual_reports.push(report);
        }
        docs.push(built.doc);
        sprites.push(sprite.id.clone());
    }
    for tile in &recipe.tiles {
        let mut pairs = Vec::new();
        for index in 0..tile.variants.max(1) {
            let frame = pxl_gen::generate_noise_tile(
                &tile.kind,
                tile.size,
                pxl_gen::variant_seed(tile.seed, index),
                &palette,
                tile.ramp.as_deref(),
            )?;
            pairs.push((format!("tile_{index}"), frame));
        }
        let mut doc = super::document::doc_from_frames(stored_palette, &pairs, tile.seed, "tiles")?;
        doc.meta.asset = Some(tile.id.clone());
        doc.meta.tile_ramp = tile.ramp.clone();
        if let Some(context) = &visual {
            let application = context.apply(&mut doc, &palette, &recipe_dir)?;
            palette = application.palette;
            visual_reports.push(application.report);
        }
        docs.push(doc);
        sprites.push(tile.id.clone());
    }
    for doc in &docs {
        let mut input = super::qa_input::doc_input(doc, &palette)?;
        if let Some(sprite) = recipe
            .sprites
            .iter()
            .find(|sprite| Some(&sprite.id) == doc.meta.asset.as_ref())
        {
            super::qa_input::apply_recipe_contract(&mut input, sprite)?;
        }
        let mut config = if let Some(thresholds) = &thresholds {
            let mut config = pxl_qa::QaConfig::for_policy(thresholds);
            if doc.w >= 32 && doc.h >= 32 {
                if let Ok(profile) = pxl_core::SizeProfile::for_size(doc.w, doc.h) {
                    let tier = pxl_qa::QaConfig::for_profile(&profile);
                    config.max_components = tier.max_components;
                    config.max_isolated_pixels = tier.max_isolated_pixels;
                    config.max_color_specks = tier.max_color_specks;
                    config.min_content_coverage_bps = tier.min_content_coverage_bps;
                    config.copy2x_warn_bps = tier.copy2x_warn_bps;
                }
            }
            config
        } else if doc.w >= 32 && doc.h >= 32 {
            pxl_qa::QaConfig::for_profile(&pxl_core::SizeProfile::for_size(doc.w, doc.h)?)
        } else {
            pxl_qa::QaConfig::default()
        };
        config.strict = recipe
            .validate
            .as_ref()
            .is_some_and(|v| v.rules == "strict");
        let report = pxl_qa::validate_with_config(&input, &config)?;
        warnings.extend(
            report
                .checks
                .iter()
                .filter(|check| !check.pass && check.severity != pxl_qa::Severity::Error)
                .map(|check| check.id.clone()),
        );
        checks.extend(report.checks.iter().map(|check| {
            json!({
                "id": check.id,
                "pass": check.pass,
                "severity": check.severity,
                "expected": check.limit,
                "actual": check.value,
            })
        }));
        if !report.ok() {
            return Err(CmdError::new(PxlError::qa(format!(
                "dry-run QA failed for '{}'",
                doc.meta.asset.clone().unwrap_or_else(|| "asset".into())
            )))
            .with_extra(json!({"checks": checks})));
        }
    }
    let dependencies = super::dependencies::recipe_dependencies(input, &recipe_path)?;
    let documents = docs.iter().map(|doc| {
        let anims = doc.anims.iter().map(|(name, anim)| {
            let dirs = anim.dir_list();
            let frames = if dirs.is_empty() {
                anim.frames.sequence(None).map(|v| v.len()).unwrap_or(0)
            } else {
                anim.frames.sequence(Some(&dirs[0])).map(|v| v.len()).unwrap_or(0)
            };
            json!({"name": name, "fps": anim.fps, "looped": anim.looped, "dirs": dirs, "frames": frames, "mode": anim.mode})
        }).collect::<Vec<_>>();
        json!({
            "asset": doc.meta.asset,
            "kind": doc.meta.kind,
            "size_px": [doc.w, doc.h],
            "anims": anims,
        })
    }).collect::<Vec<_>>();
    Ok(json!({
        "sprites": sprites,
        "documents": documents,
        "checks": checks,
        "warnings": warnings,
        "dependencies": dependencies,
        "effective_style": visual.map(|context| context.effective_style(visual_reports)),
    }))
}

/// Build one recipe through the standard pipeline and collect its entry.
pub(super) fn build_one(
    ctx: &Ctx,
    args: &PlanArgs,
    input: &Path,
    out_root: &Path,
    recipe_rel: &str,
    id: &str,
) -> Value {
    let dest = out_root.join(id);
    let build_args = BuildArgs {
        recipe: input.join(recipe_rel).to_string_lossy().into_owned(),
        out: dest.to_string_lossy().into_owned(),
        qa_config: args.qa_config.clone(),
    };
    match build_cmd(ctx, build_args) {
        Ok(()) => {
            // build_cmd published its .build.json manifest; index.json points
            // at it so the package step never has to guess the layout.
            let manifest = find_manifest(&dest);
            json!({
                "id": id,
                "recipe": recipe_rel,
                "status": "ok",
                "out": format!("{id}/"),
                "manifest": manifest,
            })
        }
        Err(err) => {
            let status = if err.code() == ErrorCode::Qa {
                "qa_failed"
            } else {
                "parse_error"
            };
            json!({
                "id": id,
                "recipe": recipe_rel,
                "status": status,
                "out": format!("{id}/"),
                "error": err.err.message,
                "code": err.err.code.as_str(),
            })
        }
    }
}

/// The build wrote exactly one `<stem>.build.json`; locate it without
/// re-deriving the sheet name.
fn find_manifest(dest: &Path) -> String {
    let fallback = manifest_file_name("sheet.png");
    fs::read_dir(dest)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .find(|n| n.ends_with(".build.json"))
        })
        .ok()
        .flatten()
        .unwrap_or(fallback)
}
