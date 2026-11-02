//! Zero-model semantic validation for authored specs.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use pxl_agent::pxl_adapter::PxlAdapter;
use pxl_contract::error::{HarnessError, Result};

use crate::{CapabilitiesArgs, LintArgs, StatusArgs};

pub(crate) fn lint_report(input: &Path, pxl_bin: Option<&Path>) -> Result<Value> {
    let manifest_path = input.join("spec-manifest.json");
    let text = std::fs::read_to_string(&manifest_path).map_err(|e| {
        HarnessError::io(format!("{}: {e}", manifest_path.display()))
            .with_hint("run `step-200 run` first")
    })?;
    let manifest: Value = serde_json::from_str(&text)
        .map_err(|e| HarnessError::schema(format!("spec-manifest.json: {e}")))?;
    let requirements_path = input.join("requirements.snapshot.json");
    let requirements: Value = serde_json::from_str(
        &std::fs::read_to_string(&requirements_path)
            .map_err(|e| HarnessError::io(format!("{}: {e}", requirements_path.display())))?,
    )
    .map_err(|e| HarnessError::schema(format!("requirements.snapshot.json: {e}")))?;

    let mut checks = Vec::new();
    visual_plan_checks(input, &mut checks);
    for asset in manifest["assets"].as_array().cloned().unwrap_or_default() {
        let id = asset["id"].as_str().unwrap_or_default();
        let requirement = requirements["requirements"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["asset_id"] == id));
        let requirement_ok = requirement.is_some_and(|requirement| {
            pxl_store::digest::digest_value(requirement).ok().as_deref()
                == asset["requirement_digest"].as_str()
        });
        push_check(
            &mut checks,
            format!("{id}.requirement"),
            requirement_ok,
            "requirement digest matches the frozen snapshot",
        );
        for dependency in asset["dependencies"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            let path = dependency["path"].as_str().unwrap_or_default();
            let expected = dependency["digest"].as_str().unwrap_or_default();
            let ok = !path.is_empty()
                && pxl_store::digest::digest_file(&input.join(path))
                    .map(|actual| actual == expected)
                    .unwrap_or(false);
            push_check(
                &mut checks,
                format!("{id}.dependency.{path}"),
                ok,
                "every transitive input exists with its recorded digest",
            );
        }
        for (label, field) in [("recipe", "recipe"), ("spec", "spec")] {
            let relative = asset[field].as_str().unwrap_or_default();
            let expected = asset["digests"][label].as_str().unwrap_or_default();
            let ok = pxl_store::digest::digest_file(&input.join(relative))
                .map(|actual| actual == expected)
                .unwrap_or(false);
            push_check(
                &mut checks,
                format!("{id}.{label}"),
                ok,
                "authored anchor has its recorded digest",
            );
        }
    }

    let bin = resolve_bin(pxl_bin)?;
    audio_checks(input, &manifest, &mut checks);
    let out_arg = std::env::temp_dir().join(format!("step-200-lint-{}", std::process::id()));
    let output = Command::new(&bin)
        .arg("--json")
        .arg("plan")
        .arg("--in")
        .arg(input)
        .arg("--out")
        .arg(&out_arg)
        .arg("--dry-run")
        .output()
        .map_err(|e| HarnessError::tool(format!("cannot run {}: {e}", bin.display())))?;
    let stdout = String::from_utf8(output.stdout).unwrap_or_default();
    let dry_run = stdout
        .lines()
        .next()
        .and_then(|line| serde_json::from_str::<Value>(line).ok())
        .unwrap_or_else(|| json!({"ok": false, "recipes": []}));
    let dry_run_ok = output.status.success() && dry_run["ok"] == true;
    push_check(
        &mut checks,
        "step300.semantic_compile",
        dry_run_ok,
        "all sources, macros, animations and in-memory QA pass",
    );
    if out_arg.exists() {
        let _ = std::fs::remove_dir_all(&out_arg);
        push_check(
            &mut checks,
            "step300.zero_output",
            false,
            "dry-run must not create its output directory",
        );
    } else {
        push_check(
            &mut checks,
            "step300.zero_output",
            true,
            "dry-run created no output directory",
        );
    }

    let recipes = dry_run["recipes"].as_array().cloned().unwrap_or_default();
    for asset in manifest["assets"].as_array().cloned().unwrap_or_default() {
        let id = asset["id"].as_str().unwrap_or_default();
        let recipe = asset["recipe"].as_str().unwrap_or_default();
        let compiled = recipes.iter().find(|entry| entry["recipe"] == recipe);
        let compiled_ok = compiled.is_some_and(|entry| entry["status"] == "ok");
        push_check(
            &mut checks,
            format!("{id}.compiled"),
            compiled_ok,
            "asset has a successful semantic compilation",
        );
        let Some(requirement) = requirements["requirements"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["asset_id"] == id))
        else {
            continue;
        };
        let documents = compiled
            .and_then(|entry| entry["documents"].as_array())
            .cloned()
            .unwrap_or_default();
        let document = documents
            .iter()
            .find(|doc| doc["asset"] == id)
            .or_else(|| documents.first());
        let size_ok = document.is_some_and(|doc| doc["size_px"] == requirement["size_px"]);
        push_check(
            &mut checks,
            format!("{id}.size.equals"),
            size_ok,
            format!("compiled size equals {}", requirement["size_px"]),
        );
        let expected_kind = match requirement["kind"].as_str().unwrap_or_default() {
            "tile" => "tiles",
            other => other,
        };
        let kind_ok = document.is_some_and(|doc| {
            doc["kind"].as_str().unwrap_or("sprite") == expected_kind
                || requirement["strategy"] == "parts" && doc["kind"] == "parts"
        });
        push_check(
            &mut checks,
            format!("{id}.asset.kind"),
            kind_ok,
            format!("compiled kind is {expected_kind}"),
        );
        for expected in requirement["anims"].as_array().cloned().unwrap_or_default() {
            let name = expected["name"].as_str().unwrap_or_default();
            let actual = document
                .and_then(|doc| doc["anims"].as_array())
                .and_then(|items| items.iter().find(|anim| anim["name"] == name));
            let ok = actual.is_some_and(|anim| {
                anim["frames"] == expected["frames"]
                    && anim["fps"] == expected["fps"]
                    && anim["looped"] == expected["looped"]
                    && anim["dirs"] == expected["dirs"]
            });
            push_check(
                &mut checks,
                format!("{id}.anim.{name}"),
                ok,
                "animation name, frame count, fps, loop and directions match the design",
            );
        }
        for acceptance in requirement["acceptance"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            if acceptance["type"] == "machine" && acceptance["check"] == "anim.anchor" {
                let ok = anchor_acceptance(&input.join(recipe), id, requirement, &acceptance);
                push_check(
                    &mut checks,
                    format!(
                        "{id}.acceptance.{}",
                        acceptance["id"].as_str().unwrap_or_default()
                    ),
                    ok,
                    "recipe implements the approved named anchor and tolerance",
                );
            }
        }
    }
    let failed = checks
        .iter()
        .filter(|check| check["result"] == "fail")
        .count();
    Ok(json!({
        "schema_version": 2,
        "cmd": "step-200 lint",
        "ok": failed == 0,
        "capability_digest": pxl_format::capability_digest(),
        "checks": checks,
        "dry_run": dry_run,
        "failed": failed,
        "visual_review": "not_run",
    }))
}

fn visual_plan_checks(input: &Path, checks: &mut Vec<Value>) {
    let result = (|| -> Result<Vec<String>> {
        let intent: pxl_contract::visual::VisualIntent = serde_json::from_str(
            &std::fs::read_to_string(input.join("visual-intent.snapshot.json"))
                .map_err(|e| HarnessError::io(format!("visual-intent.snapshot.json: {e}")))?,
        )
        .map_err(|e| HarnessError::schema(format!("visual-intent.snapshot.json: {e}")))?;
        let plan: pxl_contract::plan::VisualPlan = serde_json::from_str(
            &std::fs::read_to_string(input.join("visual-plan.json"))
                .map_err(|e| HarnessError::io(format!("visual-plan.json: {e}")))?,
        )
        .map_err(|e| HarnessError::schema(format!("visual-plan.json: {e}")))?;
        let mut errors = intent.validate();
        errors.extend(plan.validate(&intent));
        let actual = pxl_store::digest::digest_file(&input.join("visual-intent.snapshot.json"))?;
        if plan.intent_digest != actual {
            errors.push(format!(
                "visual plan intent digest '{}' does not match frozen '{actual}'",
                plan.intent_digest
            ));
        }
        // P04: a v2 intent's palette families must resolve here too — every
        // palette file must exist with its frozen byte and semantic digests,
        // and every asset's recipe must point at its family's palette.
        if intent.is_v2() && !intent.palette_families.is_empty() {
            let registry = pxl_contract::palette_family::FamilyRegistry::resolve(&intent, input)
                .map_err(|e| HarnessError::schema(format!("palette families: {e}")))?;
            for asset in &intent.asset_policies {
                let Some(family_id) = &asset.palette_family else {
                    continue;
                };
                let Some(family) = registry.get(family_id) else {
                    errors.push(format!(
                        "asset '{}' references palette family '{family_id}' that did not resolve",
                        asset.asset_id
                    ));
                    continue;
                };
                let recipe = format!("{}/{}/{}.recipe.json", input.display(), asset.asset_id, asset.asset_id);
                let Ok(text) = std::fs::read_to_string(&recipe) else {
                    // The recipe's absence is a separate manifest check; here
                    // we only judge recipes that exist.
                    continue;
                };
                let recipe_value: Value = match serde_json::from_str(&text) {
                    Ok(value) => value,
                    Err(_) => {
                        errors.push(format!(
                            "asset '{}' recipe is not valid JSON; cannot verify its family palette",
                            asset.asset_id
                        ));
                        continue;
                    }
                };
                let declared = recipe_value["palette"].as_str().unwrap_or_default();
                let declared_name = std::path::Path::new(declared)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                let expected_name = std::path::Path::new(&family.palette_path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                if declared_name != expected_name {
                    errors.push(format!(
                        "asset '{}' recipe declares palette '{declared_name}' but palette family '{family_id}' froze '{expected_name}'",
                        asset.asset_id
                    ));
                }
            }
        }
        Ok(errors)
    })();
    match result {
        Ok(errors) if errors.is_empty() => push_check(
            checks,
            "visual.plan",
            true,
            "visual plan closes against the frozen visual intent",
        ),
        Ok(errors) => {
            for (index, error) in errors.into_iter().enumerate() {
                push_check(checks, format!("visual.plan.{index}"), false, error);
            }
        }
        Err(error) => push_check(
            checks,
            "visual.plan",
            false,
            format!("{}: visual plan is required and must use visual-plan/v1", error.message),
        ),
    }
}

/// Audio checks. Zero model calls and zero output: the spec is validated
/// in-process by `pxl-audio` and nothing is rendered, so this needs no Node.
/// A spec directory with no audio at all adds no checks.
fn audio_checks(input: &Path, manifest: &Value, checks: &mut Vec<Value>) {
    let spec_path = input.join("audio/audio.spec.json");
    let block = &manifest["audio"];
    if !spec_path.is_file() && block.is_null() {
        return;
    }
    push_check(
        checks,
        "audio.spec.present",
        spec_path.is_file(),
        "audio/audio.spec.json exists when the manifest lists audio",
    );
    if !spec_path.is_file() {
        return;
    }
    let validation = pxl_audio::validate::validate_spec(&spec_path);
    push_check(
        checks,
        "audio.spec.valid",
        validation.is_ok(),
        match &validation {
            Ok(_) => "audio-spec/v1 and every song pass pxl-audio validation".to_string(),
            Err(err) => err.message.clone(),
        },
    );
    for dependency in block["dependencies"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let path = dependency["path"].as_str().unwrap_or_default();
        let expected = dependency["digest"].as_str().unwrap_or_default();
        let ok = !path.is_empty()
            && pxl_store::digest::digest_file(&input.join(path))
                .map(|actual| actual == expected)
                .unwrap_or(false);
        push_check(
            checks,
            format!("audio.dependency.{path}"),
            ok,
            "every audio input exists with its recorded digest",
        );
    }
}

fn anchor_acceptance(
    recipe_path: &Path,
    id: &str,
    requirement: &Value,
    acceptance: &Value,
) -> bool {
    let Ok(text) = std::fs::read_to_string(recipe_path) else {
        return false;
    };
    let Ok(recipe) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    let Some(sprite) = recipe["sprites"]
        .as_array()
        .and_then(|items| items.iter().find(|sprite| sprite["id"] == id))
    else {
        return false;
    };
    let name = acceptance["anim"].as_str().unwrap_or_default();
    let Some(anim) = sprite["anims"]
        .as_array()
        .and_then(|items| items.iter().find(|anim| anim["name"] == name))
    else {
        return false;
    };
    let Some(anchor_name) = anim["anchor"].as_str() else {
        return false;
    };
    let expected = [
        requirement["anchor"]["center_x"].as_i64(),
        requirement["anchor"]["bottom_y"].as_i64(),
    ];
    let actual = sprite["anchors"][anchor_name].as_array().map(|values| {
        [
            values.first().and_then(Value::as_i64),
            values.get(1).and_then(Value::as_i64),
        ]
    });
    let tolerance = acceptance["tolerance_px"]
        .as_u64()
        .or_else(|| {
            requirement["anims"]
                .as_array()
                .and_then(|items| items.iter().find(|item| item["name"] == name))
                .and_then(|item| item["anchor_tolerance_px"].as_u64())
        })
        .unwrap_or(0);
    actual == Some(expected) && anim["anchor_tolerance_px"].as_u64().unwrap_or(0) == tolerance
}

fn push_check(checks: &mut Vec<Value>, id: impl Into<String>, ok: bool, detail: impl Into<String>) {
    checks.push(json!({"id": id.into(), "result": if ok { "pass" } else { "fail" }, "detail": detail.into()}));
}

pub fn lint(args: LintArgs) -> Result<i32> {
    let report = lint_report(&args.input, args.pxl_bin.as_deref())?;
    let reports = args.input.join("reports");
    std::fs::create_dir_all(&reports)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", reports.display())))?;
    let mut text = serde_json::to_string_pretty(&report)
        .map_err(|e| HarnessError::io(format!("lint report: {e}")))?;
    text.push('\n');
    std::fs::write(reports.join("art-spec.lint.json"), text)
        .map_err(|e| HarnessError::io(format!("cannot write lint report: {e}")))?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(if report["ok"] == true { 0 } else { 11 })
}

pub fn capabilities(args: CapabilitiesArgs) -> Result<i32> {
    if args.action == "manifest" || args.kind == "capabilities" {
        println!("{}", pxl_format::CAPABILITIES_JSON.trim_end());
        return Ok(0);
    }
    let bin = resolve_bin(args.pxl_bin.as_deref())?;
    let argv: Vec<String> = match args.action.as_str() {
        "schema" => vec!["--json".into(), "schema".into(), args.kind.clone()],
        _ => vec!["--json".into(), "list".into(), args.kind.clone()],
    };
    let output = Command::new(&bin)
        .args(&argv)
        .output()
        .map_err(|e| HarnessError::tool(format!("cannot run {}: {e}", bin.display())))?;
    let stdout = String::from_utf8(output.stdout).unwrap_or_default();
    let stderr = String::from_utf8(output.stderr).unwrap_or_default();
    print!("{stdout}");
    if !stderr.trim().is_empty() {
        eprint!("{stderr}");
    }
    Ok(output.status.code().unwrap_or(1))
}

pub fn status(args: StatusArgs) -> Result<i32> {
    let path = args.input.join("step.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| HarnessError::schema(format!("step.json: {e}")))?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(0)
}

fn resolve_bin(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(bin) = explicit {
        if bin.is_file() {
            return Ok(bin.to_path_buf());
        }
        return Err(HarnessError::not_found(format!(
            "pxl binary {} does not exist",
            bin.display()
        )));
    }
    let mut cfg = pxl_agent::config::Config::default();
    cfg.pxl_bin = String::new();
    PxlAdapter::discover(&cfg).map(|a| a.bin_path().to_path_buf())
}
