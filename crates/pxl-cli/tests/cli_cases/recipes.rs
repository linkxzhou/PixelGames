use crate::support::{json_line, pxl, repo_root};
use serde_json::Value;
use std::fs;

/// §4-6 of plan/miniagent-integration.md: a release build of the tank recipe
/// should finish well under 200ms. The test binary may be a debug build, so we
/// only assert when PXL_PERF_ASSERT=1 (CI runs it against --release).
#[test]
fn tank_recipe_build_is_fast() {
    if std::env::var("PXL_PERF_ASSERT").ok().as_deref() != Some("1") {
        return;
    }
    let root = repo_root();
    let recipe = root.join("examples/tank/tank.recipe.json");
    let out = std::env::temp_dir().join(format!("pxl-tank-perf-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    let started = std::time::Instant::now();
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    let elapsed = started.elapsed();
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    assert!(
        elapsed < std::time::Duration::from_millis(200),
        "tank build took {elapsed:?}, budget is 200ms"
    );
    let _ = fs::remove_dir_all(&out);
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

fn sha256(path: &std::path::Path) -> String {
    let bytes = fs::read(path).unwrap();
    pxl_core::sha256_hex(&bytes)
}

/// The V2-E battle_city samples build with default QA under their declared
/// use (tiles full-bleed, explosion as an effect, HUD as ui), and standalone
/// sheet and doc validation reach the same verdict as the build.
#[test]
fn battle_city_samples_pass_under_their_declared_use() {
    let root = repo_root().join("examples/battle_city");
    for (name, kind, must_see) in [
        ("brick", "tiles", "full_bleed"),
        ("steel", "tiles", "full_bleed"),
        ("water", "tiles", "full_bleed"),
        ("explosion", "effect", "effect_anchor"),
        ("hud_stage_label", "ui", ".visible"),
        ("player_tank", "sprite", ".bbox"),
    ] {
        let out = crate::support::build_to(&root.join(format!("{name}.recipe.json")), name);
        let sheet = out.join(format!("{name}.sheet.json"));
        let (code, report) = crate::support::validate_sheet(&sheet);
        assert_eq!(code, 0, "{name}: {report}");
        assert!(
            report["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"].as_str().unwrap().contains(must_see)),
            "{name}: no {must_see} check: {report}"
        );
        let meta: Value = serde_json::from_str(&fs::read_to_string(&sheet).unwrap()).unwrap();
        assert_eq!(meta["meta"]["pxl"]["assets"][name]["kind"], kind, "{name}");
        let doc = out.join(format!("{name}.pxl.json"));
        let (code, stdout, _) = pxl(&[
            "--json",
            "validate",
            doc.to_str().unwrap(),
            "--rules",
            "default",
        ]);
        assert_eq!(code, 0, "{name} doc: {stdout}");
        let _ = fs::remove_dir_all(&out);
    }
}

/// Declaring a burst as `sprite` keeps the motion cap; the effect profile is
/// opt-in through the frozen kind, never a silent relaxation.
#[test]
fn explosion_declared_as_sprite_still_hits_the_motion_cap() {
    let src = repo_root().join("examples/battle_city");
    let dir = std::env::temp_dir().join(format!("pxl-boom-sprite-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for name in ["palette.json", "explosion.shapes.json"] {
        fs::copy(src.join(name), dir.join(name)).unwrap();
    }
    let text = fs::read_to_string(src.join("explosion.recipe.json"))
        .unwrap()
        .replace("\"kind\": \"effect\"", "\"kind\": \"sprite\"");
    let recipe = dir.join("explosion.recipe.json");
    fs::write(&recipe, text).unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        dir.join("out").to_str().unwrap(),
    ]);
    assert_eq!(code, 5, "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

/// Unknown uses are a schema error naming the field, not a silent sprite.
#[test]
fn unknown_kind_is_rejected() {
    let src = repo_root().join("examples/battle_city");
    let dir = std::env::temp_dir().join(format!("pxl-kind-bad-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for name in ["palette.json", "hud_stage_label.shapes.json"] {
        fs::copy(src.join(name), dir.join(name)).unwrap();
    }
    let text = fs::read_to_string(src.join("hud_stage_label.recipe.json"))
        .unwrap()
        .replace("\"kind\": \"ui\"", "\"kind\": \"hud\"");
    let recipe = dir.join("hud.recipe.json");
    fs::write(&recipe, text).unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        dir.join("out").to_str().unwrap(),
    ]);
    assert_eq!(code, 3, "{stdout}");
    assert!(stdout.contains("/kind"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}
