//! ASCII layout input: text grid -> spec JSON -> document -> image.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::support::{build_to, json_line, pxl, repo_root};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pxl-ascii-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn example(name: &str) -> PathBuf {
    repo_root().join("examples/ascii").join(name)
}

fn import_slime(dir: &Path) -> PathBuf {
    let spec = dir.join("slime.ascii.json");
    fs::create_dir_all(dir.join("shared")).unwrap();
    let legend = dir.join("shared/p16.legend.json");
    fs::copy(example("shared/p16.legend.json"), &legend).unwrap();
    let text = dir.join("slime.ascii.txt");
    fs::copy(example("slime/slime.ascii.txt"), &text).unwrap();
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "ascii",
        "import",
        s(&text),
        "--size",
        "32",
        "--cell",
        "fit",
        "--anchor",
        "bottom-center",
        "--palette",
        "p16",
        "--legend",
        "shared/p16.legend.json",
        "-o",
        s(&spec),
    ]);
    assert_eq!(code, 0, "import failed\nstdout={stdout}\nstderr={stderr}");
    spec
}

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

#[test]
fn padded_rows_warn_and_strict_turns_the_warning_into_an_error() {
    let dir = temp_dir("pad");
    let txt = dir.join("ragged.ascii.txt");
    fs::write(&txt, "0120\n01\n").unwrap();
    let out = dir.join("ragged.ascii.json");
    let args = |strict: bool| {
        let mut v = vec!["--json"];
        if strict {
            v.push("--strict");
        }
        v.extend([
            "ascii",
            "import",
            s(&txt),
            "--pad",
            "--palette",
            "p16",
            "-o",
            s(&out),
        ]);
        v.into_iter().map(String::from).collect::<Vec<_>>()
    };
    let run = |strict: bool| {
        let owned = args(strict);
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        pxl(&refs)
    };
    let (code, stdout, _) = run(true);
    assert_eq!(code, 4, "{stdout}");
    assert!(!out.exists(), "a strict failure writes nothing");
    let (code, stdout, stderr) = run(false);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let warnings = json_line(&stdout)["warnings"].clone();
    assert!(
        warnings.to_string().contains("ascii.padded_row"),
        "{warnings}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_64_canvas_renders_64_pixels_and_a_gif_keeps_its_frames() {
    let dir = temp_dir("size64");
    let png = dir.join("tank.png");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "ascii",
        "render",
        s(&example("tank_blink/tank_blink.ascii.json")),
        "--scale",
        "1",
        "-o",
        s(&png),
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let (w, h, _) = pxl_export::decode_png_rgba(&fs::read(&png).unwrap()).unwrap();
    assert_eq!((w, h), (64, 64));

    let gif = dir.join("slime.gif");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "ascii",
        "render",
        s(&example("slime/slime.ascii.json")),
        "--anim",
        "idle",
        "--scale",
        "2",
        "-o",
        s(&gif),
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let value = json_line(&stdout);
    assert_eq!(value["frames_count"], 2);
    assert_eq!(
        (value["png_w"].as_u64(), value["png_h"].as_u64()),
        (Some(64), Some(64))
    );
    assert!(fs::read(&gif).unwrap().starts_with(b"GIF89a"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_direction_frame_is_reported_not_replaced_by_base() {
    let dir = temp_dir("dirs");
    let spec = dir.join("dirs.ascii.json");
    fs::write(
        &spec,
        serde_json::to_string(&json!({
            "v": 1,
            "palette": "p16",
            "frames": { "base": { "rows": ["0120", "0210"] } },
            "anims": [{
                "name": "walk", "fps": 6, "dirs": ["n"], "from": "base",
                "macro": "bob(amp=1)"
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "gen",
        "ascii",
        "--spec",
        s(&spec),
        "-o",
        s(&dir.join("dirs.pxl.json")),
    ]);
    assert_eq!(code, 4, "{stdout}");
    assert!(stdout.contains("ascii.anim_source"), "{stdout}");
    assert!(stdout.contains("base_n"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn compiling_twice_is_byte_identical() {
    let dir = temp_dir("determinism");
    let spec = example("tank_blink/tank_blink.ascii.json");
    let mut bytes = Vec::new();
    for name in ["a.pxl.json", "b.pxl.json"] {
        let out = dir.join(name);
        let (code, stdout, stderr) =
            pxl(&["--json", "gen", "ascii", "--spec", s(&spec), "-o", s(&out)]);
        assert_eq!(code, 0, "{stdout}{stderr}");
        bytes.push(fs::read(&out).unwrap());
    }
    assert_eq!(bytes[0], bytes[1]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn every_shipped_example_parses_and_matches_its_text_source() {
    for (name, stem) in [
        ("slime", "slime"),
        ("tank_blink", "tank_blink"),
        ("brick_tile", "brick_tile"),
        ("gem", "gem"),
    ] {
        let json_path = example(&format!("{name}/{stem}.ascii.json"));
        let txt_path = example(&format!("{name}/{stem}.ascii.txt"));
        let spec = pxl_format::parse_ascii_spec(&fs::read_to_string(&json_path).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {}", e.message));
        let source = spec.source.expect("examples record their source");
        let digest = pxl_core::sha256_hex(fs::read(&txt_path).unwrap().as_slice());
        assert_eq!(
            source.sha256, digest,
            "{name}: the spec is stale against its text"
        );
    }
}

#[test]
fn a_shared_legend_must_agree_with_the_spec_charset() {
    let dir = temp_dir("legendcharset");
    fs::write(
        dir.join("shared.legend.json"),
        r##"{"v":1,"charset":"printable","legend":{"#":"outline"}}"##,
    )
    .unwrap();
    let spec = dir.join("x.ascii.json");
    fs::write(
        &spec,
        r#"{"v":1,"palette":"p16","legend_ref":"shared.legend.json","frames":{"base":{"rows":["01"]}}}"#,
    )
    .unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "gen",
        "ascii",
        "--spec",
        s(&spec),
        "-o",
        s(&dir.join("x.pxl.json")),
    ]);
    assert_eq!(code, 3, "{stdout}");
    assert!(stdout.contains("ascii.legend_charset"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn refine_doubles_the_grid_into_editable_cells() {
    let dir = temp_dir("refine");
    let src = example("slime/slime.ascii.txt");
    let copy = dir.join("copy.ascii.txt");
    let corners = dir.join("corners.ascii.txt");

    let (code, stdout, stderr) = pxl(&[
        "--json",
        "ascii",
        "refine",
        s(&src),
        "--mode",
        "copy",
        "-o",
        s(&copy),
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let body = json_line(&stdout);
    assert_eq!(body["in_grid"], json!([16, 10]));
    assert_eq!(body["out_grid"], json!([32, 20]));
    assert_eq!(body["mode"], "copy");

    let (code, stdout, stderr) = pxl(&[
        "--json",
        "ascii",
        "refine",
        s(&src),
        "--mode",
        "corners",
        "-o",
        s(&corners),
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");

    let copy_rows: Vec<String> = fs::read_to_string(&copy)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with("::") && !l.is_empty())
        .map(str::to_string)
        .collect();
    let corner_rows: Vec<String> = fs::read_to_string(&corners)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with("::") && !l.is_empty())
        .map(str::to_string)
        .collect();
    // Two source blocks (base and blink), each 10 rows of 16 cells.
    assert_eq!(copy_rows.len(), 40);
    assert!(copy_rows.iter().all(|r| r.chars().count() == 32));
    assert_ne!(copy_rows, corner_rows, "corners is not a plain 2x copy");

    // Every refined cell is a 2x2 block of one source character.
    for (y, row) in copy_rows.iter().enumerate() {
        let chars: Vec<char> = row.chars().collect();
        for pair in chars.chunks(2) {
            assert_eq!(pair[0], pair[1]);
        }
        if y % 2 == 1 {
            assert_eq!(copy_rows[y], copy_rows[y - 1]);
        }
    }

    // The refined text is still legal layout input.
    let spec = dir.join("refined.ascii.json");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "ascii",
        "import",
        s(&corners),
        "--size",
        "32",
        "--cell",
        "1",
        "--palette",
        "p16",
        "-o",
        s(&spec),
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let png = dir.join("refined.png");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "ascii",
        "render",
        s(&spec),
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
fn refine_refuses_to_overwrite_and_rejects_bad_modes() {
    let dir = temp_dir("refine-guard");
    let src = example("slime/slime.ascii.txt");
    let out = dir.join("dup.ascii.txt");
    fs::write(&out, ":: base\n0\n").unwrap();
    let (code, stdout, _) = pxl(&["--json", "ascii", "refine", s(&src), "-o", s(&out)]);
    assert_eq!(code, 6, "existing output is not replaced: {stdout}");

    let (code, stdout, _) = pxl(&[
        "--json",
        "ascii",
        "refine",
        s(&src),
        "--mode",
        "sharpen",
        "-o",
        s(&dir.join("x.ascii.txt")),
    ]);
    assert_eq!(code, 2, "{stdout}");
    assert!(stdout.contains("refine mode"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_32px_samples_are_finer_than_a_plain_2x_copy() {
    let root = repo_root().join("examples/retro_sprites_32");
    for name in ["battlecity_tank", "bomberman", "castlevania_simon"] {
        let spec_path = root.join(format!("{name}.ascii.json"));
        let spec = pxl_format::parse_ascii_spec(&fs::read_to_string(&spec_path).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {}", e.message));
        assert_eq!((spec.size, spec.cell), (32, [1, 1]), "{name}");
        let text = root.join(format!("{name}.ascii.txt"));
        let digest = pxl_core::sha256_hex(fs::read(&text).unwrap().as_slice());
        assert_eq!(
            spec.source.expect("the sample records its source").sha256,
            digest,
            "{name}: the spec is stale against its text"
        );

        // A plain copy of the 16x16 source, refined once, must differ.
        let old = repo_root().join(format!("examples/retro_sprites_48/{name}.ascii.txt"));
        let dir = temp_dir(&format!("sample-{name}"));
        let copy_path = dir.join("copy32.txt");
        let (code, stdout, stderr) = pxl(&[
            "--json",
            "ascii",
            "refine",
            s(&old),
            "--mode",
            "copy",
            "-o",
            s(&copy_path),
        ]);
        assert_eq!(code, 0, "{stdout}{stderr}");
        let copy_text = fs::read_to_string(&copy_path).unwrap();
        let shipped = fs::read_to_string(&text).unwrap();
        let strip = |t: &str| -> Vec<String> {
            t.lines()
                .filter(|l| !l.starts_with("::") && !l.is_empty())
                .map(str::to_string)
                .collect()
        };
        let (a, b) = (strip(&copy_text), strip(&shipped));
        assert_eq!(a.len(), b.len(), "{name}: same grid height");
        let differing = a
            .iter()
            .zip(&b)
            .map(|(x, y)| x.chars().zip(y.chars()).filter(|(p, q)| p != q).count())
            .sum::<usize>();
        assert!(
            differing > 40,
            "{name}: only {differing} px differ from a plain 2x copy"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
