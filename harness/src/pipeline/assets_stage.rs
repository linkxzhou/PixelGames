//! Assets stage (H2): the artist turns a frozen design into pxl recipes,
//! builds them, and stops when every required asset id is on a sheet.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::agent::provider::ModelProvider;
use crate::agent::role::artist;
use crate::agent::runner::{AgentRunner, AgentTask};
use crate::agent::tools::{ToolContext, ToolDef, ToolOutcome, ToolRegistry};
use crate::config::Config;
use crate::contracts::assets::{AssetManifest, DeliveryStatus, VisualReviewState};
use crate::contracts::design::AssetRequirements;
use crate::contracts::ContractHeader;
use crate::digest::{digest_bytes, digest_file};
use crate::error::{HarnessError, Result};
use crate::pipeline::state::set_stage;
use crate::policy::budget::BudgetLedger;
use crate::policy::{Permission, PermissionSet};
use crate::qa::{issues_from_report, GateCheck, GateReport, GateStatus, GATE_REPORT_VERSION};
use crate::storage::{ArtifactStore, Event, EventLog, RunState, RunStore, StageStatus};

pub fn registry() -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    reg.register(ToolDef {
        name: "design.read_frozen".into(),
        description: "Read the approved design package (game design, style, requirements).".into(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
        needed: Permission::ReadProject,
        run: Box::new(|_args, ctx| {
            let Some(dir) = &ctx.design_dir else {
                return Ok(ToolOutcome::fail("no frozen design revision is mounted"));
            };
            let mut files = serde_json::Map::new();
            for name in [
                "game-design.json",
                "style-guide.json",
                "asset-requirements.json",
                "acceptance-plan.json",
            ] {
                let path = dir.join(name);
                if path.is_file() {
                    let text = fs::read_to_string(&path)
                        .map_err(|e| HarnessError::io(e.to_string()))?;
                    files.insert(name.into(), Value::String(text));
                }
            }
            Ok(ToolOutcome::ok("frozen design loaded").with_data(Value::Object(files)))
        }),
    });
    reg.register(ToolDef {
        name: "assets.read_requirements".into(),
        description: "List required asset ids from the frozen design.".into(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
        needed: Permission::ReadProject,
        run: Box::new(|_args, ctx| {
            let reqs = load_requirements(ctx)?;
            let ids: Vec<Value> = reqs
                .requirements
                .iter()
                .filter(|r| r.required)
                .map(|r| {
                    // Full requirement context: animations, fps, looping,
                    // seamless tiling, semantics and priority. Without anims
                    // the model cannot know how many frames to author.
                    let anims: Vec<Value> = r
                        .anims
                        .iter()
                        .map(|a| {
                            serde_json::json!({
                                "name": a.name,
                                "fps": a.fps,
                                "looped": a.looped,
                                "frames": a.frames,
                                "dirs": a.dirs,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "asset_id": r.asset_id,
                        "kind": r.kind,
                        "strategy": r.strategy,
                        "size_px": r.size_px,
                        "directions": r.directions,
                        "anims": anims,
                        "seamless": r.seamless,
                        "semantics": r.semantics,
                        "priority": r.priority,
                        "style_ref": r.style_ref,
                        "consumer_refs": r.consumer_refs,
                    })
                })
                .collect();
            Ok(ToolOutcome::ok(format!("{} required assets", ids.len()))
                .with_data(serde_json::json!({"required": ids})))
        }),
    });
    reg.register(ToolDef {
        name: "assets.read_candidate".into(),
        description: "Read one text file already in the attempt directory.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["path"],
            "properties": {"path": {"type": "string"}}
        }),
        needed: Permission::ReadProject,
        run: Box::new(|args, ctx| {
            let rel = args["path"].as_str().unwrap_or_default();
            let root = crate::policy::path::WriteRoot::new(&ctx.attempt_dir);
            let path = root.resolve_write(rel)?;
            if !path.is_file() {
                return Ok(ToolOutcome::fail(format!("no such file {rel}")));
            }
            let text = fs::read_to_string(&path).map_err(|e| HarnessError::io(e.to_string()))?;
            let clipped = clip(&text, 24_000);
            Ok(ToolOutcome::ok(format!("read {rel} ({} bytes)", text.len()))
                .with_data(serde_json::json!({"path": rel, "content": clipped})))
        }),
    });
    reg.register(ToolDef {
        name: "assets.write_candidate".into(),
        description: "Write a recipe, shapes spec, or palette JSON file inside the attempt.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            }
        }),
        needed: Permission::WriteCandidate,
        run: Box::new(|args, ctx| {
            let rel = args["path"].as_str().unwrap_or_default();
            let content = args["content"].as_str().unwrap_or_default();
            write_candidate(ctx, rel, content)
        }),
    });
    reg.register(ToolDef {
        name: "assets.build_candidate".into(),
        description: "Run pxl build on one recipe. out_dir is relative to the attempt, for example out/units.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["recipe"],
            "properties": {
                "recipe": {"type": "string"},
                "out_dir": {"type": "string"}
            }
        }),
        needed: Permission::Pxl,
        run: Box::new(|args, ctx| {
            let recipe = args["recipe"].as_str().unwrap_or_default();
            let stem = Path::new(recipe)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("build");
            let out_rel = args["out_dir"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("out/{stem}"));
            build_candidate(ctx, recipe, &out_rel)
        }),
    });
    reg.register(ToolDef {
        name: "assets.validate_candidate".into(),
        description: "Run pxl validate on a sheet JSON. rules is tank, default, or character.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["sheet", "rules"],
            "properties": {
                "sheet": {"type": "string"},
                "rules": {"type": "string", "enum": ["tank", "default", "character"]}
            }
        }),
        needed: Permission::Pxl,
        run: Box::new(|args, ctx| {
            let sheet = args["sheet"].as_str().unwrap_or_default();
            let rules = args["rules"].as_str().unwrap_or("default");
            validate_candidate(ctx, sheet, rules)
        }),
    });
    reg.register(ToolDef {
        name: "assets.list_capabilities".into(),
        description: "pxl list: generators, ops, macros, templates, or palettes.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["kind"],
            "properties": {"kind": {"type": "string"}}
        }),
        needed: Permission::Pxl,
        run: Box::new(|args, ctx| {
            let kind = args["kind"].as_str().unwrap_or("generators");
            let result = ctx.pxl.list(kind)?;
            let body = clip(&result.stdout, 8_000);
            if result.ok {
                Ok(ToolOutcome::ok(format!("listed {kind}")).with_data(serde_json::json!({"stdout": body})))
            } else {
                Ok(ToolOutcome::fail(format!("{} {}", result.error_code, result.message))
                    .with_data(serde_json::json!({"stdout": body})))
            }
        }),
    });
    reg.register(ToolDef {
        name: "assets.schema".into(),
        description: "Read the pxl schema for recipe, shapes, doc, or palette. Use it before writing a recipe to check field names.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["kind"],
            "properties": {"kind": {"type": "string", "enum": ["recipe", "shapes", "doc", "palette"]}}
        }),
        needed: Permission::Pxl,
        run: Box::new(|args, ctx| {
            let kind = args["kind"].as_str().unwrap_or("recipe");
            // Bounded allow-list: the model gets schemas, never CLI internals.
            if !matches!(kind, "recipe" | "shapes" | "doc" | "palette") {
                return Ok(ToolOutcome::fail(format!("unknown schema kind {kind}")));
            }
            match ctx.pxl.schema(kind) {
                Ok(schema) => {
                    let body = clip(&serde_json::to_string(&schema).unwrap_or_default(), 16_000);
                    Ok(ToolOutcome::ok(format!("schema for {kind}")).with_data(serde_json::json!({"schema": body})))
                }
                Err(e) => Ok(ToolOutcome::fail(format!("schema {kind} unavailable: {}", e.message))),
            }
        }),
    });
    reg.register(ToolDef {
        name: "assets.status".into(),
        description: "Show the harness-managed check queue: which assets were written but never built, which failed, and the next todo.".into(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
        needed: Permission::ReadProject,
        run: Box::new(|_args, ctx| {
            let queue = crate::pipeline::asset_queue::AssetQueue::load(&ctx.attempt_dir);
            let summary = queue.status_summary();
            let states: Vec<Value> = queue
                .entries
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "asset_id": e.asset_id,
                        "state": crate::pipeline::asset_queue::state_name(e.state),
                        "missing": e.missing,
                        "last_error_code": e.last_error_code,
                        "last_failure": e.last_failure,
                        "last_report": e.last_report,
                        "build_attempts": e.build_attempts,
                        "repair_attempts": e.repair_attempts,
                    })
                })
                .collect();
            Ok(ToolOutcome::ok(format!("{} assets tracked", states.len()))
                .with_data(serde_json::json!({
                    "queue": summary,
                    "entries": states,
                })))
        }),
    });
    reg.register(ToolDef {
        name: "assets.request_publish".into(),
        description: "Build asset-manifest.json from the sheets on disk and request publication.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["summary"],
            "properties": {"summary": {"type": "string"}}
        }),
        needed: Permission::WriteCandidate,
        run: Box::new(|args, ctx| {
            let summary = args["summary"].as_str().unwrap_or_default();
            if summary.trim().is_empty() {
                return Ok(ToolOutcome::fail("summary must not be empty"));
            }
            request_publish(ctx, summary)
        }),
    });
    reg
}

fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

pub fn write_candidate(ctx: &ToolContext, rel: &str, content: &str) -> Result<ToolOutcome> {
    if rel.ends_with(".png") || rel.ends_with(".gif") || rel.contains("out/") {
        return Ok(ToolOutcome::fail(
            "write recipes and specs only; pxl owns out/ and image bytes",
        ));
    }
    let allowed = rel.ends_with(".json") || rel.ends_with(".md");
    if !allowed || rel.contains("asset-manifest.json") {
        return Ok(ToolOutcome::fail(
            "path must be a recipe, shapes, or palette file (not the manifest)",
        ));
    }
    let root = crate::policy::path::WriteRoot::new(&ctx.attempt_dir);
    let path = root.resolve_write(rel)?;
    if rel.ends_with(".json") {
        serde_json::from_str::<Value>(content)
            .map_err(|e| HarnessError::usage(format!("{rel}: invalid JSON: {e}")))?;
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| HarnessError::io(e.to_string()))?;
    }
    if let Some(hit) = template_json_match(ctx, content) {
        return Ok(ToolOutcome::fail(format!(
            "refusing to write {rel}: it is the same JSON document as template file {hit}. Author an original spec."
        )));
    }
    let digest = digest_bytes(content.as_bytes());
    fs::write(&path, content).map_err(|e| HarnessError::io(e.to_string()))?;
    // Writing specs only marks the queue dirty; building happens via the
    // harness-managed pending-check queue (never a silent success).
    let file_name = Path::new(rel)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let reqs = load_requirements(ctx).ok();
    let required: Vec<String> = reqs
        .as_ref()
        .map(|r| {
            r.requirements
                .iter()
                .filter(|a| a.required)
                .map(|a| a.asset_id.clone())
                .collect()
        })
        .unwrap_or_default();
    let affected = crate::pipeline::asset_queue::AssetQueue::affected_ids(file_name, &required);
    if !affected.is_empty() {
        let mut queue = crate::pipeline::asset_queue::AssetQueue::load(&ctx.attempt_dir);
        queue.seed(&required);
        if file_name == "palette.json" {
            queue.mark_all_dirty(&ctx.attempt_dir);
        } else {
            for id in &affected {
                queue.mark_dirty(id, &ctx.attempt_dir);
            }
        }
        queue.save(&ctx.attempt_dir)?;
    }
    Ok(ToolOutcome::ok(format!("wrote {rel}"))
        .with_data(serde_json::json!({
            "path": rel,
            "digest": digest,
        }))
        .with_artifact(rel, digest, content.len() as u64))
}

fn find_template_assets(ctx: &ToolContext) -> Option<PathBuf> {
    for root in &ctx.template_roots {
        for dir in [
            root.join("templates").join(&ctx.genre).join("assets"),
            root.join(&ctx.genre).join("assets"),
            root.join("assets"),
        ] {
            if dir.is_dir() {
                return Some(dir);
            }
        }
    }
    None
}

/// Template specs the model may study as a format example, but must not
/// submit as its own output. Comparison is JSON equality, so reformatting
/// the bundled file still counts as a copy.
fn template_documents(ctx: &ToolContext) -> Vec<(String, Value)> {
    let Some(dir) = find_template_assets(ctx) else {
        return Vec::new();
    };
    let Ok(rd) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut docs = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("template")
                .to_string();
            docs.push((name, value));
        }
    }
    docs
}

fn template_json_match(ctx: &ToolContext, content: &str) -> Option<String> {
    let Ok(value) = serde_json::from_str::<Value>(content) else {
        return None;
    };
    template_documents(ctx)
        .into_iter()
        .find(|(_, doc)| doc == &value)
        .map(|(name, _)| name)
}

fn authored_spec_files(attempt: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(attempt) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                return false;
            };
            name == "palette.json" || name.ends_with(".shapes.json") || name.ends_with(".recipe.json")
        })
        .collect();
    files.sort();
    files
}

fn build_candidate(ctx: &ToolContext, recipe: &str, out_rel: &str) -> Result<ToolOutcome> {
    let root = crate::policy::path::WriteRoot::new(&ctx.attempt_dir);
    let recipe_path = root.resolve_write(recipe)?;
    let out_dir = root.resolve_write(out_rel)?;
    if !recipe_path.is_file() {
        return Ok(ToolOutcome::fail(format!("recipe {recipe} is not in the attempt")));
    }
    fs::create_dir_all(&out_dir).map_err(|e| HarnessError::io(e.to_string()))?;
    let built = ctx
        .pxl
        .build(&recipe_path, &out_dir, None, Some(&ctx.attempt_dir))?;
    // Unified rendering: full check objects (value/limit), never bare "failed".
    let failed = crate::adapters::pxl::render_failed_checks(&built.checks, 8);
    // Record the result in the pending-check queue so explicit builds and
    // auto-checks share one state machine.
    queue_record_build(ctx, recipe, out_rel, &built);
    let data = serde_json::json!({
        "ok": built.qa_ok(),
        "out_dir": out_rel,
        "outputs": built.outputs,
        "message": built.result.message,
        "error_code": built.result.error_code,
        "hint": built.result.hint,
        "failed_checks": failed,
    });
    if built.qa_ok() {
        Ok(ToolOutcome::ok(format!("built {recipe} -> {out_rel}")).with_data(data))
    } else {
        Ok(ToolOutcome::fail(format!(
            "pxl build failed for {recipe} ({}) {}{}",
            built.result.error_code,
            built.result.message,
            built
                .result
                .hint
                .as_ref()
                .map(|h| format!(" -- hint: {h}"))
                .unwrap_or_default()
        ))
        .with_data(data)
        .with_pxl_result(&built.result))
    }
}

/// Mirror an explicit build into the queue state so coverage and the repair
/// budget reflect every real build, not only harness-initiated ones.
fn queue_record_build(
    ctx: &ToolContext,
    recipe: &str,
    out_rel: &str,
    built: &crate::adapters::pxl::PxlBuildOutput,
) {
    let reqs = load_requirements(ctx).ok();
    let required: Vec<String> = reqs
        .as_ref()
        .map(|r| {
            r.requirements
                .iter()
                .filter(|a| a.required)
                .map(|a| a.asset_id.clone())
                .collect()
        })
        .unwrap_or_default();
    let stem = Path::new(recipe)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let Some(asset_id) = required.iter().find(|r| r.as_str() == stem) else {
        return;
    };
    let mut queue = crate::pipeline::asset_queue::AssetQueue::load(&ctx.attempt_dir);
    queue.seed(&required);
    let fingerprint = crate::pipeline::asset_queue::input_fingerprint(
        &ctx.attempt_dir,
        asset_id,
        recipe,
    );
    if let Some(entry) = queue.entries.iter_mut().find(|e| &e.asset_id == asset_id) {
        entry.out_dir = out_rel.to_string();
        // Dedup by fingerprint even for explicit builds.
        if !entry.built_fingerprints.iter().any(|f| f == &fingerprint) {
            entry.built_fingerprints.push(fingerprint.clone());
        }
    }
    if built.qa_ok() {
        queue.record_success(asset_id);
    } else {
        let blocked = queue.record_failure_and_count(asset_id, &built.result);
        if blocked {
            // Blocked assets stop consuming the repair budget; the stage
            // round note will surface this to the model.
        }
    }
    let _ = queue.save(&ctx.attempt_dir);
}

fn validate_candidate(ctx: &ToolContext, sheet: &str, rules: &str) -> Result<ToolOutcome> {
    let root = crate::policy::path::WriteRoot::new(&ctx.attempt_dir);
    let path = root.resolve_write(sheet)?;
    if !path.is_file() {
        return Ok(ToolOutcome::fail(format!("sheet {sheet} does not exist")));
    }
    let result = ctx.pxl.validate(&path, rules)?;
    // The CLI `checks_json` emits {id, pass, value, limit}; render from those
    // objects only. Never assume message/detail exist.
    let failed = crate::adapters::pxl::render_failed_checks(&result.checks(), 12);
    let data = serde_json::json!({
        "ok": result.ok,
        "error_code": result.error_code,
        "message": result.message,
        "hint": result.hint,
        "failed_checks": failed,
    });
    if result.ok {
        Ok(ToolOutcome::ok(format!("validated {sheet} ({rules})")).with_data(data))
    } else {
        Ok(ToolOutcome::fail(format!(
            "pxl validate failed ({}) {}{}",
            result.error_code,
            result.message,
            result
                .hint
                .as_ref()
                .map(|h| format!(" -- hint: {h}"))
                .unwrap_or_default()
        ))
        .with_data(data)
        .with_pxl_result(&result))
    }
}

fn request_publish(ctx: &ToolContext, summary: &str) -> Result<ToolOutcome> {
    let reqs = load_requirements(ctx).ok();
    let required: Vec<String> = reqs
        .as_ref()
        .map(|r| {
            r.requirements
                .iter()
                .filter(|a| a.required)
                .map(|a| a.asset_id.clone())
                .collect()
        })
        .unwrap_or_default();
    let frames = frame_index(&ctx.attempt_dir, &required);
    let missing: Vec<String> = required
        .iter()
        .filter(|id| !frames.contains_key(id.as_str()))
        .cloned()
        .collect();
    let revision = ctx
        .design_dir
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    // Only assets whose *current* inputs produced a successful build may be
    // published. A leftover PNG from an older, now-failing recipe is not
    // evidence of delivery.
    let queue = crate::pipeline::asset_queue::AssetQueue::load(&ctx.attempt_dir);
    let mut stale: Vec<String> = Vec::new();
    let mut entries = Vec::new();
    for id in &required {
        let Some(hits) = frames.get(id.as_str()) else {
            continue;
        };
        // Reject residue: not built, built for other inputs, or failed.
        match queue.entry(id) {
            Some(e)
                if matches!(
                    e.state,
                    crate::pipeline::asset_queue::AssetState::Built
                        | crate::pipeline::asset_queue::AssetState::Validated
                ) => {}
            Some(e) => {
                stale.push(format!(
                    "{id}: {} (no successful build for current inputs)",
                    crate::pipeline::asset_queue::state_name(e.state)
                ));
                continue;
            }
            None => {
                stale.push(format!("{id}: no build recorded"));
                continue;
            }
        }
        let recipe = recipe_for(&ctx.attempt_dir, id).unwrap_or_else(|| "unknown.recipe.json".into());
        let recipe_path = ctx.attempt_dir.join(&recipe);
        let recipe_digest = if recipe_path.is_file() {
            digest_file(&recipe_path).unwrap_or_else(|_| "sha256:missing".into())
        } else {
            "sha256:missing".into()
        };
        let palette = ctx.attempt_dir.join("palette.json");
        let palette_digest = if palette.is_file() {
            digest_file(&palette).unwrap_or_else(|_| "sha256:missing".into())
        } else {
            "sha256:missing".into()
        };
        let mut outputs = Vec::new();
        for png in &hits.sheets {
            if png.is_file() {
                outputs.push((
                    rel_to(&ctx.attempt_dir, png),
                    digest_file(png).unwrap_or_else(|_| "sha256:missing".into()),
                ));
            }
        }
        outputs.sort();
        outputs.dedup();
        let size = reqs
            .as_ref()
            .and_then(|r| {
                r.requirements
                    .iter()
                    .find(|a| a.asset_id == *id)
                    .map(|a| a.size_px)
            })
            .unwrap_or([16, 16]);
        // Real provenance: seed and animations come from the recipe, and the
        // shapes spec is listed as an input. Never fake a fixed seed.
        let recipe_doc: Option<Value> = fs::read_to_string(&recipe_path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok());
        let seed = recipe_doc
            .as_ref()
            .and_then(|v| v.get("seed").and_then(|s| s.as_u64()))
            .unwrap_or(0);
        // Real spec provenance: (path, digest) pairs, not an empty list.
        let mut spec_paths: Vec<(String, String)> = Vec::new();
        let shapes_name = format!("{id}.shapes.json");
        let shapes_path = ctx.attempt_dir.join(&shapes_name);
        if shapes_path.is_file() {
            spec_paths.push((
                shapes_name,
                digest_file(&shapes_path).unwrap_or_else(|_| "sha256:missing".into()),
            ));
        }
        // Animations with the real atlas frame keys per direction, so
        // "some {id}_* frame exists" can never stand in for the requirement.
        let mut animations = Vec::new();
        if let Some(doc) = &recipe_doc {
            if let Some(sprites) = doc.get("sprites").and_then(|s| s.as_array()) {
                for sprite in sprites {
                    let sid = sprite.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    if sid != id.as_str() {
                        continue;
                    }
                    if let Some(anims) = sprite.get("anims").and_then(|a| a.as_array()) {
                        for anim in anims {
                            let name = anim.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let fps = anim.get("fps").and_then(|f| f.as_u64()).unwrap_or(0) as u32;
                            let looped =
                                anim.get("loop").and_then(|l| l.as_bool()).unwrap_or(true);
                            let dirs: Vec<(String, Vec<String>)> = anim
                                .get("dirs")
                                .and_then(|d| d.as_array())
                                .map(|d| {
                                    d.iter()
                                        .filter_map(|x| x.as_str())
                                        .map(|dir| {
                                            (
                                                dir.to_string(),
                                                frame_keys_for(
                                                    &ctx.attempt_dir, id, name, dir,
                                                ),
                                            )
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            animations.push(crate::contracts::assets::ManifestAnim {
                                name: name.to_string(),
                                fps,
                                looped,
                                dirs,
                            });
                        }
                    }
                }
            }
        }
        entries.push(crate::contracts::assets::ManifestEntry {
            asset_id: id.clone(),
            status: DeliveryStatus::Delivered,
            required: true,
            recipe_path: recipe,
            recipe_digest,
            spec_paths,
            palette_path: "palette.json".into(),
            palette_digest,
            seed,
            outputs,
            animations,
            size_px: size,
            pivot: None,
        });
    }
    let manifest = AssetManifest {
        header: ContractHeader {
            schema_version: crate::contracts::assets::ASSETS_SCHEMA_VERSION,
        },
        design_revision: revision,
        assets_revision: "candidate".into(),
        pxl_version: ctx.pxl.version().to_string(),
        pxl_binary_digest: ctx.pxl.binary_digest().to_string(),
        entries,
        gates_passed: vec![],
        warnings: missing
            .iter()
            .map(|id| format!("missing frames for {id}"))
            .chain(stale.iter().map(|s| format!("stale or unverified output: {s}")))
            .collect(),
        visual_review: VisualReviewState::NotRun,
    };
    let text = serde_json::to_string_pretty(&manifest)?;
    fs::write(ctx.attempt_dir.join("asset-manifest.json"), &text)
        .map_err(|e| HarnessError::io(e.to_string()))?;
    if !missing.is_empty() {
        return Ok(ToolOutcome::fail(format!(
            "not ready to publish; missing {}",
            missing.join(", ")
        ))
        .with_data(serde_json::json!({"missing": missing})));
    }
    // Leftover outputs from earlier inputs must not be published: they are
    // residue, not delivery.
    if !stale.is_empty() {
        return Ok(ToolOutcome::fail(format!(
            "not ready to publish; {} asset(s) have outputs but no successful build for their current inputs: {}",
            stale.len(),
            stale.join("; ")
        ))
        .with_data(serde_json::json!({"stale": stale})));
    }
    let request = serde_json::json!({
        "schema_version": 1,
        "summary": summary,
        "manifest_digest": digest_bytes(text.as_bytes()),
    });
    fs::write(
        ctx.attempt_dir.join("publish-request.json"),
        serde_json::to_string_pretty(&request)?,
    )
    .map_err(|e| HarnessError::io(e.to_string()))?;
    Ok(ToolOutcome::ok("publish requested").with_data(serde_json::json!({
        "assets": required.len(),
    })))
}

/// Real atlas frame keys for one `{asset}_{anim}_{dir}` sequence, taken from
/// the sheets on disk. Empty when the frames were never produced.
fn frame_keys_for(attempt: &Path, asset_id: &str, anim: &str, dir: &str) -> Vec<String> {
    let prefix = format!("{asset_id}_{anim}_{dir}_");
    let mut keys: Vec<String> = Vec::new();
    for sheet in find_sheets(attempt) {
        let Ok(text) = fs::read_to_string(&sheet) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(frames) = value.get("frames").and_then(|f| f.as_object()) else {
            continue;
        };
        for key in frames.keys() {
            if key.starts_with(&prefix) && !keys.contains(key) {
                keys.push(key.clone());
            }
        }
    }
    keys.sort();
    keys
}

struct FrameHit {
    sheets: Vec<PathBuf>,
}

fn frame_index(attempt: &Path, ids: &[String]) -> std::collections::BTreeMap<String, FrameHit> {
    let mut index = std::collections::BTreeMap::<String, FrameHit>::new();
    for sheet in find_sheets(attempt) {
        let Ok(text) = fs::read_to_string(&sheet) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(frames) = value.get("frames").and_then(|f| f.as_object()) else {
            continue;
        };
        let png = sheet.with_extension("png");
        for name in frames.keys() {
            let Some(id) = match_asset_id(name, ids) else {
                continue;
            };
            let hit = index.entry(id).or_insert_with(|| FrameHit { sheets: Vec::new() });
            if png.is_file() && !hit.sheets.iter().any(|p| p == &png) {
                hit.sheets.push(png.clone());
            }
        }
    }
    index
}

/// Asset ids may themselves contain underscores (`player_tank`), so a frame
/// matches the longest required id that is the whole key or a `{id}_` prefix.
/// `bullet_base` must not satisfy `b`.
fn match_asset_id(frame: &str, ids: &[String]) -> Option<String> {
    ids.iter()
        .filter(|id| frame == id.as_str() || frame.starts_with(&format!("{id}_")))
        .max_by_key(|id| id.len())
        .cloned()
}

fn find_sheets(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(rd) = fs::read_dir(&cur) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("json")
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".sheet.json"))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn recipe_for(attempt: &Path, asset_id: &str) -> Option<String> {
    let Ok(rd) = fs::read_dir(attempt) else {
        return None;
    };
    let mut names: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("json")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".recipe.json"))
        })
        .collect();
    names.sort();
    for path in names {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if text.contains(&format!("\"{asset_id}\"")) {
            return path.file_name().and_then(|n| n.to_str()).map(str::to_string);
        }
    }
    None
}

fn rel_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.display().to_string())
}

fn load_requirements(ctx: &ToolContext) -> Result<AssetRequirements> {
    let Some(dir) = &ctx.design_dir else {
        return Err(HarnessError::not_found("frozen design is not mounted"));
    };
    let text = fs::read_to_string(dir.join("asset-requirements.json"))
        .map_err(|e| HarnessError::not_found(format!("asset-requirements.json: {e}")))?;
    serde_json::from_str(&text).map_err(|e| HarnessError::schema(format!("asset requirements: {e}")))
}

fn rules_for_sheet(attempt: &Path, sheet: &Path) -> String {
    let id = sheet
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let recipe = attempt.join(format!("{id}.recipe.json"));
    if let Ok(text) = fs::read_to_string(&recipe) {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if let Some(rules) = value
                .get("validate")
                .and_then(|v| v.get("rules"))
                .and_then(|r| r.as_str())
            {
                if matches!(rules, "default" | "tank" | "character") {
                    return rules.to_string();
                }
            }
        }
    }
    "default".into()
}

fn png_ok(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    bytes.len() > 32 && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

pub fn gates(ctx: &ToolContext) -> Result<GateReport> {
    let mut checks = Vec::new();
    let reqs = load_requirements(ctx).ok();
    let required: Vec<String> = reqs
        .as_ref()
        .map(|r| {
            r.requirements
                .iter()
                .filter(|a| a.required)
                .map(|a| a.asset_id.clone())
                .collect()
        })
        .unwrap_or_default();
    let frames = frame_index(&ctx.attempt_dir, &required);
    checks.push(GateCheck {
        id: "files.requirements".into(),
        passed: reqs.is_some() && !required.is_empty(),
        value: required.len().to_string(),
        limit: "frozen requirements list at least one required asset".into(),
        failures: vec![],
    });
    let manifest_path = ctx.attempt_dir.join("asset-manifest.json");
    let manifest: Option<AssetManifest> = fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    checks.push(GateCheck {
        id: "files.manifest".into(),
        passed: manifest.is_some(),
        value: if manifest.is_some() { "parsed".into() } else { "missing".into() },
        limit: "asset-manifest.json parses".into(),
        failures: vec![],
    });
    let mut missing = Vec::new();
    let mut png_failures = Vec::new();
    for id in &required {
        match frames.get(id) {
            None => missing.push(id.clone()),
            Some(hit) => {
                if !hit.sheets.iter().any(|p| png_ok(p)) {
                    png_failures.push(format!("{id}: sheet png missing or empty"));
                }
            }
        }
    }
    checks.push(GateCheck {
        id: "coverage.frames".into(),
        passed: missing.is_empty() && !required.is_empty(),
        value: if missing.is_empty() {
            "covered".into()
        } else {
            format!("{} missing", missing.len())
        },
        limit: "every required asset_id has a `{id}_*` frame on a sheet".into(),
        failures: missing.iter().map(|id| format!("missing {id}")).collect(),
    });
    // `coverage.png` only judges byte validity of outputs that exist. Missing
    // assets belong to `coverage.frames`; a valid PNG with a missing asset must
    // never be reported as "PNG is broken".
    checks.push(GateCheck {
        id: "coverage.png".into(),
        passed: png_failures.is_empty(),
        value: if png_failures.is_empty() {
            "png".into()
        } else {
            format!("{} bad", png_failures.len())
        },
        limit: "every sheet png that exists is a non-empty PNG".into(),
        failures: png_failures,
    });
    // Pending / failed / stale outputs: nothing may reach the manifest unless
    // the current input fingerprint produced it.
    let queue = crate::pipeline::asset_queue::AssetQueue::load(&ctx.attempt_dir);
    let (mut queue_gaps, mut not_attempted) = (Vec::new(), Vec::new());
    for id in &required {
        match queue.entry(id) {
            None => not_attempted.push(format!("{id}: not tracked")),
            Some(e) => match e.state {
                crate::pipeline::asset_queue::AssetState::MissingSpecs
                | crate::pipeline::asset_queue::AssetState::PendingDependencies => {
                    not_attempted.push(format!(
                        "{}: {} ({})",
                        id,
                        crate::pipeline::asset_queue::state_name(e.state),
                        e.missing.join(", ")
                    ));
                }
                crate::pipeline::asset_queue::AssetState::Blocked => {
                    queue_gaps.push(format!("{id}: blocked after {} repairs", e.repair_attempts));
                }
                crate::pipeline::asset_queue::AssetState::BuildFailed => {
                    queue_gaps.push(format!(
                        "{id}: build_failed ({})",
                        e.last_error_code.as_deref().unwrap_or("unknown")
                    ));
                }
                crate::pipeline::asset_queue::AssetState::PendingBuild => {
                    queue_gaps.push(format!("{id}: pending_build (never built)"));
                }
                crate::pipeline::asset_queue::AssetState::Built
                | crate::pipeline::asset_queue::AssetState::Validated => {}
            },
        }
    }
    let mut queue_failures = queue_gaps;
    queue_failures.extend(not_attempted);
    checks.push(GateCheck {
        id: "coverage.queue".into(),
        passed: queue_failures.is_empty(),
        value: format!("{} gaps", queue_failures.len()),
        limit: "every required asset has a successful build for its current inputs".into(),
        failures: queue_failures,
    });
    let manifest_gaps = if let Some(m) = &manifest {
        required
            .iter()
            .filter(|id| {
                !m.entries.iter().any(|e| {
                    e.asset_id == **id && e.status == DeliveryStatus::Delivered && !e.outputs.is_empty()
                })
            })
            .map(|id| format!("manifest missing delivered {id}"))
            .collect()
    } else {
        vec!["manifest missing".into()]
    };
    checks.push(GateCheck {
        id: "coverage.manifest".into(),
        passed: manifest_gaps.is_empty(),
        value: format!("{} gaps", manifest_gaps.len()),
        limit: "manifest lists every required asset as delivered with outputs".into(),
        failures: manifest_gaps,
    });
    let templates = template_documents(ctx);
    let mut copies = Vec::new();
    let mut missing_specs = Vec::new();
    for id in &required {
        for suffix in ["shapes.json", "recipe.json"] {
            let name = format!("{id}.{suffix}");
            if !ctx.attempt_dir.join(&name).is_file() {
                missing_specs.push(format!("missing {name}"));
            }
        }
    }
    if !ctx.attempt_dir.join("palette.json").is_file() {
        missing_specs.push("missing palette.json".into());
    }
    for path in authored_spec_files(&ctx.attempt_dir) {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if let Some((name, _)) = templates.iter().find(|(_, doc)| doc == &value) {
            let file = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("spec");
            copies.push(format!("{file} matches template {name}"));
        }
    }
    checks.push(GateCheck {
        id: "provenance.authored_specs".into(),
        passed: missing_specs.is_empty() && !required.is_empty(),
        value: if missing_specs.is_empty() {
            "authored".into()
        } else {
            format!("{} missing", missing_specs.len())
        },
        limit: "palette.json plus {asset_id}.shapes.json and {asset_id}.recipe.json for every required asset".into(),
        failures: missing_specs,
    });
    checks.push(GateCheck {
        id: "provenance.not_template".into(),
        passed: copies.is_empty(),
        value: if copies.is_empty() {
            "original".into()
        } else {
            format!("{} copies", copies.len())
        },
        limit: "no authored spec is the same JSON document as a bundled template file".into(),
        failures: copies,
    });
    if ctx.pxl.is_available() {
        let mut validate_failures = Vec::new();
        for sheet in find_sheets(&ctx.attempt_dir) {
            let rules = rules_for_sheet(&ctx.attempt_dir, &sheet);
            match ctx.pxl.validate(&sheet, &rules) {
                Ok(res) if res.ok => {}
                Ok(res) => {
                    let detail = clip(&format!("{} {}", res.error_code, res.message), 500);
                    let extra = clip(&res.stdout, 800);
                    validate_failures.push(format!(
                        "{} --rules {rules}: {detail} {extra}",
                        sheet.file_name().and_then(|n| n.to_str()).unwrap_or("sheet")
                    ));
                }
                Err(err) => validate_failures.push(format!(
                    "{}: {}",
                    sheet.file_name().and_then(|n| n.to_str()).unwrap_or("sheet"),
                    err.message
                )),
            }
        }
        if find_sheets(&ctx.attempt_dir).is_empty() {
            validate_failures.push("no sheet to validate".into());
        }
        checks.push(GateCheck {
            id: "pxl.validate".into(),
            passed: validate_failures.is_empty(),
            value: if validate_failures.is_empty() { "passed".into() } else { "failed".into() },
            limit: "pxl validate passes using the rules declared in the recipe".into(),
            failures: validate_failures,
        });
    }
    let passed = checks.iter().all(|c| c.passed);
    Ok(GateReport {
        report_version: GATE_REPORT_VERSION,
        gate_version: 1,
        stage: "assets".into(),
        gate_id: "assets.gates".into(),
        status: if passed { GateStatus::Passed } else { GateStatus::Failed },
        input_digest: digest_bytes(ctx.attempt_dir.to_string_lossy().as_bytes()),
        checks,
        metrics: [("required".into(), required.len() as u64)].into_iter().collect(),
        evidence: vec![],
        tool_versions: [
            ("harness".into(), crate::VERSION.into()),
            ("pxl".into(), ctx.pxl.version().into()),
        ]
        .into_iter()
        .collect(),
        executed_ms: 0,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_assets_stage(
    provider: &mut dyn ModelProvider,
    cfg: &Config,
    artifacts: &ArtifactStore,
    run: &RunStore,
    state: &mut RunState,
    ledger: &mut BudgetLedger,
    events: &mut EventLog,
    template_roots: &[PathBuf],
    pxl: std::sync::Arc<crate::adapters::PxlAdapter>,
) -> Result<StageStatus> {
    set_stage(state, "assets", StageStatus::Running)?;
    run.save(state)?;
    let identity = artifacts.identity()?;
    let revision = identity.design_revision.clone().ok_or_else(|| {
        HarnessError::blocked("assets stage needs an approved design revision")
            .with_hint("pxl-harness approve --stage design --revision <rev>")
    })?;
    if !artifacts.approval_valid("design", &revision)? {
        return Err(HarnessError::blocked(
            "design revision is not approved",
        )
        .with_hint("pxl-harness approve --stage design --revision <rev>"));
    }
    let design_dir = artifacts.revision_dir("design", &revision);
    let attempt_dir = run.attempt_dir("assets", "asset_package", 1)?;
    let registry = registry();
    let role = artist();
    let ctx = ToolContext {
        host: PermissionSet::default()
            .allow(Permission::ReadProject)
            .allow(Permission::WriteCandidate)
            .allow(Permission::Pxl),
        stage: PermissionSet::default()
            .allow(Permission::ReadProject)
            .allow(Permission::WriteCandidate)
            .allow(Permission::Pxl),
        role: role.permission_set(),
        attempt_dir: attempt_dir.clone(),
        template_roots: template_roots.to_vec(),
        genre: identity.genre_template.clone(),
        design_dir: Some(design_dir.clone()),
        pxl,
    };
    let req_text = fs::read_to_string(design_dir.join("asset-requirements.json"))?;
    let base_prompt = format!(
        "Author an original asset package for design revision {revision}. \
Do not copy bundled template files. The harness rejects any spec whose JSON equals a template file. \
Write palette.json, then for every required asset_id write {{id}}.shapes.json and {{id}}.recipe.json yourself. \
Build each recipe with assets.build_candidate (out_dir out/{{id}}) and assets.validate_candidate \
with rules default. player_tank and enemy_tank still need four directions and a move animation. \
Then assets.request_publish. \
Requirements:\n{req_text}"
    );
    let specs = registry.specs(&ctx.role);
    let mut repair_note = String::new();
    let mut report = gates(&ctx)?;
    let mut published = false;
    for round in 0..=cfg.max_fixes_per_task {
        let user_prompt = if round == 0 {
            base_prompt.clone()
        } else {
            format!(
                "The asset package is not acceptable yet. Fix the recipes, rebuild, revalidate, then assets.request_publish.\n{repair_note}"
            )
        };
        let task = AgentTask {
            stage: "assets".into(),
            task_id: "asset_package".into(),
            attempt: round + 1,
            max_turns: cfg.max_model_turns_per_task,
            max_fixes: cfg.max_fixes_per_task,
        };
        let outcome = {
            let mut runner = AgentRunner {
                provider,
                registry: &registry,
                ctx: &ctx,
                ledger,
                events,
                run_dir: Some(run.dir()),
            };
            runner.run(&task, &role.prompt, &user_prompt, &specs)
        }?;
        state
            .task_mut("assets", "asset_package")
            .stopped_reason = Some(outcome.stop_reason.clone());
        run.save(state)?;
        // Harness auto-check pass: build every complete dirty recipe the model
        // never built (the `w` hole). Independent events + resource metering.
        let auto_feedback = auto_check_pass(&ctx, ledger, events)?;
        report = gates(&ctx)?;
        fs::write(
            run.reports_dir()?.join("assets.gates.json"),
            serde_json::to_string_pretty(&report)?,
        )?;
        events.append(Event::GateFinished {
            stage: "assets".into(),
            task_id: "asset_package".into(),
            passed: report.passed(),
            failures: report.failed_checks().len() as u32,
            report_digest: report.digest()?,
        })?;
        published = attempt_dir.join("publish-request.json").is_file();
        if report.passed() && published {
            break;
        }
        let mut lines: Vec<String> = issues_from_report(&report)
            .iter()
            .map(|i| format!("- {}: {}", i.check_id, i.message))
            .collect();
        if report.passed() && !published {
            lines.push("- publish: call assets.request_publish".into());
        }
        // Repair prompt carries per-asset latest diagnostics (option a):
        // injected into the next round's user message, never an orphan tool
        // message.
        if !auto_feedback.is_empty() {
            lines.push(format!(
                "harness auto-check results (you did not build these yourself):\n{auto_feedback}"
            ));
        }
        // Coverage-first scheduling: assets that never had a turn are listed
        // before deep repairs, so one stuck asset cannot starve the rest.
        lines.extend(coverage_first_note(&ctx));
        repair_note = lines.join("\n");
        set_stage(state, "assets", StageStatus::Running)?;
        run.save(state)?;
    }
    record_provenance(run)?;
    if !report.passed() || !published {
        set_stage(state, "assets", StageStatus::Failed)?;
        run.save(state)?;
        return Ok(StageStatus::Failed);
    }
    let assets_revision = format!("a{}", state.created_ms);
    // Stamp the real revision id into the manifest before it is frozen.
    let manifest_path = attempt_dir.join("asset-manifest.json");
    if let Ok(text) = fs::read_to_string(&manifest_path) {
        if let Ok(mut value) = serde_json::from_str::<Value>(&text) {
            value["assets_revision"] = Value::String(assets_revision.clone());
            value["gates_passed"] = serde_json::json!(["assets.gates"]);
            fs::write(&manifest_path, serde_json::to_string_pretty(&value)?)?;
        }
    }
    let (_dir, digest) = artifacts.commit_revision("assets", &assets_revision, &attempt_dir)?;
    let mut identity = artifacts.identity()?;
    identity.assets_revision = Some(assets_revision.clone());
    artifacts.save_identity(&identity)?;
    state.assets_revision = Some(assets_revision.clone());
    set_stage(state, "assets", StageStatus::Succeeded)?;
    run.save(state)?;
    events.append(Event::RevisionCommitted {
        kind: "assets".into(),
        revision: assets_revision,
        digest,
    })?;
    record_provenance(run)?;
    Ok(StageStatus::Succeeded)
}

/// Coverage-first scheduling note (P0-C): never-attempted assets come first,
/// blocked assets are named and explicitly deprioritised, so repeated repairs
/// on one asset cannot consume the whole package budget.
fn coverage_first_note(ctx: &ToolContext) -> Vec<String> {
    use crate::pipeline::asset_queue::{AssetState, AssetQueue};
    let reqs = load_requirements(ctx).ok();
    let mut required: Vec<(String, u32)> = reqs
        .as_ref()
        .map(|r| {
            r.requirements
                .iter()
                .filter(|a| a.required)
                .map(|a| (a.asset_id.clone(), a.priority))
                .collect()
        })
        .unwrap_or_default();
    // Lower priority number ships first; stable id order breaks ties.
    required.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    let queue = AssetQueue::load(&ctx.attempt_dir);
    let mut never: Vec<String> = Vec::new();
    let mut blocked: Vec<String> = Vec::new();
    for (id, _) in &required {
        match queue.entry(id) {
            None => never.push(id.clone()),
            Some(e) => match e.state {
                AssetState::MissingSpecs | AssetState::PendingDependencies => {
                    never.push(id.clone())
                }
                AssetState::Blocked => blocked.push(format!(
                    "{id} (blocked after {} attempts: {})",
                    e.repair_attempts,
                    e.last_failure.as_deref().unwrap_or("see report")
                )),
                _ => {}
            },
        }
    }
    let mut out = Vec::new();
    if !never.is_empty() {
        out.push(format!(
            "Do these FIRST (never attempted yet): {}. Write their specs and build them before repairing anything else.",
            never.join(", ")
        ));
    }
    if !blocked.is_empty() {
        out.push(format!(
            "Stop repairing these (harness-blocked, budget exhausted): {}. Move on to the untried assets above.",
            blocked.join("; ")
        ));
    }
    out
}

/// Harness-initiated automatic build of every complete dirty recipe (P0-B).
///
/// Closes the `w` hole: a spec the model wrote but never built still gets a
/// build and a diagnosis. Independent events (`AutoCheck*`) and real resource
/// metering, so auto-builds are auditable and never free.
fn auto_check_pass(
    ctx: &ToolContext,
    ledger: &mut BudgetLedger,
    events: &mut EventLog,
) -> Result<String> {
    let reqs = load_requirements(ctx).ok();
    let required: Vec<String> = reqs
        .as_ref()
        .map(|r| {
            r.requirements
                .iter()
                .filter(|a| a.required)
                .map(|a| a.asset_id.clone())
                .collect()
        })
        .unwrap_or_default();
    let mut queue = crate::pipeline::asset_queue::AssetQueue::load(&ctx.attempt_dir);
    queue.seed(&required);
    // Pick up inputs written after the last probe (e.g. specs authored without
    // going through write_candidate, or a resumed run).
    for id in &required {
        queue.mark_dirty(id, &ctx.attempt_dir);
    }
    let mut sink = |ev: crate::pipeline::asset_queue::AutoCheckEvent| {
        match ev.phase {
            crate::pipeline::asset_queue::AutoCheckPhase::Started => {
                let _ = events.append(Event::AutoCheckStarted {
                    stage: "assets".into(),
                    asset_id: ev.asset_id,
                    recipe: ev.recipe,
                    input_digest: ev.input_digest,
                });
            }
            crate::pipeline::asset_queue::AutoCheckPhase::Finished => {
                let _ = events.append(Event::AutoCheckFinished {
                    stage: "assets".into(),
                    asset_id: ev.asset_id,
                    recipe: ev.recipe,
                    input_digest: ev.input_digest,
                    ok: ev.ok,
                    error_code: ev.error_code,
                    elapsed_ms: ev.elapsed_ms,
                    report_path: ev.report_rel,
                });
            }
        }
    };
    let outcomes = queue.auto_build(&ctx.attempt_dir, &ctx.pxl, &mut sink);
    // Auto-builds consume real resources and must be metered.
    for (_, outcome) in &outcomes {
        ledger.settle_tool(outcome.result.elapsed_ms);
    }
    queue.save(&ctx.attempt_dir)?;
    if outcomes.is_empty() {
        return Ok(String::new());
    }
    Ok(crate::pipeline::asset_queue::render_auto_check_feedback(
        &outcomes,
        &ctx.attempt_dir,
    ))
}

fn record_provenance(run: &RunStore) -> Result<()> {
    let logged = EventLog::read_all(run.dir())?;
    let mut files = Vec::new();
    let mut turns = Vec::new();
    for ev in logged {
        match ev.event {
            Event::CandidateWritten {
                path,
                digest,
                call_id,
                turn,
                prompt_tokens,
                completion_tokens,
                tool,
                ..
            } => files.push(serde_json::json!({
                "path": path,
                "digest": digest,
                "call_id": call_id,
                "turn": turn,
                "tool": tool,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "seq": ev.seq,
            })),
            Event::ModelTurnFinished {
                turn,
                prompt_tokens,
                completion_tokens,
                stop_reason,
                stage,
                ..
            } if stage == "assets" => turns.push(serde_json::json!({
                "turn": turn,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "stop_reason": stop_reason,
            })),
            _ => {}
        }
    }
    let doc = serde_json::json!({
        "schema_version": 1,
        "files": files,
        "turns": turns,
    });
    fs::write(
        run.reports_dir()?.join("assets.provenance.json"),
        serde_json::to_string_pretty(&doc)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn ctx(attempt: &Path, design: &Path) -> ToolContext {
        let full = PermissionSet::default()
            .allow(Permission::ReadProject)
            .allow(Permission::WriteCandidate)
            .allow(Permission::Pxl);
        ToolContext {
            host: full.clone(),
            stage: full.clone(),
            role: full,
            attempt_dir: attempt.to_path_buf(),
            template_roots: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates")],
            genre: "top_down_tank".into(),
            design_dir: Some(design.to_path_buf()),
            pxl: Arc::new(crate::adapters::PxlAdapter::unavailable()),
        }
    }

    fn write_reqs(dir: &Path, ids: &[&str]) {
        fs::create_dir_all(dir).unwrap();
        let requirements: Vec<Value> = ids
            .iter()
            .map(|id| {
                serde_json::json!({
                    "asset_id": id,
                    "kind": "sprite",
                    "required": true,
                    "style_ref": "palette.json",
                    "size_px": [16, 16],
                    "priority": 1,
                    "strategy": "template"
                })
            })
            .collect();
        fs::write(
            dir.join("asset-requirements.json"),
            serde_json::to_string(&serde_json::json!({
                "schema_version": 1,
                "requirements": requirements
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn gates_require_frame_prefix_and_png() {
        let root = std::env::temp_dir().join(format!("pxl_h_assets_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let design = root.join("design");
        let attempt = root.join("attempt");
        write_reqs(&design, &["player_tank", "b"]);
        fs::create_dir_all(attempt.join("out")).unwrap();
        fs::write(
            attempt.join("out/units.sheet.json"),
            r#"{"frames":{"player_tank_move_N_0":{"frame":{"x":0,"y":0,"w":16,"h":16}},"bullet_base":{}}}"#,
        )
        .unwrap();
        fs::write(
            attempt.join("out/units.sheet.png"),
            b"\x89PNG\r\n\x1a\n................padding-bytes!!",
        )
        .unwrap();
        let context = ctx(&attempt, &design);
        let report = gates(&context).unwrap();
        assert!(!report.passed(), "{:?}", report.failed_checks());
        assert!(report
            .failed_checks()
            .iter()
            .any(|c| c.failures.iter().any(|f| f.contains("missing b"))));
        // `b` must not be satisfied by `bullet_base`.
        fs::write(
            attempt.join("out/props.sheet.json"),
            r#"{"frames":{"b_base":{}}}"#,
        )
        .unwrap();
        fs::write(
            attempt.join("out/props.sheet.png"),
            b"\x89PNG\r\n\x1a\n................padding-bytes!!",
        )
        .unwrap();
        let manifest = serde_json::json!({
            "schema_version": 1,
            "design_revision": "d1",
            "assets_revision": "candidate",
            "pxl_version": "0.1.0",
            "pxl_binary_digest": "sha256:x",
            "visual_review": "not_run",
            "entries": [
                {"asset_id":"player_tank","status":"delivered","required":true,"recipe_path":"units.recipe.json","recipe_digest":"sha256:a","palette_path":"palette.json","palette_digest":"sha256:p","seed":1,"outputs":[["out/units.sheet.png","sha256:o"]],"size_px":[16,16]},
                {"asset_id":"b","status":"delivered","required":true,"recipe_path":"props.recipe.json","recipe_digest":"sha256:a","palette_path":"palette.json","palette_digest":"sha256:p","seed":1,"outputs":[["out/props.sheet.png","sha256:o"]],"size_px":[16,16]}
            ]
        });
        fs::write(
            attempt.join("asset-manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        write_original_specs(&attempt, &["player_tank", "b"]);
        // Frames/PNG/manifest alone are not enough: the harness queue must
        // record a successful build for the current inputs.
        let report_before = gates(&context).unwrap();
        assert!(
            report_before
                .failed_checks()
                .iter()
                .any(|c| c.id == "coverage.queue"),
            "queue gate must fail before a build is recorded: {:?}",
            report_before.failed_checks()
        );
        let mut queue = crate::pipeline::asset_queue::AssetQueue::load(&attempt);
        queue.seed(&["player_tank".into(), "b".into()]);
        queue.record_success("player_tank");
        queue.record_success("b");
        queue.save(&attempt).unwrap();
        let report = gates(&context).unwrap();
        assert!(report.passed(), "{:?}", report.failed_checks());
    }

    #[test]
    fn valid_png_with_missing_asset_does_not_blame_png() {
        let root = std::env::temp_dir().join(format!("pxl_h_pngsplit_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let design = root.join("design");
        let attempt = root.join("attempt");
        write_reqs(&design, &["player_tank", "b"]);
        fs::create_dir_all(attempt.join("out")).unwrap();
        // A valid PNG covering player_tank only; `b` is missing entirely.
        fs::write(
            attempt.join("out/units.sheet.json"),
            r#"{"frames":{"player_tank_move_N_0":{"frame":{"x":0,"y":0,"w":16,"h":16}}}}"#,
        )
        .unwrap();
        fs::write(
            attempt.join("out/units.sheet.png"),
            b"\x89PNG\r\n\x1a\n................padding-bytes!!",
        )
        .unwrap();
        let context = ctx(&attempt, &design);
        let report = gates(&context).unwrap();
        let png = report
            .checks
            .iter()
            .find(|c| c.id == "coverage.png")
            .expect("coverage.png present");
        assert!(png.passed, "valid PNG must not fail: {:?}", png.failures);
        let frames = report
            .checks
            .iter()
            .find(|c| c.id == "coverage.frames")
            .unwrap();
        assert!(!frames.passed);
        assert!(frames.failures.iter().any(|f| f.contains("missing b")));
    }

    fn write_original_specs(attempt: &Path, ids: &[&str]) {
        fs::write(
            attempt.join("palette.json"),
            "{\"v\":1,\"id\":\"original-test-palette\",\"colors\":[\"#00000000\",\"#101010\",\"#f4f0e6\"]}",
        )
        .unwrap();
        for id in ids {
            fs::write(
                attempt.join(format!("{id}.shapes.json")),
                format!(r#"{{"w":8,"h":8,"name":"{id}-original","draw":[{{"rect":[1,1,2,2],"c":1}}]}}"#),
            )
            .unwrap();
            fs::write(
                attempt.join(format!("{id}.recipe.json")),
                format!(r#"{{"v":1,"asset":"{id}","gen_version":1,"seed":9,"palette":"palette.json"}}"#),
            )
            .unwrap();
        }
    }

    #[test]
    fn template_json_is_rejected_and_gate_flags_copies() {
        let root = std::env::temp_dir().join(format!("pxl_h_copy_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let design = root.join("design");
        let attempt = root.join("attempt");
        write_reqs(&design, &["player_tank"]);
        fs::create_dir_all(&attempt).unwrap();
        let context = ctx(&attempt, &design);
        let template = fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("templates/top_down_tank/assets/palette.json"),
        )
        .unwrap();
        let reg = registry();
        let rejected = reg
            .dispatch(
                "assets.write_candidate",
                &serde_json::json!({"path": "palette.json", "content": template}),
                &context,
            )
            .unwrap();
        assert!(!rejected.ok, "{}", rejected.summary);
        assert!(rejected.summary.contains("template"));
        assert!(!attempt.join("palette.json").is_file());
        // Reformatted template JSON is still a copy.
        let pretty = serde_json::to_string_pretty(
            &serde_json::from_str::<Value>(&fs::read_to_string(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("templates/top_down_tank/assets/bullet.shapes.json"),
            )
            .unwrap())
            .unwrap(),
        )
        .unwrap();
        fs::write(attempt.join("player_tank.shapes.json"), &pretty).unwrap();
        fs::write(
            attempt.join("player_tank.recipe.json"),
            r#"{"v":1,"asset":"player_tank","seed":3}"#,
        )
        .unwrap();
        fs::write(
            attempt.join("palette.json"),
            "{\"v\":1,\"id\":\"not-the-template\",\"colors\":[\"#00000000\"]}",
        )
        .unwrap();
        let report = gates(&context).unwrap();
        assert!(report
            .checks
            .iter()
            .any(|c| c.id == "provenance.not_template" && !c.passed));
    }

    // --- P0-B scheduling regressions -------------------------------------
    // Carrier assets use multi-frame directional animations (never the
    // single-frame bullet fly, which cannot pass QA before batch D).

    /// A spec written but never built must surface as a queue gap, not as a
    /// silent success. This is the `w` hole: nothing in the model's own tool
    /// results can reveal an error it never triggered.
    #[test]
    fn written_but_never_built_is_caught_by_queue_gate() {
        let root = std::env::temp_dir().join(format!("pxl_h_neverbuilt_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let design = root.join("design");
        let attempt = root.join("attempt");
        write_reqs(&design, &["player_tank", "w"]);
        fs::create_dir_all(&attempt).unwrap();
        let context = ctx(&attempt, &design);
        let reg = registry();
        // Model writes full specs for both assets...
        write_original_specs(&attempt, &["player_tank", "w"]);
        // ...but only builds player_tank; `w` is never built.
        let mut queue = crate::pipeline::asset_queue::AssetQueue::load(&attempt);
        queue.seed(&["player_tank".into(), "w".into()]);
        queue.mark_dirty("player_tank", &attempt);
        queue.mark_dirty("w", &attempt);
        queue.record_success("player_tank");
        queue.save(&attempt).unwrap();
        let report = gates(&context).unwrap();
        let queue_check = report
            .checks
            .iter()
            .find(|c| c.id == "coverage.queue")
            .expect("coverage.queue gate exists");
        assert!(!queue_check.passed, "unbuilt asset must fail the gate");
        assert!(
            queue_check.failures.iter().any(|f| f.contains("w")),
            "{:?}",
            queue_check.failures
        );
        // The status tool tells the model exactly what is outstanding.
        let status = reg
            .dispatch("assets.status", &serde_json::json!({}), &context)
            .unwrap();
        assert!(status.ok);
        let text = serde_json::to_string(&status.data).unwrap();
        assert!(text.contains("pending_build"), "{text}");
    }

    /// Palette rewrite cascades dirt to every asset (debounce: at most one
    /// auto-build per asset per pass, deduped by input fingerprint).
    #[test]
    fn palette_rewrite_does_not_trigger_more_than_one_rebuild_per_asset() {
        let root = std::env::temp_dir().join(format!("pxl_h_debounce_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let design = root.join("design");
        let attempt = root.join("attempt");
        write_reqs(&design, &["player_tank"]);
        fs::create_dir_all(&attempt).unwrap();
        // Full specs exist (multi-frame directional anim carrier).
        write_original_specs(&attempt, &["player_tank"]);
        // Three consecutive palette rewrites in one turn (observed 7x).
        for i in 0..3 {
            fs::write(
                attempt.join("palette.json"),
                format!(r##"{{"v":1,"id":"pal-{i}","colors":["#00000000","#101010"]}}"##),
            )
            .unwrap();
        }
        let mut queue = crate::pipeline::asset_queue::AssetQueue::load(&attempt);
        queue.seed(&["player_tank".into()]);
        queue.mark_all_dirty(&attempt);
        // Only one pending entry regardless of how many palette writes happened.
        let pending = queue
            .entries
            .iter()
            .filter(|e| e.state == crate::pipeline::asset_queue::AssetState::PendingBuild)
            .count();
        assert_eq!(pending, 1, "one asset = one pending build, not one per write");
        // Auto-build is deduped per fingerprint: a second pass with unchanged
        // inputs builds nothing new.
        let mut events = 0u32;
        let first = queue.auto_build(&attempt, &ctx(&attempt, &design).pxl, &mut |_| {
            events += 1;
        });
        assert!(first.is_empty() || events == 0, "pxl unavailable offline => no builds");
        // Recording the fingerprint means a repeat pass is a no-op.
        let fingerprint = queue.entry("player_tank").unwrap().input_fingerprint.clone();
        queue
            .entries
            .iter_mut()
            .find(|e| e.asset_id == "player_tank")
            .unwrap()
            .built_fingerprints
            .push(fingerprint);
        let second = queue.auto_build(&attempt, &ctx(&attempt, &design).pxl, &mut |_| {
            events += 1;
        });
        assert!(second.is_empty(), "same fingerprint must not rebuild");
    }

    /// Coverage-first: the repair note names never-attempted assets first and
    /// tells the model to stop repairing blocked ones.
    #[test]
    fn coverage_first_note_prioritises_untried_assets() {
        let root = std::env::temp_dir().join(format!("pxl_h_covfirst_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let design = root.join("design");
        let attempt = root.join("attempt");
        write_reqs(&design, &["explosion", "w", "hq_base"]);
        fs::create_dir_all(&attempt).unwrap();
        // explosion is blocked; w and hq_base were never attempted.
        let mut queue = crate::pipeline::asset_queue::AssetQueue::load(&attempt);
        queue.seed(&["explosion".into(), "w".into(), "hq_base".into()]);
        let res = crate::adapters::pxl::PxlResult_ {
            ok: false,
            exit_code: Some(5),
            error_code: "E_QA".into(),
            message: "frame diff too large".into(),
            hint: None,
            path: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            elapsed_ms: 1,
        };
        for _ in 0..3 {
            queue.record_failure_and_count("explosion", &res);
        }
        queue.save(&attempt).unwrap();
        let note = coverage_first_note(&ctx(&attempt, &design));
        let joined = note.join("\n");
        assert!(joined.contains("FIRST"), "{joined}");
        assert!(joined.contains("w"), "untried w must be listed: {joined}");
        assert!(joined.contains("hq_base"), "untried hq_base must be listed: {joined}");
        assert!(joined.contains("Stop repairing"), "{joined}");
        assert!(joined.contains("explosion"), "blocked asset must be named: {joined}");
        // The untried assets must not be described as blocked.
        let first_line = &note[0];
        assert!(!first_line.contains("explosion"), "{first_line}");
        let _ = fs::remove_dir_all(&root);
    }

    /// A leftover PNG produced by an older/failing recipe must never be
    /// published: outputs alone are not delivery.
    #[test]
    fn stale_output_from_old_inputs_cannot_be_published() {
        let root = std::env::temp_dir().join(format!("pxl_h_stale_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let design = root.join("design");
        let attempt = root.join("attempt");
        write_reqs(&design, &["player_tank"]);
        fs::create_dir_all(attempt.join("out")).unwrap();
        // A sheet + PNG exist on disk, but no successful build is recorded.
        fs::write(
            attempt.join("out/units.sheet.json"),
            r#"{"frames":{"player_tank_move_N_0":{"frame":{"x":0,"y":0,"w":16,"h":16}}}}"#,
        )
        .unwrap();
        fs::write(
            attempt.join("out/units.sheet.png"),
            b"\x89PNG\r\n\x1a\n................padding-bytes!!",
        )
        .unwrap();
        write_original_specs(&attempt, &["player_tank"]);
        let context = ctx(&attempt, &design);
        let rejected = request_publish(&context, "looks done").unwrap();
        assert!(!rejected.ok, "residue must not publish: {}", rejected.summary);
        assert!(rejected.summary.contains("no successful build"), "{}", rejected.summary);
        assert!(!attempt.join("publish-request.json").is_file());
        // The manifest records the reason instead of claiming delivery.
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(attempt.join("asset-manifest.json")).unwrap())
                .unwrap();
        let warnings = manifest["warnings"].as_array().unwrap();
        assert!(
            warnings.iter().any(|w| w.as_str().unwrap().contains("stale")),
            "{warnings:?}"
        );
        assert!(manifest["entries"].as_array().unwrap().is_empty());

        // After a real successful build for the current inputs, publish works.
        let mut queue = crate::pipeline::asset_queue::AssetQueue::load(&attempt);
        queue.seed(&["player_tank".into()]);
        queue.record_success("player_tank");
        queue.save(&attempt).unwrap();
        let accepted = request_publish(&context, "now complete").unwrap();
        assert!(accepted.ok, "{}", accepted.summary);
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(attempt.join("asset-manifest.json")).unwrap())
                .unwrap();
        let entry = &manifest["entries"][0];
        assert_eq!(entry["asset_id"], "player_tank");
        // Real seed from the recipe (write_original_specs uses seed 9), not 1.
        assert_eq!(entry["seed"], 9, "seed must come from the recipe: {entry}");
        assert!(attempt.join("publish-request.json").is_file());
    }

    /// A blocked asset must not consume the whole package: other assets keep
    /// their state and remain schedulable.
    #[test]
    fn blocked_asset_does_not_starve_others() {
        let root = std::env::temp_dir().join(format!("pxl_h_blocked_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let design = root.join("design");
        let attempt = root.join("attempt");
        write_reqs(&design, &["explosion", "hq_base", "HUD"]);
        fs::create_dir_all(&attempt).unwrap();
        let mut queue = crate::pipeline::asset_queue::AssetQueue::load(&attempt);
        queue.seed(&["explosion".into(), "hq_base".into(), "HUD".into()]);
        // explosion fails E_QA three times -> blocked.
        let res = crate::adapters::pxl::PxlResult_ {
            ok: false,
            exit_code: Some(5),
            error_code: "E_QA".into(),
            message: "1 check(s) failed".into(),
            hint: None,
            path: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            elapsed_ms: 1,
        };
        for _ in 0..3 {
            queue.record_failure_and_count("explosion", &res);
        }
        queue.save(&attempt).unwrap();
        assert_eq!(
            queue.entry("explosion").unwrap().state,
            crate::pipeline::asset_queue::AssetState::Blocked
        );
        // Others are untouched and still eligible for their first attempt.
        assert_eq!(
            queue.entry("hq_base").unwrap().state,
            crate::pipeline::asset_queue::AssetState::MissingSpecs
        );
        assert_eq!(
            queue.entry("HUD").unwrap().state,
            crate::pipeline::asset_queue::AssetState::MissingSpecs
        );
        let report = gates(&ctx(&attempt, &design)).unwrap();
        let queue_check = report.checks.iter().find(|c| c.id == "coverage.queue").unwrap();
        assert!(!queue_check.passed);
        assert!(queue_check.failures.iter().any(|f| f.contains("blocked")));
        // Not-yet-attempted assets are reported as such, not silently omitted.
        assert!(queue_check
            .failures
            .iter()
            .any(|f| f.contains("hq_base") && f.contains("missing_specs")));
    }
}
