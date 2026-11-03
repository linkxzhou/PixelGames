use std::fs;
use std::path::{Path, PathBuf};

use pxl_core::{ErrorCode, PxlError};
use serde_json::{json, Value};

use crate::cmd::args::{BuildArgs, PlanArgs};
use crate::cmd::build::build_cmd;
use crate::error::{CmdError, CmdResult};
use crate::util::{self, Ctx};


mod helpers;

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
        crate::cmd::visual::VisualContext::load(&input, true)?;
    }
    let mut recipes = helpers::discover_recipes(&input)?;
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
        return crate::cmd::baseline::baseline_cmd(ctx, &args, &input, &recipes);
    }

    // P04: resolve the palette families once, before any recipe builds, so a
    // stale family digest fails the whole batch with the family id instead of
    // surfacing per recipe. The registry loads and verifies every palette the
    // intent froze (byte digest + semantic digest).
    if input.join("spec-manifest.json").is_file() {
        if let Some(context) = crate::cmd::visual::VisualContext::load(&input, true)? {
            if !context.families.is_empty() && context.families.len() != context.intent.palette_families.len() {
                return Err(PxlError::semantic(
                    "resolved palette family count does not match the intent",
                )
                .into());
            }
        }
    }

    let audio_spec = helpers::audio_spec_path(&input)?;
    let out_root = PathBuf::from(&args.out);
    let mut entries = Vec::new();
    let mut failures = 0usize;
    for (id, recipe_rel) in &recipes {
        let entry = if args.dry_run {
            helpers::dry_run_one(&input, recipe_rel, id)
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
            let entry = helpers::dry_run_audio(path);
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
        let audio = helpers::build_audio(ctx, &spec, &out_root);
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
            let manifest = helpers::find_manifest(&dest);
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
