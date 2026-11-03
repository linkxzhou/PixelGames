use crate::support::{build_to, json_line, pxl, repo_root};
use serde_json::Value;
use std::fs;

use super::helpers::farmer_with;

/// The sprite's `post` ops redraw the canvas after a `compose`. An animation
/// that never composes starts from a direction frame that was already post
/// processed, so running `post` again would draw a second outline.
#[test]
fn parts_post_ops_run_once_for_an_animation_without_compose() {
    let (code, report, dir) = farmer_with("post-once", |r| {
        r["sprites"][0]["anims"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "name": "still", "fps": 4, "loop": true, "dirs": ["down"],
                "mode": "static", "frames": [[{"op": "noop"}]]
            }));
        // Keep the sheet to the farmer alone so the extra anim is the only change.
        r["sprites"].as_array_mut().unwrap().truncate(1);
        r["sheet"]["sources"] = serde_json::json!(["farmer"]);
        r["preview"] = serde_json::json!([]);
    });
    assert_eq!(code, 0, "{report}");
    let doc: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("out/farmer.pxl.json")).unwrap())
            .unwrap();
    assert_eq!(
        doc["frames"]["still_down_0"], doc["frames"]["down"],
        "a noop animation frame must equal its source direction frame"
    );
    assert!(doc["frames"].get("still_down_1").is_none());
    let _ = fs::remove_dir_all(&dir);
}

/// A direction derived from another direction (no compose of its own) is not
/// post processed a second time either.
#[test]
fn parts_post_ops_run_once_for_a_derived_direction() {
    let (code, report, dir) = farmer_with("post-derived", |r| {
        r["sprites"][0]["directions"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": "down_copy", "from": "down", "ops": [{"op": "noop"}]
            }));
        r["sprites"].as_array_mut().unwrap().truncate(1);
        r["sheet"]["sources"] = serde_json::json!(["farmer"]);
        r["preview"] = serde_json::json!([]);
    });
    assert_eq!(code, 0, "{report}");
    let doc: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("out/farmer.pxl.json")).unwrap())
            .unwrap();
    assert_eq!(doc["frames"]["down_copy"], doc["frames"]["down"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sheet_keeps_an_anim_entry_for_every_asset_that_reuses_a_name() {
    let out = build_to(
        &repo_root().join("examples/farmer/farmer.recipe.json"),
        "dup-anim",
    );
    let sheet: Value =
        serde_json::from_str(&fs::read_to_string(out.join("farmer.sheet.json")).unwrap()).unwrap();
    let anims = sheet["meta"]["pxl"]["anims"].as_object().unwrap();
    for key in ["walk", "farmer_blonde_walk", "farmer_raven_walk"] {
        let dirs = anims[key]["dirs"].as_object().unwrap();
        assert_eq!(dirs.len(), 4, "{key} lost directions: {dirs:?}");
    }
    // Each recolour's frames must be its own, not the first asset's.
    let first = &anims["walk"]["dirs"]["down"][0];
    let blonde = &anims["farmer_blonde_walk"]["dirs"]["down"][0];
    assert_ne!(first, blonde);
    assert!(blonde.as_str().unwrap().starts_with("farmer_blonde_"));
    let _ = fs::remove_dir_all(&out);
}

/// Two assets reuse the animation name `idle` but face different ways, so their
/// tags never collide. The second asset's animation must still be exported and
/// checked: a dynamic single frame there is a QA failure, not a silent skip.
#[test]
fn second_asset_with_a_reused_anim_name_and_other_directions_is_checked() {
    let root = repo_root();
    let src = root.join("examples/static_bullet");
    let dir = std::env::temp_dir().join(format!("pxl-multi-dir-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for name in ["static_bullet.shapes.json", "static_bullet.palette.json"] {
        fs::copy(src.join(name), dir.join(name)).unwrap();
    }
    let mut recipe: Value =
        serde_json::from_str(&fs::read_to_string(src.join("static_bullet.recipe.json")).unwrap())
            .unwrap();
    let mut first = recipe["sprites"][0].clone();
    first["anims"][0]["name"] = Value::String("idle".into());
    let mut second = first.clone();
    second["id"] = Value::String("second".into());
    second["directions"][0]["id"] = Value::String("E".into());
    second["anims"][0]["dirs"] = serde_json::json!(["E"]);
    second["anims"][0]["mode"] = Value::String("dynamic".into());
    recipe["sprites"] = serde_json::json!([first, second]);
    recipe["sheet"]["sources"] = serde_json::json!(["bullet", "second"]);
    let path = dir.join("multi.recipe.json");
    fs::write(&path, serde_json::to_string(&recipe).unwrap()).unwrap();
    let out = dir.join("out");
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        path.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 5, "the dynamic single frame must fail QA: {stdout}");
    let sheet: Value = serde_json::from_str(
        &fs::read_to_string(out.join(".rejected/static_bullet.sheet.json")).unwrap(),
    )
    .unwrap();
    let dirs = sheet["meta"]["pxl"]["anims"]["second_idle"]["dirs"]
        .as_object()
        .unwrap();
    assert_eq!(dirs.len(), 1, "second asset lost its directions: {dirs:?}");
    let report = json_line(&stdout);
    assert!(
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "anim.second_idle.E.frames" && c["pass"] == false),
        "{}",
        report["checks"]
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn qa_checks_a_recolour_whose_walk_was_broken() {
    let out = build_to(
        &repo_root().join("examples/farmer/farmer.recipe.json"),
        "dup-qa",
    );
    let sheet_path = out.join("farmer.sheet.json");
    let mut sheet: Value = serde_json::from_str(&fs::read_to_string(&sheet_path).unwrap()).unwrap();
    // Point every blonde walk frame at one pose so the animation is frozen.
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
    assert_eq!(code, 5, "a frozen recolour walk must fail QA: {stdout}");
    let report = json_line(&stdout);
    let failed: Vec<&str> = report["checks"]
        .as_array()
        .map(|c| {
            c.iter()
                .filter(|c| c["pass"] == false && c["severity"] == "error")
                .filter_map(|c| c["id"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        failed.iter().any(|id| id.contains("farmer_blonde_walk")),
        "no failing check names the broken anim: {report}"
    );
    let _ = fs::remove_dir_all(&out);
}
