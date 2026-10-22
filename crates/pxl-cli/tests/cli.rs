//! CLI exit codes, JSON shape, and the tank demo acceptance checks.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn pxl(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_pxl"))
        .args(args)
        .output()
        .expect("pxl binary");
    let code = out.status.code().unwrap_or(1);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    (code, stdout, stderr)
}

fn json_line(stdout: &str) -> Value {
    let line = stdout.trim();
    assert!(!line.contains('\n'), "stdout must be one JSON line: {line}");
    serde_json::from_str(line).unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {line}"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn usage_error_is_json_exit_2() {
    let (code, stdout, _) = pxl(&["--json"]);
    assert_eq!(code, 2);
    let v = json_line(&stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "E_USAGE");
    assert!(v["error"]["hint"].as_str().unwrap().contains("help"));
}

#[test]
fn schema_is_raw_json_and_recipe_parses() {
    let (code, stdout, _) = pxl(&["--json", "schema", "recipe"]);
    assert_eq!(code, 0);
    let schema = json_line(&stdout);
    assert!(schema.get("ok").is_none(), "schema is not wrapped in ok");
    assert!(
        schema.get("properties").is_some()
            || schema.get("$schema").is_some()
            || schema.get("title").is_some()
    );
    let recipe_path = repo_root().join("examples/tank/tank.recipe.json");
    let recipe: pxl_format::Recipe =
        serde_json::from_str(&fs::read_to_string(recipe_path).unwrap()).unwrap();
    assert_eq!(recipe.asset, "tank");
    assert_eq!(
        recipe.sprites[0].anims[0].macro_name(),
        Some("tread_cycle(steps=4, region=auto_tread, cycle=tread, bob=1)")
    );
}

#[test]
fn bad_json_is_exit_3_and_rotate_is_exit_4_and_exists_is_exit_6() {
    let dir = std::env::temp_dir().join(format!("pxl-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.json");
    fs::write(&bad, "{").unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "op",
        bad.to_str().unwrap(),
        "--op",
        "{\"op\":\"noop\"}",
        "-o",
        dir.join("x.pxl.json").to_str().unwrap(),
    ]);
    assert_eq!(code, 3, "{stdout}");
    assert_eq!(json_line(&stdout)["error"]["code"], "E_SCHEMA");

    let spec = repo_root().join("examples/tank/tank_body.shapes.json");
    let doc = dir.join("tank.pxl.json");
    let (code, stdout, _) = pxl(&[
        "--json",
        "gen",
        "shapes",
        "--spec",
        spec.to_str().unwrap(),
        "--palette",
        "tank",
        "-o",
        doc.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let ok = json_line(&stdout);
    assert_eq!(ok["ok"], true);
    assert!(ok["hash"].as_str().unwrap().starts_with("sha256:"));

    let again = dir.join("tank.pxl.json");
    let (code, stdout, _) = pxl(&[
        "--json",
        "gen",
        "shapes",
        "--spec",
        spec.to_str().unwrap(),
        "--palette",
        "tank",
        "-o",
        again.to_str().unwrap(),
    ]);
    assert_eq!(code, 6, "{stdout}");
    assert_eq!(json_line(&stdout)["error"]["code"], "E_EXISTS");

    let (code, stdout, _) = pxl(&[
        "--json",
        "--overwrite",
        "op",
        doc.to_str().unwrap(),
        "--op",
        r#"{"op":"rotate","angle":45}"#,
        "-o",
        doc.to_str().unwrap(),
    ]);
    assert_eq!(code, 4, "{stdout}");
    let err = json_line(&stdout);
    assert_eq!(err["error"]["code"], "E_SEMANTIC");
    assert!(err["error"]["hint"].as_str().unwrap().contains("rotate90"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn list_is_one_json_line() {
    let (code, stdout, _) = pxl(&["--json", "list", "macros"]);
    assert_eq!(code, 0);
    let v = json_line(&stdout);
    assert_eq!(v["ok"], true);
    let items = v["items"].as_array().unwrap();
    assert!(items.iter().any(|i| i == "tread_cycle"));
    assert!(items.iter().any(|i| i == "idle_rumble"));
    assert!(items.iter().any(|i| i == "doll"));
    // Macros also carry argument metadata so callers do not guess signatures.
    let macros = v["macros"].as_array().expect("macros metadata present");
    let glow = macros
        .iter()
        .find(|m| m["name"] == "pulse_glow")
        .expect("pulse_glow metadata");
    let args = glow["args"].as_array().unwrap();
    assert_eq!(args.len(), 1, "{args:?}");
    assert_eq!(args[0]["name"], "ramp");
    assert!(!args.iter().any(|a| a["name"] == "steps"));
    assert!(glow["example"].as_str().unwrap().contains("ramp="));
    assert_eq!(glow["frames"], 2);
    let (code, stdout, _) = pxl(&["--json", "list", "generators"]);
    assert_eq!(code, 0);
    let gens = json_line(&stdout);
    assert!(gens["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i == "parts"));
}

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
        fs::copy(root.join("examples/static_bullet").join(name), dir.join(name)).unwrap();
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

#[test]
fn tank_build_is_deterministic_and_validates() {
    let root = repo_root();
    let recipe = root.join("examples/tank/tank.recipe.json");
    let a = std::env::temp_dir().join(format!("pxl-tank-a-{}", std::process::id()));
    let b = std::env::temp_dir().join(format!("pxl-tank-b-{}", std::process::id()));
    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        a.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let built = json_line(&stdout);
    assert_eq!(built["ok"], true);
    assert!(built["hash"].as_str().unwrap().starts_with("sha256:"));
    let (code, _, stderr) = pxl(&[
        "build",
        recipe.to_str().unwrap(),
        "-o",
        b.to_str().unwrap(),
        "--quiet",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let mut files = Vec::new();
    for entry in fs::read_dir(&a).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            files.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    files.sort();
    assert!(files.iter().any(|f| f == "tank.sheet.png"));
    assert!(files.iter().any(|f| f == "tank.sheet.json"));
    assert!(files.iter().any(|f| f == "preview.png"));
    assert!(files.iter().any(|f| f == "tank_move_E.gif"));
    for name in &files {
        let ha = sha256(&a.join(name));
        let hb = sha256(&b.join(name));
        assert_eq!(ha, hb, "{name} differs between builds");
    }

    let (code, stdout, stderr) = pxl(&[
        "--json",
        "validate",
        a.join("tank.sheet.json").to_str().unwrap(),
        "--rules",
        "tank",
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let report = json_line(&stdout);
    assert_eq!(report["ok"], true);
    assert!(report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["pass"] == true));

    let sheet: Value =
        serde_json::from_str(&fs::read_to_string(a.join("tank.sheet.json")).unwrap()).unwrap();
    let frames = sheet["frames"].as_object().unwrap();
    assert!(frames.len() >= 4 * 4 + 4 * 2 + 4);
    for frame in frames.values() {
        assert!(frame.get("frame").is_some());
        assert!(frame.get("duration").is_some());
        assert!(frame.get("spriteSourceSize").is_some());
        assert!(frame.get("sourceSize").is_some());
    }
    let tags = sheet["meta"]["frameTags"].as_array().unwrap();
    for dir in ["N", "E", "S", "W"] {
        let move_tag = tags
            .iter()
            .find(|t| t["name"] == format!("move_{dir}"))
            .unwrap();
        let idle_tag = tags
            .iter()
            .find(|t| t["name"] == format!("idle_{dir}"))
            .unwrap();
        let span = |t: &Value| t["to"].as_u64().unwrap() - t["from"].as_u64().unwrap() + 1;
        assert!(span(move_tag) >= 4, "{move_tag}");
        assert!(span(idle_tag) >= 2, "{idle_tag}");
        assert_eq!(move_tag["direction"], "forward");
    }
    let tiles = sheet["meta"]["pxl"]["tiles"]["ground"]["frames"]
        .as_array()
        .unwrap();
    assert!(tiles.len() >= 4);

    let doc: Value =
        serde_json::from_str(&fs::read_to_string(a.join("tank.pxl.json")).unwrap()).unwrap();
    for (dir, expect_x_gt, expect_y_lt) in [
        ("N", None, Some(true)),
        ("E", Some(true), None),
        ("S", None, Some(false)),
        ("W", Some(false), None),
    ] {
        let rows = doc["frames"][&format!("idle_{dir}_0")].as_array().unwrap();
        let (mx, my, bx, by, mn, bn) = centroids(rows);
        assert!(mn > 0.0, "{dir} has no muzzle");
        assert!(bn > mn);
        if let Some(gt) = expect_x_gt {
            if gt {
                assert!(mx > bx, "{dir} gun should point east");
            } else {
                assert!(mx < bx, "{dir} gun should point west");
            }
        }
        if let Some(lt) = expect_y_lt {
            if lt {
                assert!(my < by, "{dir} gun should point north");
            } else {
                assert!(my > by, "{dir} gun should point south");
            }
        }
    }

    let (code, stdout, _) = pxl(&[
        "--json",
        "validate",
        a.join("tank.pxl.json").to_str().unwrap(),
        "--rules",
        "tank",
    ]);
    assert_eq!(
        code, 5,
        "sprite-only validate should fail tile count: {stdout}"
    );
    assert_eq!(json_line(&stdout)["error"]["code"], "E_QA");

    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
}

fn centroids(rows: &[Value]) -> (f64, f64, f64, f64, f64, f64) {
    let mut mx = 0.0;
    let mut my = 0.0;
    let mut mn = 0.0;
    let mut bx = 0.0;
    let mut by = 0.0;
    let mut bn = 0.0;
    for (y, row) in rows.iter().enumerate() {
        let row = row.as_str().unwrap();
        for (x, ch) in row.chars().enumerate() {
            if ch == '.' || ch == '0' {
                continue;
            }
            bx += x as f64;
            by += y as f64;
            bn += 1.0;
            if ch == 'a' {
                mx += x as f64;
                my += y as f64;
                mn += 1.0;
            }
        }
    }
    (mx / mn, my / mn, bx / bn, by / bn, mn, bn)
}

#[test]
fn farmer_build_is_deterministic_and_validates() {
    let root = repo_root();
    let recipe = root.join("examples/farmer/farmer.recipe.json");
    let a = std::env::temp_dir().join(format!("pxl-farmer-a-{}", std::process::id()));
    let b = std::env::temp_dir().join(format!("pxl-farmer-b-{}", std::process::id()));
    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        a.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let built = json_line(&stdout);
    assert_eq!(built["ok"], true);
    let (code, _, stderr) = pxl(&[
        "build",
        recipe.to_str().unwrap(),
        "-o",
        b.to_str().unwrap(),
        "--quiet",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let mut files = Vec::new();
    for entry in fs::read_dir(&a).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            files.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    files.sort();
    for name in [
        "farmer.sheet.png",
        "farmer.sheet.json",
        "farmer.parts.json",
        "preview.png",
        "walk_down.png",
        "walk_up.png",
        "walk_left.png",
        "walk_right.png",
        "idle_down.png",
        "hoe_down.png",
        "blonde_walk_down.png",
        "raven_walk_down.png",
    ] {
        assert!(files.iter().any(|f| f == name), "missing {name}");
    }
    for name in &files {
        assert_eq!(sha256(&a.join(name)), sha256(&b.join(name)), "{name}");
    }

    let (code, stdout, stderr) = pxl(&[
        "--json",
        "validate",
        a.join("farmer.sheet.json").to_str().unwrap(),
        "--rules",
        "character",
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    assert!(json_line(&stdout)["checks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["pass"] == true));

    let sheet: Value =
        serde_json::from_str(&fs::read_to_string(a.join("farmer.sheet.json")).unwrap()).unwrap();
    let tags = sheet["meta"]["frameTags"].as_array().unwrap();
    let span = |name: &str| {
        let tag = tags.iter().find(|t| t["name"] == name).unwrap();
        tag["to"].as_u64().unwrap() - tag["from"].as_u64().unwrap() + 1
    };
    for dir in ["down", "up", "left", "right"] {
        assert!(span(&format!("walk_{dir}")) >= 4);
        assert!(span(&format!("idle_{dir}")) >= 2);
    }
    assert!(span("hoe_down") >= 4);
    assert!(span("farmer_blonde_walk_down") >= 4);
    assert!(span("farmer_raven_walk_right") >= 4);
    assert!(tags
        .iter()
        .all(|t| t["name"] != "walk_down" || t["from"] == 0));
    assert_eq!(sheet["meta"]["pxl"]["anims"]["hoe"]["loop"], false);
    assert_eq!(
        sheet["meta"]["pxl"]["anims"]["walk"]["dirs"]
            .as_object()
            .unwrap()
            .len(),
        4
    );

    let doc: Value =
        serde_json::from_str(&fs::read_to_string(a.join("farmer.pxl.json")).unwrap()).unwrap();
    assert_eq!(doc["w"], 16);
    assert_eq!(doc["h"], 32);
    assert_eq!(doc["meta"]["parts"], "farmer.parts.json");
    let left = doc["frames"]["idle_left_0"].as_array().unwrap();
    let right = doc["frames"]["idle_right_0"].as_array().unwrap();
    for (l, r) in left.iter().zip(right.iter()) {
        let l = l.as_str().unwrap();
        let rev: String = l.chars().rev().collect();
        assert_eq!(r.as_str().unwrap(), rev);
    }
    let walk_l = doc["frames"]["walk_left_0"].as_array().unwrap();
    let walk_r = doc["frames"]["walk_right_0"].as_array().unwrap();
    for (l, r) in walk_l.iter().zip(walk_r.iter()) {
        let rev: String = l.as_str().unwrap().chars().rev().collect();
        assert_eq!(r.as_str().unwrap(), rev);
    }

    let blonde: Value =
        serde_json::from_str(&fs::read_to_string(a.join("farmer_blonde.pxl.json")).unwrap())
            .unwrap();
    let raven: Value =
        serde_json::from_str(&fs::read_to_string(a.join("farmer_raven.pxl.json")).unwrap())
            .unwrap();
    let flat = |doc: &Value, frame: &str| {
        doc["frames"][frame]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<String>()
    };
    let base = flat(&doc, "walk_down_0");
    let gold = flat(&blonde, "walk_down_0");
    let dark = flat(&raven, "walk_down_0");
    assert!(base.contains('6'), "base hair");
    assert!(base.contains('9'), "base shirt");
    assert!(gold.contains('k'), "blonde hair ramp");
    assert!(gold.contains('q'), "red shirt ramp");
    assert!(!gold.contains('6') && !gold.contains('9'));
    assert!(dark.contains('n'), "raven hair ramp");
    assert!(dark.contains('t'), "green shirt ramp");
    assert!(!dark.contains('6') && !dark.contains('9'));

    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
}

#[test]
fn reference_recipes_build_and_validate() {
    let root = repo_root().join("examples/reference");
    let names = [
        "celeste-run",
        "celeste-spikes",
        "terra-tiles",
        "crop-stages",
        "orchard-tree",
        "item-chest",
        "soul-bullet",
        "ui-dialog",
        "meadow-slime",
        "shaded-crate",
    ];
    for name in names {
        let out = std::env::temp_dir().join(format!("pxl-ref-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&out);
        let recipe = root.join(name).join("recipe.json");
        let (code, stdout, stderr) = pxl(&[
            "--json",
            "build",
            recipe.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{name}\nstdout={stdout}\nstderr={stderr}");
        let report = json_line(&stdout);
        assert_eq!(report["ok"], true, "{name}");
        assert!(report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["pass"] == true));
        let _ = fs::remove_dir_all(&out);
    }
}

fn build_to(recipe: &std::path::Path, tag: &str) -> PathBuf {
    let out = std::env::temp_dir().join(format!("pxl-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{tag}\nstdout={stdout}\nstderr={stderr}");
    out
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
                .filter(|c| c["pass"] == false)
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

/// Read the NETSCAPE loop block: present means the GIF repeats.
fn gif_repeats(bytes: &[u8]) -> bool {
    bytes.windows(11).any(|w| w == b"NETSCAPE2.0")
}

#[test]
fn one_shot_animation_previews_as_a_non_looping_gif() {
    let dir = std::env::temp_dir().join(format!("pxl-gifloop-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let out = build_to(
        &repo_root().join("examples/farmer/farmer.recipe.json"),
        "gifloop",
    );
    let doc = out.join("farmer.pxl.json");
    let looping = out.join("walk.gif");
    let once = out.join("hoe.gif");
    let (code, stdout, _) = pxl(&[
        "--json",
        "render",
        doc.to_str().unwrap(),
        "--anim",
        "walk",
        "--dir",
        "down",
        "-o",
        looping.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let (code, stdout, _) = pxl(&[
        "--json",
        "render",
        doc.to_str().unwrap(),
        "--anim",
        "hoe",
        "--dir",
        "down",
        "-o",
        once.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    assert!(gif_repeats(&fs::read(&looping).unwrap()), "walk loops");
    assert!(!gif_repeats(&fs::read(&once).unwrap()), "hoe plays once");
    let _ = fs::remove_dir_all(&out);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn multi_frame_png_reports_the_real_strip_size() {
    let out = build_to(
        &repo_root().join("examples/farmer/farmer.recipe.json"),
        "strip",
    );
    let doc = out.join("farmer.pxl.json");
    let strip = out.join("strip.png");
    let (code, stdout, _) = pxl(&[
        "--json",
        "render",
        doc.to_str().unwrap(),
        "--anim",
        "walk",
        "--dir",
        "down",
        "--scale",
        "2",
        "-o",
        strip.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let v = json_line(&stdout);
    let bytes = fs::read(&strip).unwrap();
    let (w, h, _) = pxl_export::decode_png_rgba(&bytes).unwrap();
    assert_eq!(v["png_w"].as_u64().unwrap(), w as u64);
    assert_eq!(v["png_h"].as_u64().unwrap(), h as u64);
    assert!(
        w > 16 * 2,
        "a strip of several frames is wider than one frame"
    );
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn misspelled_macro_argument_fails_the_build() {
    let dir = std::env::temp_dir().join(format!("pxl-typo-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let src = repo_root().join("examples/reference/meadow-slime");
    fs::create_dir_all(&dir).unwrap();
    for f in ["palette.json", "slime.shapes.json"] {
        fs::copy(src.join(f), dir.join(f)).unwrap();
    }
    // The meadow recipe's blink call is the fixture. A renamed argument must
    // fail the build instead of being dropped.
    let recipe = fs::read_to_string(src.join("recipe.json"))
        .unwrap()
        .replace(
            "blink(x=2, y=11, w=3, h=2, lid=5)",
            "blink(step=2, y=11, w=3, h=2, lid=5)",
        );
    fs::write(dir.join("recipe.json"), recipe).unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        dir.join("recipe.json").to_str().unwrap(),
        "-o",
        dir.join("out").to_str().unwrap(),
    ]);
    assert_eq!(code, 3, "{stdout}");
    assert!(json_line(&stdout)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("step"));
    let _ = fs::remove_dir_all(&dir);
}

fn sha256(path: &std::path::Path) -> String {
    let bytes = fs::read(path).unwrap();
    pxl_core::sha256_hex(&bytes)
}
