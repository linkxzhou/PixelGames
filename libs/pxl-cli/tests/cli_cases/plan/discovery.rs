use std::fs;

use serde_json::Value;

use crate::support::{json_line, pxl, repo_root};

use super::helpers::{bullet_dir, install_visual_contract, temp_dir};

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

    let (code, stdout, stderr) = pxl(&[
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
    let (code, _stdout, _stderr) = pxl(&[
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
    let (code, stdout, stderr) = pxl(&[
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
    let (code, stdout, stderr) = pxl(&[
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
