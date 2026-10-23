use crate::support::{json_line, pxl, repo_root};
use serde_json::Value;
use std::fs;

/// A single-frame fly/idle is legal when the recipe declares `mode: static`,
/// and the declaration must survive into the sheet so a standalone validate
/// agrees with build.
#[test]
fn declared_static_single_frame_builds_and_validates() {
    let root = repo_root();
    let recipe = root.join("examples/static_bullet/static_bullet.recipe.json");
    let out = std::env::temp_dir().join(format!("pxl-static-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let built = json_line(&stdout);
    assert_eq!(built["ok"], true, "{built}");
    assert!(
        built["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "anim.fly.N.static_single_frame" && c["pass"] == true),
        "static declaration not applied: {}",
        built["checks"]
    );
    // Independent validate must reach the same verdict.
    let sheet = out.join("static_bullet.sheet.json");
    let (code, stdout, _) = pxl(&[
        "--json",
        "validate",
        sheet.to_str().unwrap(),
        "--rules",
        "default",
    ]);
    assert_eq!(code, 0, "{stdout}");
    let validated = json_line(&stdout);
    assert_eq!(validated["ok"], true, "{validated}");
    assert!(
        validated["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "anim.fly.N.static_single_frame" && c["pass"] == true),
        "sheet metadata lost the static mode: {}",
        validated["checks"]
    );
    let _ = fs::remove_dir_all(&out);
}

/// The `.pxl.json` that build writes must reach the same verdict as the sheet,
/// and a static declaration with several frames must fail on that path too.
#[test]
fn static_declaration_survives_the_document_validate_path() {
    let root = repo_root();
    let recipe = root.join("examples/static_bullet/static_bullet.recipe.json");
    let out = std::env::temp_dir().join(format!("pxl-static-doc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let doc_path = out.join("bullet.pxl.json");
    let (code, stdout, _) = pxl(&[
        "--json",
        "validate",
        doc_path.to_str().unwrap(),
        "--rules",
        "default",
    ]);
    assert_eq!(code, 0, "doc validate lost the static mode: {stdout}");
    let validated = json_line(&stdout);
    assert!(
        validated["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "anim.fly.N.static_single_frame" && c["pass"] == true),
        "{}",
        validated["checks"]
    );

    // Static with two different frames is a contradiction on the doc path too.
    let mut doc: Value = serde_json::from_str(&fs::read_to_string(&doc_path).unwrap()).unwrap();
    let base = doc["frames"]["fly_N_0"].clone();
    let mut other = base.clone();
    other[8] = Value::String("....12333331....".replace('3', "4"));
    doc["frames"]["fly_N_1"] = other;
    doc["anims"]["fly"]["frames"]["N"] = serde_json::json!(["fly_N_0", "fly_N_1"]);
    doc["anims"]["fly"]["mode"] = Value::String("static".into());
    let bad = out.join("static-two-frames.pxl.json");
    fs::write(&bad, serde_json::to_string(&doc).unwrap()).unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "validate",
        bad.to_str().unwrap(),
        "--rules",
        "default",
    ]);
    assert_ne!(code, 0, "static with two frames must fail: {stdout}");
    let _ = fs::remove_dir_all(&out);
}

/// Without the declaration the same single frame is still a failure: `mode`
/// is not a way to silence the frame-count rule.
#[test]
fn undeclared_single_frame_still_fails_to_build() {
    let root = repo_root();
    let recipe = root.join("examples/static_bullet/static_bullet.recipe.json");
    let text = fs::read_to_string(&recipe).unwrap();
    let undeclared = text.replace("\"mode\": \"static\",", "");
    assert_ne!(undeclared, text, "fixture must declare static mode");
    let dir = std::env::temp_dir().join(format!("pxl-static-bad-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("static_bullet.recipe.json");
    fs::write(&bad, &undeclared).unwrap();
    for name in ["static_bullet.shapes.json", "static_bullet.palette.json"] {
        fs::copy(
            root.join("examples/static_bullet").join(name),
            dir.join(name),
        )
        .unwrap();
    }
    let out = dir.join("out");
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        bad.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_ne!(code, 0, "undeclared single frame must fail: {stdout}");
    let built = json_line(&stdout);
    assert!(
        built["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| !c["pass"].as_bool().unwrap_or(true)
                && c["id"].as_str().unwrap().contains("frames")),
        "{}",
        built["checks"]
    );
    let _ = fs::remove_dir_all(&dir);
}

fn recipe_with_fps(tag: &str, fps: u64, extra: impl FnOnce(&mut Value)) -> (i32, String) {
    let src = repo_root().join("examples/static_bullet");
    let dir = std::env::temp_dir().join(format!("pxl-fps-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for name in ["static_bullet.shapes.json", "static_bullet.palette.json"] {
        fs::copy(src.join(name), dir.join(name)).unwrap();
    }
    let mut recipe: Value =
        serde_json::from_str(&fs::read_to_string(src.join("static_bullet.recipe.json")).unwrap())
            .unwrap();
    recipe["sprites"][0]["anims"][0]["fps"] = serde_json::json!(fps);
    extra(&mut recipe);
    let path = dir.join("r.recipe.json");
    fs::write(&path, serde_json::to_string(&recipe).unwrap()).unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        path.to_str().unwrap(),
        "-o",
        dir.join("out").to_str().unwrap(),
    ]);
    let _ = fs::remove_dir_all(&dir);
    (code, stdout)
}

/// `pxl anim` limits fps to 1..=60 and frames to 1..=16; a recipe gets the same
/// limits, so a recipe cannot describe an animation no preview can play.
#[test]
fn recipe_animation_timing_uses_the_same_limits_as_the_anim_command() {
    for bad in [0u64, 61, 1000] {
        let (code, stdout) = recipe_with_fps(&format!("bad{bad}"), bad, |_| {});
        assert_eq!(code, 3, "fps {bad} must be refused: {stdout}");
        assert!(stdout.contains("fps"), "{stdout}");
    }
    for ok in [1u64, 60] {
        let (code, stdout) = recipe_with_fps(&format!("ok{ok}"), ok, |_| {});
        assert_eq!(code, 0, "fps {ok} is valid: {stdout}");
    }
    let (code, stdout) = recipe_with_fps("frames", 8, |r| {
        let frame = serde_json::json!([{"op": "noop"}]);
        r["sprites"][0]["anims"][0]["mode"] = serde_json::json!("dynamic");
        r["sprites"][0]["anims"][0]["frames"] = serde_json::json!(vec![frame; 17]);
    });
    assert_eq!(code, 3, "17 frames must be refused: {stdout}");
    assert!(stdout.contains("17"), "{stdout}");
}
