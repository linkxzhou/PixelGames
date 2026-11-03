//! `step-100 run` / `gate` / `approve` / `status`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use pxl_agent::agent::role::{designer_for, PromptLang};
use pxl_agent::agent::runner::{AgentRunner, AgentTask};
use pxl_agent::agent::tools::{TodoManager, ToolContext};
use pxl_agent::policy::budget::BudgetLedger;
use pxl_agent::policy::{Permission, PermissionSet};
use pxl_agent::pxl_adapter::PxlAdapter;
use pxl_contract::error::{ErrorCode, HarnessError, Result};
use pxl_contract::{
    DigestEntry, GateCheck, GateResult, Gates, Scope, StepEnvelope, StepStatus, Verdict,
};
use pxl_store::digest::{digest_bytes, digest_file, digest_pairs};
use pxl_store::storage::EventLog;

use crate::gates::{gates, DESIGN_FILES};
use crate::init::{canonical_json, write_envelope};
use crate::review::render_review;
use crate::tools;
use crate::{ApproveArgs, GateArgs, RunArgs, StatusArgs, STEP_ID, VERSION};

pub fn run(args: RunArgs) -> Result<i32> {
    let input = &args.input;
    if !input.is_dir() {
        return Err(
            HarnessError::usage(format!("{} is not a directory", input.display()))
                .with_hint("run --in takes the step-100 init output"),
        );
    }
    let out = args.out.clone().unwrap_or_else(|| input.clone());
    let staging = out.join(".staging");
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|e| HarnessError::io(format!("cannot clear {}: {e}", staging.display())))?;
    }
    fs::create_dir_all(&staging)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", staging.display())))?;
    for name in [
        "brief.snapshot.json",
        "brief.json",
        "template.json",
        "pipeline.json",
    ] {
        let source = input.join(name);
        if source.is_file() {
            fs::copy(&source, staging.join(name))
                .map_err(|e| HarnessError::io(format!("cannot stage {name}: {e}")))?;
        }
    }
    // The brief snapshot the tools read: prefer the init snapshot, fall back
    // to a raw brief.json.
    let brief_path = ["brief.snapshot.json", "brief.json"]
        .iter()
        .map(|n| staging.join(n))
        .find(|p| p.is_file())
        .ok_or_else(|| {
            HarnessError::not_found("no brief.snapshot.json in the input directory")
                .with_hint("run `step-100 init` first")
        })?;
    let brief_text = fs::read_to_string(&brief_path)
        .map_err(|e| HarnessError::io(format!("{}: {e}", brief_path.display())))?;
    let brief: Value = serde_json::from_str(&brief_text)
        .map_err(|e| HarnessError::schema(format!("{}: {e}", brief_path.display())))?;
    let genre = brief["genre"]
        .as_str()
        .unwrap_or("top_down_tank")
        .to_string();

    // The designer loop (step-100 never calls pxl; the adapter placeholder
    // exists only because ToolContext requires one).
    let mut model = build_provider(&args.provider, args.env.as_deref())?;
    let permissions = PermissionSet::default()
        .allow(Permission::ReadProject)
        .allow(Permission::WriteCandidate)
        .allow(Permission::Todo);
    let todos = std::sync::Arc::new(TodoManager::new());
    let ctx = ToolContext {
        host: permissions.clone(),
        stage: permissions.clone(),
        role: permissions,
        attempt_dir: staging.clone(),
        template_roots: vec![crate::init::templates_root()],
        genre,
        design_dir: None,
        pxl: std::sync::Arc::new(PxlAdapter::unavailable()),
        todos: Some(todos),
    };
    let registry = tools::registry();

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let run_id = format!("run-{}-{nonce}", std::process::id());
    let runs_dir = out
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("_runs")
        .join(format!(
            "{}-{run_id}",
            out.file_name().and_then(|s| s.to_str()).unwrap_or("design")
        ));
    let mut events = EventLog::open(&runs_dir)?;
    let mut ledger = BudgetLedger::load(&runs_dir)?;
    ledger.set_limits(args.token_budget, args.request_budget, args.wall_clock_secs);
    if args.provider != "fake" && !ledger.has_limits() {
        eprintln!(
            "warning: no token/request/wall-clock budget set for a real provider; only --max-turns bounds this run"
        );
    }

    let lang = match args.lang.as_str() {
        "zh" => PromptLang::Zh,
        _ => PromptLang::En,
    };
    let role = designer_for(lang);
    let knowledge = pxl_contract::knowledge::lock_from_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").as_path(),
        &pxl_contract::knowledge::LessonCriteria::bounded(),
    )?;
    pxl_contract::knowledge::write_lock(&staging.join("knowledge.lock.json"), &knowledge)?;
    let user_prompt = format!(
        "{}\n{}",
        user_prompt(&brief_text),
        pxl_contract::knowledge::prompt(&knowledge)
    );
    let allowed = ctx.host.clone();
    let mut attempts = Vec::new();
    let mut final_code = 12;
    let mut final_result = None;
    for attempt in 1..=3 {
        let prompt = if attempt == 1 {
            user_prompt.clone()
        } else {
            design_repair_prompt(&user_prompt, &staging)
        };
        let task = AgentTask {
            stage: STEP_ID.to_string(),
            task_id: "design".to_string(),
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
            "designer loop finished: {} turn(s), stop: {}",
            result.turns_used, result.stop_reason
        );
        if args.until == "draft" {
            let envelope = envelope_for(&staging, StepStatus::Draft, Verdict::NotAssessed, "draft");
            write_envelope(&staging, &envelope)?;
            final_code = 0;
        } else {
            final_code = finish_package(&staging, &brief_path)?;
        }
        attempts.push(json!({
            "attempt": attempt,
            "stop_reason": result.stop_reason,
            "turns_used": result.turns_used,
            "exit_code": final_code,
        }));
        let stop = result.stop_reason.clone();
        final_result = Some(result);
        if final_code == 10 || args.until == "draft" || stop == "max_turns" || attempt == 3 {
            break;
        }
    }
    let loop_result = final_result.expect("the designer always runs once");
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
        "input_digest": digest_file(&brief_path)?,
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
    fs::write(staging.join("model-run.json"), canonical_json(&provenance))
        .map_err(|e| HarnessError::io(format!("cannot write model-run.json: {e}")))?;
    let rejected = finalize_design_output(&staging, &out, final_code == 10)?;
    let status = if final_code == 10 {
        "waiting_approval"
    } else if args.until == "draft" {
        "draft"
    } else {
        "gate_failed"
    };
    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": final_code == 0 || final_code == 10,
            "cmd": "step-100 run",
            "status": status,
            "out": out.to_string_lossy(),
            "rejected": rejected,
        }))?
    );
    Ok(final_code)
}

fn design_repair_prompt(original: &str, staging: &Path) -> String {
    let diagnostics = fs::read_to_string(staging.join("reports/design.gates.json"))
        .unwrap_or_else(|_| "{\"error\":\"gate report unavailable\"}".into());
    format!(
        "{original}\n\nThe previous design failed its gate. Preserve every passing file and requirement. Change only fields implicated by the diagnostics; do not rewrite the package or weaken acceptance to evade a failure. Preserve/Change scope and diagnostics:\n{diagnostics}"
    )
}

fn finalize_design_output(staging: &Path, out: &Path, publish: bool) -> Result<Option<String>> {
    let rejected = out.join(".rejected");
    if !publish {
        if rejected.exists() {
            fs::remove_dir_all(&rejected).map_err(|e| {
                HarnessError::io(format!("cannot clear {}: {e}", rejected.display()))
            })?;
        }
        fs::rename(staging, &rejected)
            .map_err(|e| HarnessError::io(format!("cannot retain rejected design: {e}")))?;
        return Ok(Some(rejected.to_string_lossy().into_owned()));
    }
    let current_step = out.join("step.json");
    if current_step.exists() {
        fs::remove_file(&current_step)
            .map_err(|e| HarnessError::io(format!("cannot retire current step.json: {e}")))?;
    }
    let mut files = Vec::new();
    collect_design_files(staging, staging, &mut files)?;
    files.sort_by(|a, b| (a == "step.json").cmp(&(b == "step.json")).then(a.cmp(b)));
    for relative in files {
        let source = staging.join(&relative);
        let destination = out.join(&relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                HarnessError::io(format!("cannot create {}: {e}", parent.display()))
            })?;
        }
        fs::rename(&source, &destination)
            .map_err(|e| HarnessError::io(format!("cannot publish {relative}: {e}")))?;
    }
    let _ = fs::remove_dir_all(staging);
    let _ = fs::remove_dir_all(rejected);
    Ok(None)
}

fn collect_design_files(root: &Path, dir: &Path, files: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(dir)
        .map_err(|e| HarnessError::io(format!("cannot read {}: {e}", dir.display())))?
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            collect_design_files(root, &path, files)?;
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

/// Shared tail of `run`: gates -> review -> revision manifest -> envelope.
pub(crate) fn finish_package(out: &Path, brief_path: &Path) -> Result<i32> {
    let report = gates(out)?;
    let reports = out.join("reports");
    fs::create_dir_all(&reports)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", reports.display())))?;
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|e| HarnessError::io(format!("design.gates.json: {e}")))?;
    fs::write(
        reports.join("design.gates.json"),
        format!("{report_text}\n"),
    )
    .map_err(|e| HarnessError::io(format!("cannot write design.gates.json: {e}")))?;
    if !report.passed() {
        let envelope = envelope_for(out, StepStatus::GateFailed, Verdict::Fail, "gates");
        write_envelope(out, &envelope)?;
        let failed = report
            .checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| c.id.clone())
            .collect::<Vec<_>>();
        let err = HarnessError::new(
            ErrorCode::Gate,
            format!("design gates failed: {} check(s)", failed.len()),
        )
        .with_hint(format!("failed checks: {}", failed.join(", ")));
        eprintln!("error [{}]: {}", err.code.as_str(), err.message);
        if let Some(hint) = &err.hint {
            eprintln!("hint: {hint}");
        }
        return Ok(12);
    }
    // The Markdown review view is derived, never a source of truth.
    render_review(out, "step-100")?;

    // Immutable revision manifest: sorted (path, digest) pairs over the
    // design files that exist. Approval binds to this aggregate.
    let manifest = revision_manifest(out, brief_path)?;
    fs::write(
        out.join("revision.manifest.json"),
        serde_json::to_string_pretty(&manifest)
            .map_err(|e| HarnessError::io(format!("revision.manifest.json: {e}")))?,
    )
    .map_err(|e| HarnessError::io(format!("cannot write revision.manifest.json: {e}")))?;

    let envelope = envelope_for(out, StepStatus::WaitingApproval, Verdict::Pass, "gates");
    write_envelope(out, &envelope)?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": true, "cmd": "step-100 run", "status": "waiting_approval",
            "out": out.to_string_lossy(),
            "manifest_digest": manifest["manifest_digest"],
        }))?
    );
    Ok(10)
}

/// `gate`: re-run gates without a model. When an approval exists, its digest
/// is verified against a freshly recomputed manifest — tampering is 15.
pub fn gate(args: GateArgs) -> Result<i32> {
    let dir = &args.input;
    if !dir.is_dir() {
        return Err(HarnessError::usage(format!(
            "{} is not a directory",
            dir.display()
        )));
    }
    if dir.join("approval.json").is_file() {
        let current = recompute_manifest_digest(dir)?;
        let approval: Value = serde_json::from_str(
            &fs::read_to_string(dir.join("approval.json"))
                .map_err(|e| HarnessError::io(format!("approval.json: {e}")))?,
        )
        .map_err(|e| HarnessError::schema(format!("approval.json: {e}")))?;
        let approved = approval["digest"].as_str().unwrap_or_default();
        if approved != current {
            let err =
                HarnessError::integrity("the approved digest no longer matches the design package")
                    .with_hint(format!("approved {approved}, current {current}"));
            eprintln!("error [{}]: {}", err.code.as_str(), err.message);
            if let Some(hint) = &err.hint {
                eprintln!("hint: {hint}");
            }
            return Ok(15);
        }
    }
    let report = gates(dir)?;
    let reports = dir.join("reports");
    fs::create_dir_all(&reports)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", reports.display())))?;
    fs::write(
        reports.join("design.gates.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&report)
                .map_err(|e| HarnessError::io(format!("design.gates.json: {e}")))?
        ),
    )
    .map_err(|e| HarnessError::io(format!("cannot write design.gates.json: {e}")))?;
    if report.passed() {
        Ok(0)
    } else {
        eprintln!("error [E_GATE]: design gates failed");
        Ok(12)
    }
}

/// `approve`: bind the current revision manifest digest.
pub fn approve(args: ApproveArgs) -> Result<i32> {
    let dir = &args.input;
    let manifest_path = dir.join("revision.manifest.json");
    if !manifest_path.is_file() {
        let err = HarnessError::not_found(
            "no revision.manifest.json: run `step-100 run` to a passing package first",
        );
        eprintln!("error [{}]: {}", err.code.as_str(), err.message);
        return Ok(4);
    }
    let current = recompute_manifest_digest(dir)?;
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
    let approval = json!({
        "schema_version": 1,
        "stage": "design",
        "digest": current,
        "approved_by": "user",
    });
    fs::write(dir.join("approval.json"), canonical_json(&approval))
        .map_err(|e| HarnessError::io(format!("cannot write approval.json: {e}")))?;
    let envelope = envelope_for(dir, StepStatus::Ok, Verdict::Pass, "approved");
    write_envelope(dir, &envelope)?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": true, "cmd": "step-100 approve", "digest": current,
        }))?
    );
    Ok(0)
}

pub fn status(args: StatusArgs) -> Result<i32> {
    let path = args.input.join("step.json");
    let text = fs::read_to_string(&path).map_err(|e| {
        HarnessError::io(format!("{}: {e}", path.display()))
            .with_hint("run `step-100 init` or `step-100 run` first")
    })?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| HarnessError::schema(format!("step.json: {e}")))?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(0)
}

// ---- helpers ----

fn build_provider(
    kind: &str,
    env_path: Option<&Path>,
) -> Result<Box<dyn pxl_agent::agent::provider::ModelProvider>> {
    match kind {
        "fake" => {
            const FAKE_TURNS_ENV: &str = "STEP_100_FAKE_TURNS";
            if let Ok(script) = std::env::var(FAKE_TURNS_ENV) {
                if !script.is_empty() {
                    let text = fs::read_to_string(&script).map_err(|e| {
                        HarnessError::usage(format!("{FAKE_TURNS_ENV}: cannot read {script}: {e}"))
                    })?;
                    return Ok(Box::new(
                        pxl_agent::agent::provider::FakeProvider::from_scripted_json(&text)?,
                    ));
                }
            }
            Err(
                HarnessError::usage("the fake provider needs scripted turns").with_hint(format!(
                    "set {FAKE_TURNS_ENV} to a JSON file of tool-call turns"
                )),
            )
        }
        "openai" => {
            let path = env_path
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".env"));
            let mut cfg = pxl_agent::config::Config::default();
            cfg.provider = pxl_agent::config::ProviderKind::Openai;
            Ok(Box::new(
                pxl_agent::agent::provider::OpenAiProvider::from_dotenv(&cfg, &path)?,
            ))
        }
        other => Err(HarnessError::usage(format!("unknown provider '{other}'"))
            .with_hint("use fake or openai")),
    }
}

fn user_prompt(brief_text: &str) -> String {
    format!(
        "The brief (verbatim, never summarise it in your outputs):\n\n{brief_text}\n\n\
         Write the complete design package:\n\
         1. game-design.json — entities, actions, rules, win/lose conditions.\n\
         2. style-guide.json — resolution, tile and sprite sizes, palette, directions.\n\
         3. visual-intent.json — visual-intent/v1 (use visual-intent/v2 with modern and palette_families only when the brief explicitly asks for the modern-pixel profile) with a real profile digest, native grid, palette, roles, materials, asset policies, animation, scenes and review risks. Read design.read_visual_profile before choosing a profile.\n\
         4. levels/<id>.json — at least one level: grid, tile semantics, spawn, exit, placements.\n\
         5. asset-requirements.json — every entity and tile the levels reference.\n\
         6. acceptance-plan.json — one item per requirement.\n\
         Use schema_version 3. Every asset needs executable machine/human acceptance items; \
         visual, anchor and direction_policy are production contracts, not free-text hints.\n\
         Then call design.validate; repair until it passes, and finish with \
         design.request_review."
    )
}

/// The files that make up a revision: everything in DESIGN_FILES that exists,
/// plus the review view, with sorted digests.
pub(crate) fn revision_files(out: &Path) -> Result<Vec<(String, String)>> {
    let mut files = Vec::new();
    let push = |rel: &str, files: &mut Vec<(String, String)>| -> Result<()> {
        let path = out.join(rel);
        if path.is_file() {
            files.push((rel.to_string(), digest_file(&path)?));
        }
        Ok(())
    };
    for name in DESIGN_FILES {
        if *name == "levels" {
            let levels = out.join("levels");
            if levels.is_dir() {
                let mut entries: Vec<PathBuf> = fs::read_dir(&levels)
                    .map_err(|e| HarnessError::io(format!("levels: {e}")))?
                    .flatten()
                    .map(|e| e.path())
                    .collect();
                entries.sort();
                for entry in entries {
                    if entry.is_file() {
                        let rel = format!(
                            "levels/{}",
                            entry.file_name().unwrap_or_default().to_string_lossy()
                        );
                        files.push((rel, digest_file(&entry)?));
                    }
                }
            }
        } else {
            push(name, &mut files)?;
        }
    }
    push("brief.snapshot.json", &mut files)?;
    push("knowledge.lock.json", &mut files)?;
    push("reports/design-review.md", &mut files)?;
    files.sort();
    Ok(files)
}

fn revision_manifest(out: &Path, _brief_path: &Path) -> Result<Value> {
    let files = revision_files(out)?;
    let pairs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, d)| (p.as_str(), d.as_str()))
        .collect();
    Ok(json!({
        "schema_version": 1,
        "stage": "design",
        "files": files.iter()
            .map(|(p, d)| json!({ "path": p, "digest": d }))
            .collect::<Vec<_>>(),
        "manifest_digest": digest_pairs(pairs),
    }))
}

/// Recompute the aggregate over the current files; any edit moves it.
fn recompute_manifest_digest(dir: &Path) -> Result<String> {
    let files = revision_files(dir)?;
    let pairs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, d)| (p.as_str(), d.as_str()))
        .collect();
    Ok(digest_pairs(pairs))
}

fn envelope_for(out: &Path, status: StepStatus, verdict: Verdict, phase: &str) -> StepEnvelope {
    let inputs = ["brief.snapshot.json", "brief.json"]
        .iter()
        .map(|n| out.join(n))
        .find(|p| p.is_file())
        .and_then(|p| digest_file(&p).ok())
        .map(|d| DigestEntry {
            path: "brief.snapshot.json".to_string(),
            digest: d,
        })
        .into_iter()
        .collect();
    let outputs = [
        "game-design.json",
        "asset-requirements.json",
        "visual-intent.json",
    ]
        .iter()
        .filter_map(|n| {
            digest_file(&out.join(n)).ok().map(|d| DigestEntry {
                path: n.to_string(),
                digest: d,
            })
        })
        .collect();
    let required = DESIGN_FILES
        .iter()
        .filter(|f| {
            **f != "brief.json"
                && **f != "tuning.json"
                && **f != "levels"
                && **f != crate::contracts::AUDIO_REQUIREMENTS_FILE
        })
        .map(|f| GateCheck {
            id: f.to_string(),
            result: if out.join(f).is_file() || (matches!(verdict, Verdict::Pass) && *f == "levels")
            {
                GateResult::Pass
            } else {
                GateResult::NotAssessed
            },
            detail: None,
        })
        .collect();
    let not_assessed = if matches!(verdict, Verdict::Pass) {
        Vec::new()
    } else {
        vec![phase.to_string()]
    };
    StepEnvelope {
        schema_version: 1,
        step: STEP_ID.to_string(),
        step_version: VERSION.to_string(),
        contract: crate::CONTRACT_ID.to_string(),
        status,
        scope: Scope::All,
        inputs,
        outputs,
        verdict,
        gates: Gates {
            required_artifacts: required,
            quality_checks: Vec::new(),
            not_assessed,
        },
        error: None,
    }
}

// digest_bytes is re-exported for the tools module's summary payloads; keep
// the import meaningful even when the run tail short-circuits.
#[allow(dead_code)]
fn _digest_marker() -> String {
    digest_bytes(b"step-100")
}
