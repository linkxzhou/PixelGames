use std::fs;

use serde_json::{json, Value};

use crate::support::{build_to, json_line, pxl, repo_root};

use super::helpers::{example, import_slime, s, temp_dir};

#[test]
fn import_gen_render_produces_a_32_pixel_canvas() {
    let dir = temp_dir("basic");
    let spec = import_slime(&dir);
    let value: Value = serde_json::from_str(&fs::read_to_string(&spec).unwrap()).unwrap();
    assert_eq!(value["cell"], json!([2, 2]));
    assert_eq!(value["anchor"], "bottom-center");
    assert_eq!(value["legend_ref"], "shared/p16.legend.json");

    let doc = dir.join("slime.pxl.json");
    let (code, stdout, stderr) =
        pxl(&["--json", "gen", "ascii", "--spec", s(&spec), "-o", s(&doc)]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let gen = json_line(&stdout);
    assert_eq!(gen["w"], 32);
    assert_eq!(gen["h"], 32);
    let frames = gen["frames"].as_array().unwrap();
    assert!(frames.iter().any(|f| f == "base"));
    assert!(frames.iter().any(|f| f == "blink"));

    let png = dir.join("slime.png");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "render",
        s(&doc),
        "--frame",
        "base",
        "--scale",
        "1",
        "-o",
        s(&png),
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let bytes = fs::read(&png).unwrap();
    let (w, h, _) = pxl_export::decode_png_rgba(&bytes).unwrap();
    assert_eq!((w, h), (32, 32));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn one_step_render_matches_import_then_gen() {
    let dir = temp_dir("onestep");
    let one = dir.join("one.png");
    let legend = example("shared/p16.legend.json");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "ascii",
        "render",
        s(&example("slime/slime.ascii.txt")),
        "--size",
        "32",
        "--cell",
        "fit",
        "--anchor",
        "bottom-center",
        "--palette",
        "p16",
        "--legend",
        s(&legend),
        "--scale",
        "4",
        "-o",
        s(&one),
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let one_hash = json_line(&stdout)["hash"].as_str().unwrap().to_string();

    let spec = import_slime(&dir);
    let doc = dir.join("slime.pxl.json");
    let (code, stdout, _) = pxl(&["--json", "gen", "ascii", "--spec", s(&spec), "-o", s(&doc)]);
    assert_eq!(code, 0, "{stdout}");
    let two = dir.join("two.png");
    let (code, stdout, _) = pxl(&[
        "--json",
        "render",
        s(&doc),
        "--frame",
        "base",
        "--scale",
        "4",
        "-o",
        s(&two),
    ]);
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(one_hash, json_line(&stdout)["hash"]);
    assert_eq!(fs::read(&one).unwrap(), fs::read(&two).unwrap());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn strict_mode_rejects_unconfirmed_auto_colours() {
    let dir = temp_dir("strict");
    let spec = dir.join("gem.ascii.json");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "ascii",
        "import",
        s(&example("gem/gem.ascii.txt")),
        "--charset",
        "printable",
        "--palette",
        "p16",
        "-o",
        s(&spec),
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let value: Value = serde_json::from_str(&fs::read_to_string(&spec).unwrap()).unwrap();
    assert!(value["legend"]["#"]["auto"].as_bool().unwrap_or(false));

    let doc = dir.join("gem.pxl.json");
    let (code, stdout, _) = pxl(&[
        "--json",
        "--strict",
        "gen",
        "ascii",
        "--spec",
        s(&spec),
        "-o",
        s(&doc),
    ]);
    assert_eq!(code, 4, "{stdout}");
    assert!(stdout.contains("ascii.auto_legend"), "{stdout}");

    let (code, stdout, _) = pxl(&[
        "--json",
        "gen",
        "ascii",
        "--spec",
        s(&spec),
        "-o",
        s(&doc),
        "--overwrite",
    ]);
    assert_eq!(code, 0, "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn digits_mode_reports_the_bad_character_location() {
    let dir = temp_dir("badchar");
    let txt = dir.join("bad.ascii.txt");
    fs::write(&txt, "01x\n").unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "ascii",
        "import",
        s(&txt),
        "-o",
        s(&dir.join("bad.ascii.json")),
    ]);
    assert_eq!(code, 3, "{stdout}");
    assert!(stdout.contains("ascii.bad_char"), "{stdout}");
    assert!(stdout.contains(":1:3"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn only_32_or_64_canvases_are_accepted() {
    let dir = temp_dir("size");
    let (code, stdout, _) = pxl(&[
        "--json",
        "ascii",
        "import",
        s(&example("slime/slime.ascii.txt")),
        "--size",
        "16",
        "-o",
        s(&dir.join("x.ascii.json")),
    ]);
    assert_eq!(code, 3, "{stdout}");
    assert!(stdout.contains("ascii.bad_size"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_text_grid_cannot_be_rendered_with_an_animation() {
    let dir = temp_dir("animtxt");
    let (code, stdout, _) = pxl(&[
        "--json",
        "ascii",
        "render",
        s(&example("slime/slime.ascii.txt")),
        "--palette",
        "p16",
        "--anim",
        "idle",
        "-o",
        s(&dir.join("x.gif")),
    ]);
    assert_eq!(code, 4, "{stdout}");
    assert!(stdout.contains("ascii.anim_source"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn spec_anims_and_the_delta_patch_reach_the_document() {
    let dir = temp_dir("specanims");
    let doc = dir.join("tank.pxl.json");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "gen",
        "ascii",
        "--spec",
        s(&example("tank_blink/tank_blink.ascii.json")),
        "-o",
        s(&doc),
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let value = json_line(&stdout);
    assert_eq!(value["w"], 64);
    let frames = value["frames"].as_array().unwrap();
    assert!(frames.iter().any(|f| f == "hop_0" || f == "idle_0"));
    assert!(frames.iter().any(|f| f == "blink"));

    let text = fs::read_to_string(&doc).unwrap();
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["meta"]["ascii"], "tank_blink.ascii.json");
    assert!(parsed["anims"]["blink"].is_object());
    let base = &parsed["frames"]["base"];
    let blink = &parsed["frames"]["blink"];
    assert_ne!(base, blink, "the patch changed the eye cells");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn recipe_can_use_an_ascii_base() {
    let out = build_to(&example("slime/slime.recipe.json"), "ascii");
    assert!(out.join("slime.pxl.json").is_file());
    assert!(out.join("slime.sheet.png").is_file());
    assert!(out.join("slime.preview.png").is_file());
    assert!(out.join("slime.build.json").is_file());
    let text = fs::read_to_string(out.join("slime.pxl.json")).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["meta"]["ascii"], "slime.ascii.json");
    assert_eq!(doc["meta"]["kind"], "sprite");
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn ascii_is_a_generator_and_serves_schemas() {
    let (code, stdout, _) = pxl(&["--json", "list", "generators"]);
    assert_eq!(code, 0);
    let value = json_line(&stdout);
    assert!(value["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i == "ascii"));

    for kind in ["ascii", "legend"] {
        let (code, stdout, _) = pxl(&["--json", "schema", kind]);
        assert_eq!(code, 0, "schema {kind}: {stdout}");
        let schema = json_line(&stdout);
        assert!(schema.get("ok").is_none());
        assert!(schema["properties"].is_object(), "{kind} schema: {schema}");
    }
}

#[test]
fn a_recipe_and_a_spec_may_not_both_declare_anims() {
    let dir = temp_dir("conflict");
    let recipe = dir.join("conflict.recipe.json");
    fs::write(
        &recipe,
        serde_json::to_string(&json!({
            "v": 1,
            "asset": "slime",
            "palette": "p16",
            "sprites": [{
                "id": "slime",
                "base": { "gen": "ascii", "spec": "slime/slime.ascii.json" },
                "anims": [{ "name": "idle", "frames": [[{ "op": "noop" }]] }]
            }],
            "sheet": { "file": "conflict.sheet.png" }
        }))
        .unwrap(),
    )
    .unwrap();
    fs::create_dir_all(dir.join("slime")).unwrap();
    fs::copy(
        example("slime/slime.ascii.json"),
        dir.join("slime/slime.ascii.json"),
    )
    .unwrap();
    fs::create_dir_all(dir.join("shared")).unwrap();
    fs::copy(
        example("shared/p16.legend.json"),
        dir.join("shared/p16.legend.json"),
    )
    .unwrap();
    let (code, stdout, _) = pxl(&["--json", "build", s(&recipe), "-o", s(&dir.join("out"))]);
    assert_eq!(code, 3, "{stdout}");
    assert!(stdout.contains("ascii.recipe_anims"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

