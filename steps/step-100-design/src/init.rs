//! `step-100 init`: brief.json -> brief.snapshot + template + pipeline lock.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use pxl_contract::error::{HarnessError, Result};
use pxl_contract::{DigestEntry, GateCheck, GateResult, Gates, Scope, StepEnvelope, StepStatus};
use pxl_store::digest::{digest_bytes, digest_file};

use crate::{InitArgs, STEP_ID, VERSION};

/// Built-in genre templates shipped with this crate.
pub fn templates_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")
}

/// `^[a-z][a-z0-9-]{1,31}$` — same as brief.schema.json / pipeline.schema.json.
fn valid_game_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    let rest: Vec<char> = chars.collect();
    (1..=31).contains(&rest.len())
        && rest
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
}

pub fn init(args: InitArgs) -> Result<i32> {
    let text = fs::read_to_string(&args.brief)
        .map_err(|e| HarnessError::io(format!("{}: {e}", args.brief.display())))?;
    let brief: Value = serde_json::from_str(&text)
        .map_err(|e| HarnessError::schema(format!("brief.json: {e}")))?;

    // Required identity fields.
    let game_id = brief["game_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            HarnessError::schema("brief.json must carry a non-empty game_id")
                .with_hint("see steps/contracts/brief.schema.json")
        })?;
    if !valid_game_id(game_id) {
        return Err(HarnessError::schema(format!(
            "game_id '{game_id}' must match ^[a-z_][a-z0-9-]{{1,31}}$"
        )));
    }
    let genre = brief["genre"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| HarnessError::schema("brief.json must carry a genre"))?;

    // The genre must name a bundled template.
    let template_path = templates_root().join(genre).join("template.json");
    if !template_path.is_file() {
        return Err(HarnessError::semantic(format!("unknown genre '{genre}'"))
            .with_hint(format!("expected {}", template_path.display())));
    }

    // Idempotence: a second init into a live workspace needs --overwrite.
    // Exit 6 per the pipeline table (plan §5.4), not the harness table.
    if args.out.join("step.json").is_file() && !args.overwrite {
        let err = HarnessError::exists(format!(
            "{} already carries a step.json",
            args.out.display()
        ))
        .with_hint("pass --overwrite to re-init");
        eprintln!("error [{}]: {}", err.code.as_str(), err.message);
        if let Some(hint) = &err.hint {
            eprintln!("hint: {hint}");
        }
        return Ok(6);
    }
    fs::create_dir_all(&args.out)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", args.out.display())))?;

    // 1. Canonical brief snapshot (the notes stay verbatim, never summarised).
    let snapshot = canonical_json(&brief);
    fs::write(args.out.join("brief.snapshot.json"), &snapshot)
        .map_err(|e| HarnessError::io(format!("brief.snapshot.json: {e}")))?;

    // 2. The expanded genre template.
    let template_text = fs::read_to_string(&template_path)
        .map_err(|e| HarnessError::io(format!("{}: {e}", template_path.display())))?;
    fs::write(args.out.join("template.json"), &template_text)
        .map_err(|e| HarnessError::io(format!("template.json: {e}")))?;

    // 3. The pipeline lock: contract ids and step versions every later step
    //    reads. The slice passes through when the brief declares one.
    let steps = json!({
        "step-100-design": { "version": VERSION, "contract": "design/v3" },
        "step-200-art-spec": { "version": "0.1.0", "contract": "art-spec/v3" },
        "step-300-asset-build": { "version": "0.1.0", "contract": "asset-build/v4" },
        "step-400-package": { "version": "0.1.0", "contract": "asset-package/v4" },
        "step-500-engine": { "version": "0.1.0", "contract": "engine/v4" },
    });
    let mut pipeline = json!({
        "schema_version": 2,
        "contract_version": "pipeline/v2",
        "game_id": game_id,
        "steps": steps,
        "runtime": { "name": game_id, "engine": "macroquad" },
    });
    if let Some(slice) = brief.get("slice") {
        pipeline["slice"] = slice.clone();
    }
    let pipeline_text = canonical_json(&pipeline);
    fs::write(args.out.join("pipeline.json"), &pipeline_text)
        .map_err(|e| HarnessError::io(format!("pipeline.json: {e}")))?;

    // 4. step.json envelope.
    let outputs = [
        ("brief.snapshot.json", digest_bytes(snapshot.as_bytes())),
        ("template.json", digest_bytes(template_text.as_bytes())),
        ("pipeline.json", digest_bytes(pipeline_text.as_bytes())),
    ];
    let envelope = StepEnvelope {
        schema_version: 1,
        step: STEP_ID.to_string(),
        step_version: VERSION.to_string(),
        contract: "pipeline/v2".to_string(),
        status: StepStatus::Ok,
        scope: Scope::All,
        inputs: vec![DigestEntry {
            path: "brief.json".to_string(),
            digest: digest_file(&args.brief)?,
        }],
        outputs: outputs
            .iter()
            .map(|(p, d)| DigestEntry {
                path: p.to_string(),
                digest: d.clone(),
            })
            .collect(),
        verdict: gates_of(&outputs).verdict(),
        gates: gates_of(&outputs),
        error: None,
    };
    write_envelope(&args.out, &envelope)?;

    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": true,
            "cmd": "step-100 init",
            "out": args.out.to_string_lossy(),
            "game_id": game_id,
            "genre": genre,
        }))?
    );
    Ok(0)
}

fn gates_of(outputs: &[(&str, String)]) -> Gates {
    Gates {
        required_artifacts: outputs
            .iter()
            .map(|(p, _)| GateCheck {
                id: p.to_string(),
                result: GateResult::Pass,
                detail: None,
            })
            .collect(),
        quality_checks: Vec::new(),
        not_assessed: Vec::new(),
    }
}

pub(crate) fn canonical_json(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.push('\n');
    text
}

pub(crate) fn write_envelope(dir: &Path, envelope: &StepEnvelope) -> Result<()> {
    let text = serde_json::to_string_pretty(envelope)
        .map_err(|e| HarnessError::io(format!("step.json: {e}")))?;
    fs::write(dir.join("step.json"), format!("{text}\n"))
        .map_err(|e| HarnessError::io(format!("cannot write step.json: {e}")))
}
