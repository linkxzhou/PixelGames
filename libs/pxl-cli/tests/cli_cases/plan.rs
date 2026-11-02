//! `plan` — the batch entry between steps (plan/v3.0/pxl-step-pipeline-plan.md
//! §4.3). It must reuse the `build` pipeline byte for byte, never abort the
//! batch on one failed recipe, and write an index.json the package step can
//! consume.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use crate::support::{json_line, pxl, repo_root};

fn step300(args: &[&str]) -> (i32, String, String) {
    let bin = env!("CARGO_BIN_EXE_step-300");
    let out = std::process::Command::new(bin)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("step-300 binary at {bin}: {e}"));
    (
        out.status.code().unwrap_or(1),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pxl-plan-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn bullet_dir() -> PathBuf {
    repo_root().join("examples/static_bullet")
}

fn install_visual_contract(root: &PathBuf, asset_dir: &str) {
    let fixture = repo_root().join(
        "libs/pxl-cli/tests/fixtures/visual-contract",
    );
    for name in ["visual-intent.snapshot.json", "visual-plan.json"] {
        fs::copy(fixture.join(name), root.join(name)).unwrap();
    }
    fs::copy(
        fixture.join("static_bullet.shapes.json"),
        root.join(asset_dir).join("static_bullet.shapes.json"),
    )
    .unwrap();
}

/// `plan` and `build` produce byte-identical outputs for the same recipe.
#[test]
fn plan_matches_build_bytes() {
    let recipe = bullet_dir().join("static_bullet.recipe.json");
    let build_out = temp_dir("build");
    let plan_in = temp_dir("plan-in");
    let plan_out = temp_dir("plan-out");

    // Assemble a step-200 style tree: <id>/<id>.recipe.json + siblings.
    fs::create_dir_all(plan_in.join("static_bullet")).unwrap();
    for entry in fs::read_dir(bullet_dir()).unwrap().flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".json") {
            fs::copy(entry.path(), plan_in.join("static_bullet").join(&name)).unwrap();
        }
    }

    let (build_code, build_stdout, build_stderr) = pxl(&[
        "--json",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        build_out.to_str().unwrap(),
    ]);
    assert_eq!(
        build_code, 0,
        "build\nstdout={build_stdout}\nstderr={build_stderr}"
    );

    let (plan_code, plan_stdout, plan_stderr) = step300(&[
        "--json",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
    ]);
    assert_eq!(
        plan_code, 0,
        "plan\nstdout={plan_stdout}\nstderr={plan_stderr}"
    );

    // Every published file the build wrote must be byte-identical under plan.
    let mut names: Vec<String> = fs::read_dir(&build_out)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json") || n.ends_with(".png"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "build produced no outputs");
    for name in &names {
        let a = fs::read(build_out.join(name)).unwrap();
        let b = fs::read(plan_out.join("static_bullet").join(name)).unwrap();
        assert_eq!(a, b, "plan and build differ on {name}");
    }

    let index: Value =
        serde_json::from_str(&fs::read_to_string(plan_out.join("index.json")).unwrap()).unwrap();
    assert_eq!(index["schema_version"], 5);
    assert_eq!(index["scope"], "all");
    let recipes = index["recipes"].as_array().unwrap();
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0]["id"], "static_bullet");
    assert_eq!(recipes[0]["status"], "ok");
    assert!(recipes[0]["manifest"]
        .as_str()
        .unwrap()
        .ends_with(".build.json"));
}

#[test]
fn baseline_rebuilds_twice_and_separates_review_states() {
    let plan_in = temp_dir("baseline-in");
    let plan_out = temp_dir("baseline-out");
    fs::create_dir_all(plan_in.join("static_bullet")).unwrap();
    for entry in fs::read_dir(bullet_dir()).unwrap().flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".json") {
            fs::copy(entry.path(), plan_in.join("static_bullet").join(&name)).unwrap();
        }
    }
    let (code, stdout, stderr) = step300(&[
        "--json",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
        "--baseline",
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let baseline: Value =
        serde_json::from_str(&fs::read_to_string(plan_out.join("baseline.json")).unwrap()).unwrap();
    assert_eq!(baseline["ok"], true);
    assert_eq!(baseline["assets"][0]["deterministic_pass"], true);
    assert_eq!(baseline["assets"][0]["contract_pass"], true);
    assert_eq!(baseline["assets"][0]["visual_review"], "not_run");
    assert_eq!(baseline["model_robustness"], "not_run");
    assert!(
        baseline["assets"][0]["indexed_frames"]
            .as_object()
            .unwrap()
            .len()
            > 0
    );
    assert!(!plan_out.join(".baseline").exists());
}

/// `--dry-run` parses and validates without writing anything.
#[test]
fn plan_dry_run_writes_nothing() {
    let plan_in = temp_dir("dry-in");
    let plan_out = temp_dir("dry-out");
    fs::create_dir_all(plan_in.join("static_bullet")).unwrap();
    for entry in fs::read_dir(bullet_dir()).unwrap().flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".json") {
            fs::copy(entry.path(), plan_in.join("static_bullet").join(&name)).unwrap();
        }
    }

    let (code, stdout, stderr) = step300(&[
        "--json",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let report = json_line(&stdout);
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["checked"], 1);
    assert_eq!(report["ok_count"], 1);
    assert!(
        !plan_out.exists(),
        "--dry-run must not create the output directory"
    );

    // A recipe whose palette does not resolve fails the dry run, is reported
    // in the summary line, and still writes nothing. In --json mode the
    // summary and the trailing error are two objects on stdout, so parse the
    // first line.
    let broken = plan_in.join("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(
        broken.join("broken.recipe.json"),
        r#"{ "v": 1, "asset": "broken", "seed": 1, "palette": "no-such.palette.json",
             "sprites": [], "tiles": [], "sheet": { "sources": [], "file": "broken.sheet.png" } }"#,
    )
    .unwrap();
    let (code, stdout, stderr) = step300(&[
        "--json",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_ne!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let report: Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    assert_eq!(report["ok"], false);
    assert!(report["ok_count"].as_u64().unwrap() < report["checked"].as_u64().unwrap());
    assert!(
        !plan_out.exists(),
        "--dry-run must not create the output directory, even on failure"
    );
}

/// One failed recipe never aborts the batch: the good one still builds, the
/// bad one is recorded, and the exit code is 5.
#[test]
fn failed_recipe_does_not_abort_the_batch() {
    let plan_in = temp_dir("mixed-in");
    let plan_out = temp_dir("mixed-out");

    // Good recipe.
    let good = plan_in.join("static_bullet");
    fs::create_dir_all(&good).unwrap();
    for entry in fs::read_dir(bullet_dir()).unwrap().flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".json") {
            fs::copy(entry.path(), good.join(&name)).unwrap();
        }
    }

    // Bad recipe: names a spec that does not exist.
    let bad = plan_in.join("broken");
    fs::create_dir_all(&bad).unwrap();
    fs::write(
        bad.join("broken.recipe.json"),
        r#"{ "v": 1, "asset": "broken", "seed": 1, "palette": "p16",
             "sprites": [ { "id": "broken", "base": { "gen": "shapes", "spec": "missing.shapes.json" } } ],
             "tiles": [], "sheet": { "sources": [ "broken" ], "file": "broken.sheet.png" } }"#,
    )
    .unwrap();

    let (code, stdout, stderr) = step300(&[
        "--json",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
    ]);
    assert_eq!(
        code, 5,
        "the worst status must win\nstdout={stdout}\nstderr={stderr}"
    );

    let index: Value =
        serde_json::from_str(&fs::read_to_string(plan_out.join("index.json")).unwrap()).unwrap();
    let recipes = index["recipes"].as_array().unwrap();
    assert_eq!(recipes.len(), 2, "both entries must be recorded");
    let by_id = |id: &str| {
        recipes
            .iter()
            .find(|r| r["id"] == id)
            .unwrap_or_else(|| panic!("missing {id} in index.json"))
    };
    assert_eq!(by_id("static_bullet")["status"], "ok");
    assert_eq!(by_id("broken")["status"], "parse_error");
    assert!(by_id("broken")["error"]
        .as_str()
        .unwrap()
        .contains("missing.shapes.json"));
    // The good recipe's outputs were still published.
    assert!(plan_out
        .join("static_bullet/static_bullet.build.json")
        .is_file());
}

#[test]
fn dry_run_compiles_svg_and_strict_covers_variant_warnings() {
    let input = temp_dir("bad-svg-in");
    let output = temp_dir("bad-svg-out");
    fs::create_dir_all(input.join("broken")).unwrap();
    fs::write(input.join("broken/broken.recipe.json"), r#"{"v":1,"asset":"broken","gen_version":1,"palette":"p16","sprites":[{"id":"broken","base":{"gen":"sprite","spec":"broken.sprite.json"}}],"sheet":{"sources":["broken"],"file":"broken.sheet.png"}}"#).unwrap();
    fs::write(
        input.join("broken/broken.sprite.json"),
        r#"{"v":3,"size":32,"palette":"p16","source":{"kind":"svg","file":"broken.svg"}}"#,
    )
    .unwrap();
    fs::write(input.join("broken/broken.svg"), "<svg><").unwrap();
    let (code, stdout, _stderr) = step300(&[
        "--json",
        "plan",
        "--in",
        input.to_str().unwrap(),
        "--out",
        output.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_ne!(code, 0, "bad SVG must not pass dry-run: {stdout}");
    assert!(!output.exists());

    let out = temp_dir("strict-morden").join("morden.png");
    let input = repo_root().join("examples/v3/morden/morden.sprite.json");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "--strict",
        "sprite",
        "build",
        input.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_ne!(
        code, 0,
        "variant warning must fail strict: {stdout}\n{stderr}"
    );
    assert!(format!("{stdout}\n{stderr}").contains("palette_over_tier"));
    assert!(
        !out.exists(),
        "failed bundle must not publish the primary PNG"
    );
}

#[test]
fn sprite_build_publishes_outputs_and_commit_manifest() {
    let input = temp_dir("sprite-publish-in").join("asset.sprite.json");
    fs::create_dir_all(input.parent().unwrap()).unwrap();
    fs::write(&input, r##"{"v":3,"size":32,"palette":"p16","source":{"kind":"svg","svg":"<svg viewBox=\"0 0 8 8\"><rect x=\"1\" y=\"1\" width=\"6\" height=\"6\" fill=\"#d04648\"/></svg>"},"finish":{"skip":true}}"##).unwrap();
    let out = temp_dir("sprite-publish-out").join("asset.png");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "sprite",
        "build",
        input.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let manifest_path = out.parent().unwrap().join("asset.sprite.build.json");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["schema_version"], 2);
    for name in ["asset.png", "asset.pxl.json", "asset.palette.json"] {
        assert!(out.parent().unwrap().join(name).is_file(), "missing {name}");
        assert!(
            manifest["outputs"].get(name).is_some(),
            "manifest misses {name}"
        );
    }
    assert!(!out.parent().unwrap().join(".staging").exists());
}

#[test]
fn transitive_svg_changes_the_build_input_digest() {
    let input = temp_dir("closure-in");
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("asset.recipe.json"), r#"{"v":1,"asset":"asset","gen_version":1,"palette":"p16","sprites":[{"id":"asset","base":{"gen":"sprite","spec":"asset.sprite.json"}}],"sheet":{"sources":["asset"],"file":"asset.sheet.png"}}"#).unwrap();
    fs::write(
        input.join("asset.sprite.json"),
        r#"{"v":3,"size":32,"palette":"p16","source":{"kind":"svg","file":"asset.svg"}}"#,
    )
    .unwrap();
    let svg = input.join("asset.svg");
    fs::write(
        &svg,
        r##"<svg viewBox="0 0 8 8"><rect x="1" y="1" width="6" height="6" fill="#d04648"/></svg>"##,
    )
    .unwrap();
    let mut manifests = Vec::new();
    for index in 0..2 {
        if index == 1 {
            fs::write(&svg, r##"<svg viewBox="0 0 8 8"><rect x="0" y="1" width="6" height="6" fill="#d04648"/></svg>"##).unwrap();
        }
        let out = temp_dir(&format!("closure-out-{index}"));
        let (code, stdout, stderr) = pxl(&[
            "--json",
            "build",
            input.join("asset.recipe.json").to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(out.join("asset.build.json")).unwrap())
                .unwrap();
        assert!(
            manifest["inputs"]["specs"].get("asset.svg").is_some(),
            "transitive SVG missing: {manifest}"
        );
        manifests.push(
            manifest["inputs"]["closure_digest"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    assert_ne!(manifests[0], manifests[1]);
}

/// A spec-manifest.json (the step-200 contract) drives discovery, honors
/// --asset, and records the scope label.
#[test]
fn spec_manifest_drives_discovery() {
    let plan_in = temp_dir("manifest-in");
    let plan_out = temp_dir("manifest-out");

    let bullet = plan_in.join("bullet");
    fs::create_dir_all(&bullet).unwrap();
    for entry in fs::read_dir(bullet_dir()).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".json") {
            fs::copy(entry.path(), bullet.join(&name)).unwrap();
        }
    }
    install_visual_contract(&plan_in, "bullet");
    let recipe_rel = "bullet/static_bullet.recipe.json";
    let manifest = format!(
        r#"{{
            "schema_version": 3,
            "scope": "slice",
            "palette": "bullet/palette.json",
            "assets": [
                {{ "id": "bullet", "kind": "shapes",
                   "spec": "bullet/static_bullet.shapes.json",
                   "recipe": "{recipe_rel}",
                   "status": "ok",
                   "digests": {{ "spec": "sha256:{zeros}", "recipe": "sha256:{zeros}" }} }}
            ]
        }}"#,
        zeros = "0".repeat(64)
    );
    fs::write(plan_in.join("spec-manifest.json"), manifest).unwrap();

    let (code, stdout, stderr) = step300(&[
        "--json",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
        "--scope",
        "slice",
        "--asset",
        "bullet",
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");

    let index: Value =
        serde_json::from_str(&fs::read_to_string(plan_out.join("index.json")).unwrap()).unwrap();
    assert_eq!(index["scope"], "slice");
    let recipes = index["recipes"].as_array().unwrap();
    assert_eq!(recipes.len(), 1, "only the --asset filter may build");
    assert_eq!(recipes[0]["id"], "bullet");
    assert_eq!(recipes[0]["status"], "ok");
    assert_eq!(recipes[0]["recipe"], recipe_rel);
    let effective: Value = serde_json::from_str(
        &fs::read_to_string(plan_out.join("bullet/effective-style.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(effective["contract"], "effective-style/v1");
    assert_eq!(effective["assets"][0]["palette"]["mode"], "strict");
    assert_eq!(effective["assets"][0]["palette"]["extension"], 0);
    assert!(effective["assets"][0]["frames"]
        .as_array()
        .unwrap()
        .iter()
        .any(|frame| frame["protect_pixels"].as_u64().unwrap_or(0) > 0));

    // The --asset filter that matches nothing is a semantic error.
    let (code, _stdout, _stderr) = step300(&[
        "--json",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
        "--asset",
        "no_such_asset",
    ]);
    assert_eq!(code, 4);
}

#[test]
fn plan_builds_the_audio_entry_and_dry_run_stays_output_free() {
    let node_ok = std::process::Command::new("node")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|version| {
            version
                .trim()
                .trim_start_matches('v')
                .split('.')
                .next()?
                .parse::<u32>()
                .ok()
        })
        .is_some_and(|major| major >= 22);
    if !node_ok {
        eprintln!("SKIP: Node 22+ not found; audio plan render is NOT_ASSESSED");
        return;
    }
    let plan_in = temp_dir("audio-plan-in");
    let dry_out = temp_dir("audio-plan-dry-out");
    let plan_out = temp_dir("audio-plan-out");
    fs::create_dir_all(plan_in.join("static_bullet")).unwrap();
    for entry in fs::read_dir(bullet_dir()).unwrap().flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".json") {
            fs::copy(entry.path(), plan_in.join("static_bullet").join(&name)).unwrap();
        }
    }
    install_visual_contract(&plan_in, "static_bullet");
    let audio_source = repo_root().join(
        "libs/pxl-cli/tests/fixtures/audio",
    );
    fs::create_dir_all(plan_in.join("audio/songs")).unwrap();
    fs::copy(
        audio_source.join("audio.spec.json"),
        plan_in.join("audio/audio.spec.json"),
    )
    .unwrap();
    fs::copy(
        audio_source.join("songs/theme.beepbox.json"),
        plan_in.join("audio/songs/theme.beepbox.json"),
    )
    .unwrap();
    fs::write(
        plan_in.join("spec-manifest.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 3,
            "scope": "all",
            "palette": null,
            "capability_digest": "fixture",
            "assets": [{"id": "static_bullet", "recipe": "static_bullet/static_bullet.recipe.json"}],
            "audio": {"contract": "audio-spec/v1", "spec": "audio/audio.spec.json", "status": "ok"},
        }))
        .unwrap(),
    )
    .unwrap();
    let (code, stdout, stderr) = step300(&[
        "--json",
        "plan",
        "--dry-run",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        dry_out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    assert_eq!(json_line(&stdout)["audio"]["status"], "ok");
    assert!(!dry_out.exists());
    let (code, stdout, stderr) = step300(&[
        "--json",
        "--strict",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let index: Value =
        serde_json::from_str(&fs::read_to_string(plan_out.join("index.json")).unwrap()).unwrap();
    assert_eq!(index["schema_version"], 5);
    assert_eq!(index["audio"]["status"], "ok");
    assert_eq!(index["audio"]["entries"], 4);
    assert!(plan_out.join("audio/audio-manifest.json").is_file());
    assert!(plan_out.join("audio/bgm/theme.wav").is_file());
}
