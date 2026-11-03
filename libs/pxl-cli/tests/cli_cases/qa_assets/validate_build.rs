use crate::support::{build_to, json_line, pxl, repo_root};
use serde_json::Value;
use std::fs;

/// Default QA checks every asset; freezing the second animation must report
/// duplicate frames under that asset's own animation name.
#[test]
fn default_rules_check_the_second_asset_in_a_sheet() {
    let root = repo_root();
    let src = root.join("examples/tank");
    let dir = std::env::temp_dir().join(format!("pxl-two-tanks-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for entry in fs::read_dir(&src).unwrap().flatten() {
        let p = entry.path();
        if p.is_file() {
            fs::copy(&p, dir.join(p.file_name().unwrap())).unwrap();
        }
    }
    let mut recipe: Value =
        serde_json::from_str(&fs::read_to_string(src.join("tank.recipe.json")).unwrap()).unwrap();
    let mut second = recipe["sprites"][0].clone();
    second["id"] = Value::String("tank2".into());
    recipe["sprites"].as_array_mut().unwrap().push(second);
    recipe["sheet"]["sources"] = serde_json::json!(["tank", "tank2", "ground"]);
    recipe["preview"] = serde_json::json!([]);
    let path = dir.join("two.recipe.json");
    fs::write(&path, serde_json::to_string(&recipe).unwrap()).unwrap();
    let out = dir.join("out");
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        path.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "two healthy tanks pass: {stdout}");
    let report = json_line(&stdout);
    let ids: Vec<&str> = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["id"].as_str())
        .collect();
    assert!(
        ids.iter()
            .any(|id| id.starts_with("anim.tank2_move.") && id.contains("duplicate_")),
        "tank2 got no duplicate-frame checks: {ids:?}"
    );

    // Freeze tank2's move: every frame becomes its first frame.
    let sheet_path = out.join("tank.sheet.json");
    let mut sheet: Value = serde_json::from_str(&fs::read_to_string(&sheet_path).unwrap()).unwrap();
    let dirs = sheet["meta"]["pxl"]["anims"]["tank2_move"]["dirs"]
        .as_object_mut()
        .unwrap();
    for list in dirs.values_mut() {
        let first = list[0].clone();
        for slot in list.as_array_mut().unwrap() {
            *slot = first.clone();
        }
    }
    fs::write(&sheet_path, serde_json::to_string(&sheet).unwrap()).unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "validate",
        sheet_path.to_str().unwrap(),
        "--rules",
        "default",
    ]);
    assert_eq!(code, 5, "a frozen second tank must fail: {stdout}");
    let report = json_line(&stdout);
    let failed: Vec<&str> = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["pass"] == false && c["severity"] == "error")
        .filter_map(|c| c["id"].as_str())
        .collect();
    assert!(
        failed
            .iter()
            .any(|id| id.contains("tank2") && id.contains("duplicate_")),
        "no duplicate-frame failure names tank2: {failed:?}"
    );
    assert!(
        !failed
            .iter()
            .any(|id| id.starts_with("anim.move.") && !id.contains("tank2")),
        "the healthy first tank must not fail: {failed:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Default QA judges every recolour: freezing the second animation must
/// produce a blocking check that names that asset.
#[test]
fn default_rules_check_every_recolour() {
    let out = build_to(
        &repo_root().join("examples/farmer/farmer.recipe.json"),
        "char-all",
    );
    let sheet_path = out.join("farmer.sheet.json");
    let mut sheet: Value = serde_json::from_str(&fs::read_to_string(&sheet_path).unwrap()).unwrap();
    let dirs = sheet["meta"]["pxl"]["anims"]["farmer_blonde_walk"]["dirs"]
        .as_object_mut()
        .unwrap();
    for list in dirs.values_mut() {
        let first = list[0].clone();
        for slot in list.as_array_mut().unwrap() {
            *slot = first.clone();
        }
    }
    fs::write(&sheet_path, serde_json::to_string(&sheet).unwrap()).unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "validate",
        sheet_path.to_str().unwrap(),
        "--rules",
        "default",
    ]);
    assert_eq!(
        code, 5,
        "a frozen recolour walk must fail default QA: {stdout}"
    );
    let report = json_line(&stdout);
    let failed: Vec<&str> = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["pass"] == false && c["severity"] == "error")
        .filter_map(|c| c["id"].as_str())
        .collect();
    assert!(
        failed.iter().any(|id| id.contains("farmer_blonde_walk")),
        "no failing check names the broken recolour: {failed:?}"
    );
    assert!(
        !failed.iter().any(|id| id.starts_with("anim.walk.")),
        "the untouched first farmer must keep passing: {failed:?}"
    );
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn validate_reads_json_config_for_docs_and_sheets() {
    let out = build_to(
        &repo_root().join("examples/farmer/farmer.recipe.json"),
        "qa-config",
    );
    let config = out.join("qa.json");
    fs::write(
        &config,
        serde_json::json!({
            "max_area_change_bps": 0,
            "max_centroid_shift_millipixels": 0,
            "max_samples": 2,
            "allow_empty_frames": true
        })
        .to_string(),
    )
    .unwrap();
    for name in ["farmer.sheet.json", "farmer.pxl.json"] {
        let input = out.join(name);
        let (code, stdout, _) = pxl(&[
            "--json",
            "validate",
            input.to_str().unwrap(),
            "--qa-config",
            config.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{stdout}");
        let report = json_line(&stdout);
        assert_eq!(report["rules"], "custom");
        let checks = report["checks"].as_array().unwrap();
        for metric in [".area_", ".centroid_"] {
            assert!(
                checks.iter().any(|c| c["pass"] == false
                    && c["severity"] == "warning"
                    && c["id"].as_str().unwrap().contains(metric)),
                "missing {metric} warning: {report}"
            );
        }
        assert!(checks
            .iter()
            .all(|c| c["samples"].as_array().unwrap().len() <= 2));
        assert!(checks
            .iter()
            .any(|c| !c["samples"].as_array().unwrap().is_empty()));
    }
    fs::write(&config, r#"{"strict":true,"max_area_change_bps":0}"#).unwrap();
    let input = out.join("farmer.sheet.json");
    let (code, stdout, _) = pxl(&[
        "--json",
        "validate",
        input.to_str().unwrap(),
        "--qa-config",
        config.to_str().unwrap(),
    ]);
    assert_eq!(code, 5, "{stdout}");
    let report = json_line(&stdout);
    let failed = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["pass"] == false && c["severity"] == "error")
        .count();
    assert_eq!(
        report["error"]["message"],
        format!("{failed} check(s) failed")
    );
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn validate_rejects_bad_configs_and_removed_rules() {
    let out = build_to(
        &repo_root().join("examples/static_bullet/static_bullet.recipe.json"),
        "qa-config-errors",
    );
    let input = out.join("static_bullet.sheet.json");
    let config = out.join("qa.json");
    for (text, expected) in [
        ("{", 3),
        (r#"{"unknown_option":1}"#, 3),
        (r#"{"max_samples":1025}"#, 2),
    ] {
        fs::write(&config, text).unwrap();
        let (code, stdout, _) = pxl(&[
            "--json",
            "validate",
            input.to_str().unwrap(),
            "--qa-config",
            config.to_str().unwrap(),
        ]);
        assert_eq!(code, expected, "{stdout}");
    }
    fs::write(&config, "{}").unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "validate",
        input.to_str().unwrap(),
        "--rules",
        "strict",
        "--qa-config",
        config.to_str().unwrap(),
    ]);
    assert_eq!(code, 2, "{stdout}");
    for rules in ["tank", "character", "unknown"] {
        let (code, stdout, _) = pxl(&[
            "--json",
            "validate",
            input.to_str().unwrap(),
            "--rules",
            rules,
        ]);
        assert_eq!(code, 2, "{stdout}");
        assert_eq!(json_line(&stdout)["error"]["code"], "E_USAGE");
    }
    let (code, stdout, _) = pxl(&[
        "--json",
        "validate",
        input.to_str().unwrap(),
        "--qa-config",
        out.join("missing.json").to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "{stdout}");
    assert_eq!(json_line(&stdout)["error"]["code"], "E_IO");
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn build_config_overrides_recipe_and_records_effective_policy() {
    let src = repo_root().join("examples/static_bullet");
    let dir = std::env::temp_dir().join(format!("pxl-build-config-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for name in ["static_bullet.shapes.json", "static_bullet.palette.json"] {
        fs::copy(src.join(name), dir.join(name)).unwrap();
    }
    let mut recipe: Value =
        serde_json::from_str(&fs::read_to_string(src.join("static_bullet.recipe.json")).unwrap())
            .unwrap();
    recipe["validate"]["rules"] = serde_json::json!("strict");
    recipe["sprites"][0]["anims"][0]["mode"] = serde_json::json!("dynamic");
    recipe["sprites"][0]["anims"][0]["frames"] =
        serde_json::json!([[{"op":"noop"}], [{"op":"noop"}]]);
    let recipe_path = dir.join("hold.recipe.json");
    fs::write(&recipe_path, recipe.to_string()).unwrap();
    let config = dir.join("qa.json");
    fs::write(
        &config,
        r#"{"allow_duplicate_frames":true,"max_samples":1}"#,
    )
    .unwrap();
    let out = dir.join("out");
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        recipe_path.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
        "--qa-config",
        config.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let manifest_path = out.join("static_bullet.build.json");
    let before = fs::read(&manifest_path).unwrap();
    let manifest: Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(manifest["qa"]["rules"], "custom");
    assert_eq!(manifest["qa"]["config"]["allow_duplicate_frames"], true);
    assert_eq!(manifest["qa"]["config"]["strict"], false);
    assert_eq!(manifest["qa"]["config"]["max_samples"], 1);
    assert_eq!(manifest["qa"]["failed"], 0);
    assert!(
        manifest["qa"]["config"]["max_frame_pixels"]
            .as_u64()
            .unwrap()
            > 0
    );
    fs::write(&config, "{}").unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        recipe_path.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
        "--qa-config",
        config.to_str().unwrap(),
    ]);
    assert_eq!(code, 5, "{stdout}");
    assert_eq!(fs::read(&manifest_path).unwrap(), before);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn global_strict_gates_validate_and_build_with_or_without_config() {
    let recipe = repo_root().join("examples/battle_city/explosion.recipe.json");
    let out = build_to(&recipe, "global-strict");
    let input = out.join("explosion.sheet.json");
    let config = out.join("qa.json");
    fs::write(&config, r#"{"strict":false}"#).unwrap();
    for custom in [false, true] {
        for command in ["validate", "build"] {
            let rejected = out.join(format!("strict-{custom}"));
            let mut args = vec!["--json", "--strict", command];
            if command == "validate" {
                args.push(input.to_str().unwrap());
            } else {
                args.extend([recipe.to_str().unwrap(), "-o", rejected.to_str().unwrap()]);
            }
            if custom {
                args.extend(["--qa-config", config.to_str().unwrap()]);
            }
            let (code, stdout, _) = pxl(&args);
            assert_eq!(code, 5, "{command}, config={custom}: {stdout}");
            let report = json_line(&stdout);
            assert!(
                report["checks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["pass"] == false
                        && c["severity"] == "error"
                        && c["id"].as_str().unwrap().contains(".area_")),
                "{report}"
            );
        }
    }
    let (code, stdout, _) = pxl(&[
        "--json",
        "--strict",
        "validate",
        input.to_str().unwrap(),
        "--rules",
        "unknown",
    ]);
    assert_eq!(code, 2, "{stdout}");
    let mut relaxed = pxl_qa::QaConfig::default();
    relaxed.max_colors = None;
    relaxed.max_components = None;
    relaxed.max_isolated_pixels = None;
    relaxed.max_holes = None;
    relaxed.max_color_specks = None;
    relaxed.max_outline_gaps = None;
    relaxed.max_duplicate_colors = None;
    relaxed.max_checkerboard_bps = None;
    relaxed.min_neighbor_contrast = None;
    relaxed.max_animation_diff_bps = None;
    relaxed.max_area_change_bps = None;
    relaxed.max_centroid_shift_millipixels = None;
    relaxed.max_palette_change_bps = None;
    fs::write(&config, serde_json::to_string(&relaxed).unwrap()).unwrap();
    let strict_out = out.join("strict-success");
    let (code, stdout, _) = pxl(&[
        "--json",
        "--strict",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        strict_out.to_str().unwrap(),
        "--qa-config",
        config.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(strict_out.join("explosion.build.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["qa"]["config"]["strict"], true);
    let _ = fs::remove_dir_all(&out);
}
