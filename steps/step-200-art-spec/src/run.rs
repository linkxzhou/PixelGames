//! `step-200 run`: requirements -> artist loop -> spec-manifest.

use std::path::Path;

use serde_json::{json, Value};

use pxl_agent::agent::role::{artist_for, PromptLang};
use pxl_agent::agent::runner::{AgentRunner, AgentTask};
use pxl_agent::agent::tools::{TodoManager, ToolContext, ToolRegistry};
use pxl_agent::policy::budget::BudgetLedger;
use pxl_agent::policy::{Permission, PermissionSet};
use pxl_agent::pxl_adapter::PxlAdapter;
use pxl_contract::error::{HarnessError, Result};
use pxl_contract::{DigestEntry, GateCheck, GateResult, Gates, Scope, StepEnvelope, StepStatus};
use pxl_store::digest::{digest_bytes, digest_file};
use pxl_store::storage::EventLog;

use crate::provider;
use crate::tools;
use crate::{RunArgs, STEP_ID, VERSION};

pub fn run(args: RunArgs) -> Result<i32> {
    if !args.input.is_dir() {
        return Err(
            HarnessError::usage(format!("{} is not a directory", args.input.display())).with_hint(
                "run --in takes the step-100 output (or any dir with asset-requirements.json)",
            ),
        );
    }
    validate_design_approval(&args.input)?;
    let staging = args.out.join(".staging");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .map_err(|e| HarnessError::io(format!("cannot clear {}: {e}", staging.display())))?;
    }
    std::fs::create_dir_all(&staging)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", staging.display())))?;
    let intent_path = args.input.join("visual-intent.json");
    std::fs::copy(&intent_path, staging.join("visual-intent.snapshot.json")).map_err(|e| {
        HarnessError::io(format!("cannot freeze {}: {e}", intent_path.display()))
    })?;

    // 1. Requirements and the approved, immutable visual intent.
    let requirements = read_requirements(&args.input)?;
    let assets = filter_assets(requirements.clone(), &args)?;
    let intent: pxl_contract::visual::VisualIntent = serde_json::from_str(
        &std::fs::read_to_string(staging.join("visual-intent.snapshot.json"))
            .map_err(|e| HarnessError::io(format!("visual-intent.snapshot.json: {e}")))?,
    )
    .map_err(|e| HarnessError::schema(format!("visual-intent.snapshot.json: {e}")))?;
    let criteria = pxl_contract::knowledge::LessonCriteria {
        profile: Some(intent.profile.id.clone()),
        ..pxl_contract::knowledge::LessonCriteria::bounded()
    };
    let knowledge = pxl_contract::knowledge::lock_from_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").as_path(),
        &criteria,
    )?;
    pxl_contract::knowledge::write_lock(&staging.join("knowledge.lock.json"), &knowledge)?;

    // 2. Provider, adapter, tool context.
    let mut model = provider::build(&args.provider, args.env.as_deref())?;
    let adapter = match PxlAdapter::discover(&provider::adapter_config(args.pxl_bin.as_deref())) {
        Ok(adapter) => adapter,
        // The read-only tools fail loudly if actually invoked; a run that
        // never calls them must not be blocked by a missing binary.
        Err(err) => {
            eprintln!("warning: step-300 discovery failed ({err}); list/schema tools will fail");
            PxlAdapter::unavailable()
        }
    };
    let permissions = PermissionSet::default()
        .allow(Permission::ReadProject)
        .allow(Permission::WriteCandidate)
        .allow(Permission::Pxl)
        .allow(Permission::Todo);
    let todos = std::sync::Arc::new(TodoManager::new());
    let ctx = ToolContext {
        host: permissions.clone(),
        stage: permissions.clone(),
        role: permissions,
        attempt_dir: staging.clone(),
        // Approved read-only roots: here, the prototype library.
        template_roots: args.libraries.clone(),
        genre: String::new(),
        design_dir: Some(args.input.clone()),
        pxl: std::sync::Arc::new(adapter),
        todos: Some(todos),
    };
    let registry: ToolRegistry = tools::registry();

    // 3. Events live beside the step output, never inside it. The directory
    // is keyed by the output directory name so concurrent runs never share
    // an append-only log.
    let out_name = args
        .out
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("art-spec")
        .to_string();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let run_id = format!("run-{}-{nonce}", std::process::id());
    let runs_dir = args
        .out
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("_runs")
        .join(format!("{out_name}-{run_id}"));
    let mut events = EventLog::open(&runs_dir)?;
    let mut ledger = BudgetLedger::load(&runs_dir)?;
    ledger.set_limits(args.token_budget, args.request_budget, args.wall_clock_secs);
    if args.provider != "fake" && !ledger.has_limits() {
        eprintln!(
            "warning: no token/request/wall-clock budget set for a real provider; only --max-turns bounds this run"
        );
    }

    // 4. The artist loop.
    let lang = match args.lang.as_str() {
        "zh" => PromptLang::Zh,
        _ => PromptLang::En,
    };
    let role = artist_for(lang);
    let user_prompt = format!(
        "{}\n{}",
        user_prompt(&assets),
        pxl_contract::knowledge::prompt(&knowledge)
    );
    let allowed = ctx.host.clone();
    let mut attempts = Vec::new();
    let mut loop_result = None;
    let mut manifest = Value::Null;
    let mut lint = Value::Null;
    let mut ok = false;
    let mut previous_failed = None;
    for attempt in 1..=3 {
        let prompt = if attempt == 1 {
            user_prompt.clone()
        } else {
            repair_prompt(&user_prompt, &lint)
        };
        let task = AgentTask {
            stage: STEP_ID.to_string(),
            task_id: "spec".to_string(),
            attempt,
            max_turns: args.max_turns,
            max_fixes: 0,
        };
        let mut runner = AgentRunner {
            provider: model.as_mut(),
            registry: &registry,
            ctx: &ctx,
            ledger: &mut ledger,
            events: &mut events,
            run_dir: Some(&runs_dir),
        };
        let result = runner.run(&task, &role.prompt, &prompt, &registry.specs(&allowed))?;
        eprintln!(
            "artist loop finished: {} turn(s), stop: {}",
            result.turns_used, result.stop_reason
        );
        write_json(&staging.join("requirements.snapshot.json"), &assets)?;
        manifest = assemble_manifest(&staging, &assets, &scope_of(&args))?;
        write_json(&staging.join("spec-manifest.json"), &manifest)?;
        lint = crate::lint::lint_report(&staging, args.pxl_bin.as_deref())?;
        merge_lint_into_manifest(&mut manifest, &lint);
        write_json(&staging.join("spec-manifest.json"), &manifest)?;
        ok = lint["ok"] == true && result.stop_reason != "max_turns";
        let failed = lint["failed"].as_u64().unwrap_or(u64::MAX);
        let no_gain = previous_failed.is_some_and(|previous| failed >= previous);
        previous_failed = Some(failed);
        let evidence = snapshot_attempt(&staging, &runs_dir, attempt)?;
        let change_budget_exceeded = evidence["bytes"].as_u64().unwrap_or(0) > 16 * 1024 * 1024;
        attempts.push(json!({
            "attempt": attempt,
            "stop_reason": result.stop_reason,
            "turns_used": result.turns_used,
            "lint": lint,
            "evidence": evidence,
            "no_gain": no_gain,
            "change_budget_exceeded": change_budget_exceeded,
        }));
        let stop = result.stop_reason.clone();
        loop_result = Some(result);
        if ok || no_gain || change_budget_exceeded || stop == "max_turns" || attempt == 3 {
            break;
        }
    }
    let loop_result = loop_result.expect("the artist always runs once");
    let provenance = json!({
        "schema_version": 2,
        "run_id": run_id,
        "step": STEP_ID,
        "step_version": VERSION,
        "provider": model.describe(),
        "system_prompt_digest": digest_bytes(role.prompt.as_bytes()),
        "user_prompt_digest": digest_bytes(user_prompt.as_bytes()),
        "tool_schema_digest": pxl_store::digest::digest_value(&registry.specs(&allowed))?,
        "capability_digest": pxl_format::capability_digest(),
        "input_digest": pxl_store::digest::digest_value(&assets)?,
        "stop_reason": loop_result.stop_reason,
        "turns_used": loop_result.turns_used,
        "budget": {
            "limits": ledger.limits_json(),
            "tokens": ledger.tokens(),
            "requests": ledger.usage.requests,
            "elapsed_ms": ledger.usage.elapsed_ms,
        },
        "attempts": attempts,
    });
    write_json(&staging.join("model-run.json"), &provenance)?;
    let reports = staging.join("reports");
    std::fs::create_dir_all(&reports)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", reports.display())))?;
    write_json(&reports.join("art-spec.lint.json"), &lint)?;
    let total = assets["requirements"].as_array().map(Vec::len).unwrap_or(0);
    let step = envelope(&args, &staging, &manifest, &lint, ok);
    write_json(&staging.join("step.json"), &serde_json::to_value(&step)?)?;
    let rejected = finalize_output(&staging, &args.out, ok)?;

    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": ok,
            "cmd": "step-200 run",
            "out": args.out.to_string_lossy(),
            "scope": args.scope,
            "total": total,
            "turns_used": loop_result.turns_used,
            "stop_reason": loop_result.stop_reason,
            "rejected": rejected,
        }))?
    );
    Ok(if ok { 0 } else { 11 })
}

fn repair_prompt(original: &str, lint: &Value) -> String {
    format!(
        "{original}\n\nThe previous candidate failed semantic lint. Preserve every asset and field not implicated by a failed check. Change only the named asset/field and never change kind, size, direction policy or frame count to evade the design. Preserve/Change scope: preserve all passing assets; change only diagnostics below. Do not re-roll the package.\n{}",
        serde_json::to_string_pretty(lint).unwrap_or_default()
    )
}

fn finalize_output(staging: &Path, out: &Path, ok: bool) -> Result<Option<String>> {
    let rejected = out.join(".rejected");
    if !ok {
        if rejected.exists() {
            std::fs::remove_dir_all(&rejected).map_err(|e| {
                HarnessError::io(format!("cannot clear {}: {e}", rejected.display()))
            })?;
        }
        std::fs::rename(staging, &rejected)
            .map_err(|e| HarnessError::io(format!("cannot retain rejected specs: {e}")))?;
        return Ok(Some(rejected.to_string_lossy().into_owned()));
    }
    let current_step = out.join("step.json");
    if current_step.exists() {
        std::fs::remove_file(&current_step)
            .map_err(|e| HarnessError::io(format!("cannot retire current step.json: {e}")))?;
    }
    let mut files = Vec::new();
    collect_output_files(staging, staging, &mut files)?;
    files.sort_by(|a, b| (a == "step.json").cmp(&(b == "step.json")).then(a.cmp(b)));
    for relative in files {
        let source = staging.join(&relative);
        let destination = out.join(&relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                HarnessError::io(format!("cannot create {}: {e}", parent.display()))
            })?;
        }
        std::fs::rename(&source, &destination)
            .map_err(|e| HarnessError::io(format!("cannot publish {relative}: {e}")))?;
    }
    let _ = std::fs::remove_dir_all(staging);
    let _ = std::fs::remove_dir_all(rejected);
    Ok(None)
}

fn collect_output_files(root: &Path, dir: &Path, files: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)
        .map_err(|e| HarnessError::io(format!("cannot read {}: {e}", dir.display())))?
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            collect_output_files(root, &path, files)?;
        } else if path.is_file() {
            files.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

fn snapshot_attempt(staging: &Path, runs_dir: &Path, attempt: u32) -> Result<Value> {
    let relative = format!("attempts/attempt-{attempt}");
    let destination = runs_dir.join(&relative);
    if destination.exists() {
        return Err(HarnessError::integrity(format!(
            "immutable attempt directory {} already exists",
            destination.display()
        )));
    }
    std::fs::create_dir_all(&destination)
        .map_err(|e| HarnessError::io(format!("cannot create attempt snapshot: {e}")))?;
    let mut names = Vec::new();
    collect_output_files(staging, staging, &mut names)?;
    names.sort();
    let mut files = Vec::new();
    let mut bytes = 0u64;
    for name in names {
        let source = staging.join(&name);
        let target = destination.join(&name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| HarnessError::io(format!("cannot create attempt parent: {e}")))?;
        }
        std::fs::copy(&source, &target)
            .map_err(|e| HarnessError::io(format!("cannot preserve attempt file {name}: {e}")))?;
        let size = std::fs::metadata(&target)
            .map_err(|e| HarnessError::io(format!("cannot measure attempt file {name}: {e}")))?
            .len();
        bytes = bytes.saturating_add(size);
        files.push((name, digest_file(&target)?));
    }
    let digest = pxl_store::digest::digest_pairs(
        files
            .iter()
            .map(|(path, digest)| (path.as_str(), digest.as_str())),
    );
    write_json(
        &destination.join("attempt-manifest.json"),
        &json!({
            "schema_version": 1,
            "attempt": attempt,
            "files": files.iter().map(|(path, digest)| json!({"path": path, "digest": digest})).collect::<Vec<_>>(),
            "bytes": bytes,
            "digest": digest,
        }),
    )?;
    Ok(json!({"path": relative, "digest": digest, "bytes": bytes, "files": files.len()}))
}

fn validate_design_approval(input: &Path) -> Result<()> {
    let pipeline_path = input.join("pipeline.json");
    let pipeline: Value = serde_json::from_str(
        &std::fs::read_to_string(&pipeline_path)
            .map_err(|e| HarnessError::io(format!("{}: {e}", pipeline_path.display())))?,
    )
    .map_err(|e| HarnessError::schema(format!("pipeline.json: {e}")))?;
    let contract = pipeline["steps"]["step-100-design"]["contract"]
        .as_str()
        .unwrap_or_default();
    if contract != "design/v3" {
        return Err(HarnessError::integrity(format!(
            "design contract '{contract}' does not match required 'design/v3'; regenerate it with the new workflow"
        )));
    }
    let step_path = input.join("step.json");
    let step: Value = serde_json::from_str(
        &std::fs::read_to_string(&step_path)
            .map_err(|e| HarnessError::io(format!("{}: {e}", step_path.display())))?,
    )
    .map_err(|e| HarnessError::schema(format!("step.json: {e}")))?;
    if step["status"] != "ok" || step["verdict"] != "PASS" || step["contract"] != "design/v3" {
        return Err(HarnessError::integrity(
            "step-100 is not an approved passing design/v3 package",
        ));
    }
    let manifest_path = input.join("revision.manifest.json");
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path)
            .map_err(|e| HarnessError::io(format!("{}: {e}", manifest_path.display())))?,
    )
    .map_err(|e| HarnessError::schema(format!("revision.manifest.json: {e}")))?;
    let files = manifest["files"].as_array().cloned().unwrap_or_default();
    if !files
        .iter()
        .any(|file| file["path"] == "visual-intent.json")
    {
        return Err(HarnessError::integrity(
            "approved design/v3 package has no visual-intent.json in its digest closure",
        ));
    }
    let intent_text = std::fs::read_to_string(input.join("visual-intent.json"))
        .map_err(|e| HarnessError::io(format!("visual-intent.json: {e}")))?;
    let intent: pxl_contract::visual::VisualIntent = serde_json::from_str(&intent_text)
        .map_err(|e| HarnessError::schema(format!("visual-intent.json: {e}")))?;
    let visual_errors = intent.validate();
    if !visual_errors.is_empty() {
        return Err(HarnessError::integrity(format!(
            "approved visual intent is invalid: {}",
            visual_errors.join("; ")
        )));
    }
    let mut pairs = Vec::new();
    for file in &files {
        let path = file["path"].as_str().unwrap_or_default();
        let expected = file["digest"].as_str().unwrap_or_default();
        let actual = digest_file(&input.join(path))?;
        if actual != expected {
            return Err(HarnessError::integrity(format!(
                "approved design input '{path}' has changed"
            )));
        }
        pairs.push((path.to_string(), actual));
    }
    let pairs_ref = pairs
        .iter()
        .map(|(path, digest)| (path.as_str(), digest.as_str()))
        .collect::<Vec<_>>();
    let current = pxl_store::digest::digest_pairs(pairs_ref);
    if manifest["manifest_digest"].as_str() != Some(current.as_str()) {
        return Err(HarnessError::integrity(
            "design revision manifest digest is invalid",
        ));
    }
    let approval_path = input.join("approval.json");
    let approval: Value = serde_json::from_str(
        &std::fs::read_to_string(&approval_path)
            .map_err(|_| HarnessError::integrity("design approval is missing"))?,
    )
    .map_err(|e| HarnessError::schema(format!("approval.json: {e}")))?;
    if approval["digest"].as_str() != Some(current.as_str()) {
        return Err(HarnessError::integrity(
            "design approval is missing, expired, or belongs to another revision",
        ));
    }
    Ok(())
}

fn read_requirements(input: &Path) -> Result<Value> {
    let path = input.join("asset-requirements.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| HarnessError::schema(format!("asset-requirements.json: {e}")))?;
    if value["requirements"].as_array().is_none() {
        return Err(HarnessError::schema(
            "asset-requirements.json must carry a requirements array",
        )
        .with_hint("see steps/contracts/design.schema.json"));
    }
    Ok(value)
}

fn filter_assets(mut requirements: Value, args: &RunArgs) -> Result<Value> {
    let mut keep: Vec<String> = requirements["requirements"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e["asset_id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if args.scope == "slice" {
        let pipeline = args.input.join("pipeline.json");
        let text = std::fs::read_to_string(&pipeline).map_err(|e| {
            HarnessError::io(format!("--scope slice needs {} : {e}", pipeline.display()))
        })?;
        let value: Value = serde_json::from_str(&text)
            .map_err(|e| HarnessError::schema(format!("pipeline.json: {e}")))?;
        let slice: Vec<String> = value["slice"]["assets"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        keep.retain(|id| slice.contains(id));
    }
    if let Some(only) = &args.asset {
        keep.retain(|id| id == only);
    }
    requirements["requirements"] = json!(requirements["requirements"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e["asset_id"]
            .as_str()
            .is_some_and(|id| keep.contains(&id.to_string())))
        .collect::<Vec<_>>());
    if requirements["requirements"]
        .as_array()
        .is_some_and(Vec::is_empty)
    {
        return Err(
            HarnessError::semantic("no assets left after --scope/--asset filtering")
                .with_hint("check pipeline.json slice.assets or the --asset id"),
        );
    }
    Ok(requirements)
}

fn scope_of(args: &RunArgs) -> &'static str {
    if args.scope == "slice" {
        "slice"
    } else {
        "all"
    }
}

fn user_prompt(assets: &Value) -> String {
    let list: Vec<String> = assets["requirements"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| {
                    format!(
                        "- {} (kind {}, strategy {}, size {:?}, anims {:?})",
                        e["asset_id"],
                        e["kind"],
                        e["strategy"],
                        e["size_px"],
                        e["anims"]
                            .as_array()
                            .map(|v| v
                                .iter()
                                .filter_map(|a| a["name"].as_str())
                                .collect::<Vec<_>>())
                            .unwrap_or_default()
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    format!(
        "Write the art specs for exactly these assets:\n{}\n\n\
         Rules:\n\
         1. Call assets.read_requirements and assets.read_visual_profile first; the acceptance assertions and frozen visual intent are the contract.\n\
         2. Write one shared palette.json and one visual-plan.json at the spec root. The plan uses visual-plan/v1 (visual-plan/v2 only when the intent is visual-intent/v2 with palette families), the exact intent digest, named source regions, material/ramp/protect bindings, animation phases, channels and preview references.\n\
         3. Author each asset in production order: base spec and palette first, then per-asset recipe; never batch-generate before the review pause. When the intent is visual-intent/v2, prefer one authored exemplar per palette family for review before any remaining assets.\n\
         4. For each asset write <id>/<id>.recipe.json (the anchor). Shapes/parts \
         use their direct source; SVG/ASCII use <id>.sprite.json plus the referenced source.\n\
         5. Effects are ops in the recipe, never pixels in the source spec.\n\
         6. Follow each requirement's direction_policy; never impose a global E/W mirror.\n\
         7. Use assets.list_capabilities and assets.schema before writing a \
         format you are unsure about.\n\
         8. This step never builds. Stop after every spec file is written.",
        list.join("\n")
    )
}

/// Derivation of what "landed on disk": for every requirement, look for the
/// recipe and the kind's spec file, digest both, and record ok/missing.
fn assemble_manifest(out: &Path, requirements: &Value, scope: &str) -> Result<Value> {
    let mut entries = Vec::new();
    let palette_path = out.join("palette.json");
    let palette_digest = palette_path
        .is_file()
        .then(|| digest_file(&palette_path))
        .transpose()?;
    let intent_path = out.join("visual-intent.snapshot.json");
    let plan_path = out.join("visual-plan.json");
    let intent_digest = intent_path
        .is_file()
        .then(|| digest_file(&intent_path))
        .transpose()?;
    let plan_digest = plan_path
        .is_file()
        .then(|| digest_file(&plan_path))
        .transpose()?;
    let knowledge_path = out.join("knowledge.lock.json");
    let knowledge_digest = knowledge_path
        .is_file()
        .then(|| digest_file(&knowledge_path))
        .transpose()?;
    for asset in requirements["requirements"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let id = asset["asset_id"].as_str().unwrap_or_default().to_string();
        let strategy = asset["strategy"].as_str().unwrap_or("shapes");
        let spec_name = match strategy {
            "svg" | "ascii" => format!("{id}.sprite.json"),
            "parts" => format!("{id}.parts.json"),
            "template" => format!("{id}.template.json"),
            _ => format!("{id}.shapes.json"),
        };
        let recipe = format!("{id}/{id}.recipe.json");
        let spec = format!("{id}/{spec_name}");
        let recipe_path = out.join(&recipe);
        let spec_path = out.join(&spec);
        let spec_required = strategy != "tile_gen" && strategy != "external";
        let complete = recipe_path.is_file() && (!spec_required || spec_path.is_file());
        let digests = json!({
            "spec": if spec_path.is_file() { Some(digest_file(&spec_path)?) } else { None },
            "recipe": if recipe_path.is_file() { Some(digest_file(&recipe_path)?) } else { None },
        });
        let mut dependencies = Vec::new();
        if recipe_path.is_file() {
            dependencies.push(json!({"path": recipe, "digest": digest_file(&recipe_path)?}));
        }
        if spec_path.is_file() {
            dependencies.push(json!({"path": spec, "digest": digest_file(&spec_path)?}));
        }
        if let Some(digest) = &palette_digest {
            dependencies.push(json!({"path": "palette.json", "digest": digest}));
        }
        if let Some(digest) = &intent_digest {
            dependencies.push(json!({"path": "visual-intent.snapshot.json", "digest": digest}));
        }
        if let Some(digest) = &plan_digest {
            dependencies.push(json!({"path": "visual-plan.json", "digest": digest}));
        }
        if let Some(digest) = &knowledge_digest {
            dependencies.push(json!({"path": "knowledge.lock.json", "digest": digest}));
        }
        entries.push(json!({
            "id": id,
            "kind": asset["kind"],
            "strategy": strategy,
            "size_px": asset["size_px"],
            "spec": spec,
            "recipe": recipe,
            "anims": asset["anims"],
            "status": if complete { "ok" } else { "missing" },
            "digests": digests,
            "dependencies": dependencies,
            "requirement_digest": pxl_store::digest::digest_value(&asset)?,
            "warnings": [],
        }));
    }
    let mut manifest = json!({
        "schema_version": 3,
        "scope": scope,
        "palette": if palette_path.is_file() { Some("palette.json".to_string()) } else { None },
        "visual_intent": {"path": "visual-intent.snapshot.json", "digest": intent_digest},
        "visual_plan": {"path": "visual-plan.json", "digest": plan_digest},
        "knowledge_lock": {"path": "knowledge.lock.json", "digest": knowledge_digest},
        "capability_digest": pxl_format::capability_digest(),
        "assets": entries,
    });
    if let Some(audio) = audio_entry(out)? {
        manifest["audio"] = audio;
    }
    Ok(manifest)
}

/// The audio block of spec-manifest.json, or `None` when the author wrote no
/// audio. Every file the spec depends on is digested, so editing a song moves
/// the manifest digest exactly like editing a recipe does.
fn audio_entry(out: &Path) -> Result<Option<Value>> {
    let spec_rel = "audio/audio.spec.json";
    let spec_path = out.join(spec_rel);
    if !spec_path.is_file() {
        return Ok(None);
    }
    let mut dependencies = vec![json!({"path": spec_rel, "digest": digest_file(&spec_path)?})];
    let mut status = "ok";
    let mut problem = Value::Null;
    match pxl_audio::validate::validate_spec(&spec_path) {
        Ok(validated) => {
            for song in &validated.songs {
                let rel = format!("audio/{}", song.rel_path);
                if let Ok(digest) = digest_file(&out.join(&rel)) {
                    dependencies.push(json!({"path": rel, "digest": digest}));
                }
            }
        }
        Err(err) => {
            status = "invalid";
            problem = json!(err.message);
        }
    }
    dependencies.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    dependencies.dedup_by(|a, b| a["path"] == b["path"]);
    Ok(Some(json!({
        "contract": pxl_audio::limits::SPEC_CONTRACT,
        "spec": spec_rel,
        "status": status,
        "error": problem,
        "dependencies": dependencies,
    })))
}

fn merge_lint_into_manifest(manifest: &mut Value, lint: &Value) {
    let recipes = lint["dry_run"]["recipes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for asset in manifest["assets"].as_array_mut().into_iter().flatten() {
        let id = asset["id"].as_str().unwrap_or_default();
        let failed = lint["checks"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|check| {
                check["result"] == "fail"
                    && check["id"].as_str().is_some_and(|check_id| {
                        check_id == "step300.semantic_compile"
                            || check_id == "step300.zero_output"
                            || check_id.starts_with(&format!("{id}."))
                    })
            });
        if failed && asset["status"] == "ok" {
            asset["status"] = json!("invalid");
        }
        if let Some(compiled) = recipes
            .iter()
            .find(|entry| entry["recipe"] == asset["recipe"])
        {
            asset["warnings"] = compiled["warnings"].clone();
            if let Some(dependencies) = compiled["dependencies"].as_object() {
                let mut entries = dependencies
                    .iter()
                    .map(|(path, digest)| json!({"path": path, "digest": digest}))
                    .collect::<Vec<_>>();
                if let Some(recipe) = asset["dependencies"]
                    .as_array()
                    .and_then(|items| items.first())
                {
                    entries.push(recipe.clone());
                }
                entries.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
                entries.dedup_by(|a, b| a["path"] == b["path"]);
                asset["dependencies"] = json!(entries);
            }
        }
    }
}

fn envelope(args: &RunArgs, out: &Path, manifest: &Value, lint: &Value, ok: bool) -> StepEnvelope {
    let inputs = [
        Some((
            "asset-requirements.json",
            digest_file(&args.input.join("asset-requirements.json")),
        )),
        args.input.join("style-guide.json").is_file().then(|| {
            (
                "style-guide.json",
                digest_file(&args.input.join("style-guide.json")),
            )
        }),
        Some((
            "approval.json",
            digest_file(&args.input.join("approval.json")),
        )),
    ]
    .into_iter()
    .flatten()
    .filter_map(|(p, d)| {
        d.ok().map(|d| DigestEntry {
            path: p.to_string(),
            digest: d,
        })
    })
    .collect();
    let required = manifest["assets"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|asset| GateCheck {
            id: format!("{}/recipe+spec", asset["id"].as_str().unwrap_or_default()),
            result: if asset["status"] == "ok" {
                GateResult::Pass
            } else {
                GateResult::Fail
            },
            detail: None,
        })
        .collect();
    let mut quality_checks = lint["checks"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|check| GateCheck {
            id: check["id"].as_str().unwrap_or_default().to_string(),
            result: if check["result"] == "pass" {
                GateResult::Pass
            } else {
                GateResult::Fail
            },
            detail: check["detail"].as_str().map(str::to_string),
        })
        .collect::<Vec<_>>();
    if lint["ok"] == true && !ok {
        quality_checks.push(GateCheck {
            id: "model.stop_reason".into(),
            result: GateResult::Fail,
            detail: Some("model did not finish normally".into()),
        });
    }
    let gates = Gates {
        required_artifacts: required,
        quality_checks,
        not_assessed: Vec::new(),
    };
    StepEnvelope {
        schema_version: 1,
        step: STEP_ID.to_string(),
        step_version: VERSION.to_string(),
        contract: crate::CONTRACT_ID.to_string(),
        status: if ok {
            StepStatus::Ok
        } else {
            StepStatus::Blocked
        },
        scope: if args.scope == "slice" {
            Scope::Slice
        } else {
            Scope::All
        },
        inputs,
        outputs: vec![DigestEntry {
            path: "spec-manifest.json".to_string(),
            digest: digest_file(&out.join("spec-manifest.json"))
                .unwrap_or_else(|_| "sha256:unknown".to_string()),
        }],
        verdict: gates.verdict(),
        gates,
        error: None,
    }
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
    text.push('\n');
    std::fs::write(path, text)
        .map_err(|e| HarnessError::io(format!("cannot write {}: {e}", path.display())))
}
