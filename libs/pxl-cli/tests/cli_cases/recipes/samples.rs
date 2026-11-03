use crate::support::{json_line, pxl, repo_root};
use serde_json::Value;
use std::fs;

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
            .all(|c| c["pass"] == true || c["severity"] != "error"));
        let _ = fs::remove_dir_all(&out);
    }
}


#[test]
fn battle_city_samples_pass_under_their_declared_use() {
    let root = repo_root().join("examples/battle_city");
    for (name, kind, must_see) in [
        ("brick", "tiles", "full_bleed"),
        ("steel", "tiles", "full_bleed"),
        ("water", "tiles", "full_bleed"),
        ("explosion", "effect", ".centroid_"),
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
        if name == "explosion" {
            let checks = report["checks"].as_array().unwrap();
            assert!(checks.iter().any(|c| c["pass"] == false
                && c["severity"] == "warning"
                && c["id"].as_str().unwrap().contains(".area_")));
            assert!(checks
                .iter()
                .any(|c| c["severity"] == "warning"
                    && c["id"].as_str().unwrap().contains(".centroid_")));
        }
        let _ = fs::remove_dir_all(&out);
    }
}

/// A burst keeps generic area and centroid diagnostics regardless of kind;
/// default warnings do not block publishing, while strict mode gates them.
#[test]
fn explosion_declared_as_sprite_keeps_generic_motion_warnings() {
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
    assert_eq!(code, 0, "{stdout}");
    let report = json_line(&stdout);
    let checks = report["checks"].as_array().unwrap();
    assert!(
        checks.iter().any(|c| c["pass"] == false
            && c["severity"] == "warning"
            && c["id"].as_str().unwrap().contains(".area_")),
        "{report}"
    );
    assert!(checks.iter().any(|c| c["severity"] == "warning" && c["id"].as_str().unwrap().contains(".centroid_")), "{report}");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("out/explosion.build.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["qa"]["failed"], 0);
    assert_eq!(manifest["qa"]["ok"], true);
    let (code, stdout, _) = pxl(&[
        "--json",
        "validate",
        dir.join("out/explosion.sheet.json").to_str().unwrap(),
        "--rules",
        "strict",
    ]);
    assert_eq!(code, 5, "strict mode must gate style failures: {stdout}");
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
