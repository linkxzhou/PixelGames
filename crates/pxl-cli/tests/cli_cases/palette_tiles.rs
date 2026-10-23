use crate::support::{json_line, pxl, repo_root};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn palette_recipe(tag: &str, colors: usize, shape_color: u32) -> (i32, String) {
    let src = repo_root().join("examples/static_bullet");
    let dir = std::env::temp_dir().join(format!("pxl-pal-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut palette: Value =
        serde_json::from_str(&fs::read_to_string(src.join("static_bullet.palette.json")).unwrap())
            .unwrap();
    let list = palette["colors"].as_array_mut().unwrap();
    while list.len() < colors {
        let n = list.len() as u32;
        list.push(Value::String(format!(
            "#{:02x}{:02x}{:02x}",
            n % 250,
            40,
            90
        )));
    }
    fs::write(
        dir.join("static_bullet.palette.json"),
        serde_json::to_string(&palette).unwrap(),
    )
    .unwrap();
    let shapes = serde_json::json!({
        "w": 16, "h": 16,
        "draw": [{"rect": [4, 4, 8, 8], "c": shape_color}]
    });
    fs::write(dir.join("static_bullet.shapes.json"), shapes.to_string()).unwrap();
    let mut recipe: Value =
        serde_json::from_str(&fs::read_to_string(src.join("static_bullet.recipe.json")).unwrap())
            .unwrap();
    recipe["sprites"][0]["post"] = serde_json::json!([]);
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

/// Index 35 is the last character the text matrix can write; a palette beyond
/// 36 colours is refused up front, naming the palette, not a later pixel.
#[test]
fn text_matrix_limit_is_reported_against_the_palette() {
    let (code, stdout) = palette_recipe("c36", 36, 35);
    assert_eq!(code, 0, "36 colours with index 35 must build: {stdout}");
    let (code, stdout) = palette_recipe("c37", 37, 2);
    assert_eq!(code, 4, "a 37 colour palette must be refused: {stdout}");
    assert!(stdout.contains("36"), "{stdout}");
    assert!(stdout.contains("palette"), "{stdout}");
}

/// A palette that hides a solid colour behind alpha 0 is refused at load, so QA
/// and the PNG can never disagree about which pixels are solid.
#[test]
fn a_palette_with_a_hidden_transparent_colour_is_refused() {
    let src = repo_root().join("examples/static_bullet");
    let dir = std::env::temp_dir().join(format!("pxl-pal-hidden-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for name in ["static_bullet.shapes.json", "static_bullet.recipe.json"] {
        fs::copy(src.join(name), dir.join(name)).unwrap();
    }
    let mut palette: Value =
        serde_json::from_str(&fs::read_to_string(src.join("static_bullet.palette.json")).unwrap())
            .unwrap();
    palette["colors"][2] = Value::String("#3a4a6600".into());
    fs::write(dir.join("static_bullet.palette.json"), palette.to_string()).unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        dir.join("static_bullet.recipe.json").to_str().unwrap(),
        "-o",
        dir.join("out").to_str().unwrap(),
    ]);
    assert_eq!(code, 3, "{stdout}");
    assert!(stdout.contains("transparent"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

/// A palette with two ramps; `bone` is the one a tile is declared against.
fn tile_palette() -> Value {
    serde_json::json!({
        "v": 1,
        "colors": ["#00000000", "#141018", "#3a2a1a", "#5a4228", "#8a6a3c", "#b89a5a", "#cfcfcf", "#e8e8e8"],
        "ramps": {"ground": [2, 3, 4], "bone": [5, 6, 7]},
        "special": {"outline": 1, "flash": 7}
    })
}

fn build_tile_recipe(tag: &str, recipe: Value, shapes: Option<Value>) -> (i32, Value, PathBuf) {
    let dir = std::env::temp_dir().join(format!("pxl-tile-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("tile.palette.json"), tile_palette().to_string()).unwrap();
    if let Some(s) = shapes {
        fs::write(dir.join("fill.shapes.json"), s.to_string()).unwrap();
    }
    let path = dir.join("tile.recipe.json");
    fs::write(&path, recipe.to_string()).unwrap();
    let out = dir.join("out");
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        path.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    (code, json_line(&stdout), dir)
}

fn sheet_pxl(dir: &std::path::Path) -> Value {
    let text = fs::read_to_string(dir.join("out/tile.sheet.json")).unwrap();
    serde_json::from_str::<Value>(&text).unwrap()["meta"]["pxl"].clone()
}

/// A recipe `tiles[]` entry names the ramp its seams are judged against; that
/// choice must survive into the sheet instead of falling back to `ground`.
#[test]
fn recipe_tiles_keep_their_declared_ramp_and_kind() {
    let recipe = serde_json::json!({
        "v": 1, "asset": "tile", "gen_version": 1, "seed": 1,
        "palette": "tile.palette.json",
        "sprites": [],
        "tiles": [{"id": "floor", "kind": "dirt", "size": 16, "seed": 3, "variants": 2, "ramp": "bone"}],
        "sheet": {"sources": ["floor"], "layout": "rows-by-anim", "pad": 1, "file": "tile.sheet.png"}
    });
    let (code, report, dir) = build_tile_recipe("ramp", recipe, None);
    assert_eq!(code, 0, "{report}");
    let tiles = &sheet_pxl(&dir)["tiles"];
    assert_eq!(
        tiles["floor"]["ramp"], "bone",
        "declared ramp was lost: {tiles}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// One variant is still a tile: category must not depend on how many there are.
#[test]
fn a_single_tile_variant_is_still_a_tile() {
    let dir = std::env::temp_dir().join(format!("pxl-tile-one-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("p.palette.json"), tile_palette().to_string()).unwrap();
    for (n, name) in [(1, "one.pxl.json"), (2, "two.pxl.json")] {
        let (code, stdout, _) = pxl(&[
            "--json",
            "tile",
            "gen",
            "--kind",
            "dirt",
            "--size",
            "16",
            "--seed",
            "4",
            "--palette",
            dir.join("p.palette.json").to_str().unwrap(),
            "--ramp",
            "bone",
            "--variants",
            &n.to_string(),
            "-o",
            dir.join(name).to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{stdout}");
        let doc: Value =
            serde_json::from_str(&fs::read_to_string(dir.join(name)).unwrap()).unwrap();
        assert_eq!(
            doc["meta"]["kind"], "tiles",
            "variants={n}: {}",
            doc["meta"]
        );
        assert_eq!(
            doc["meta"]["tile_ramp"], "bone",
            "variants={n} lost the ramp"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

/// A full-bleed sprite that declares kind=tiles and has an idle animation is a
/// tile: the sheet must list it, so the bbox rule does not read it as a sprite.
#[test]
fn an_animated_tile_sprite_is_listed_as_a_tile() {
    let shapes = serde_json::json!({
        "w": 16, "h": 16,
        "draw": [{"rect": [0, 0, 16, 16], "c": 3}, {"rect": [2, 2, 4, 4], "c": 4}]
    });
    let recipe = serde_json::json!({
        "v": 1, "asset": "tile", "gen_version": 1, "seed": 1,
        "palette": "tile.palette.json",
        "sprites": [{
            "id": "water", "kind": "tiles", "tile_ramp": "ground",
            "base": {"gen": "shapes", "spec": "fill.shapes.json", "seed": 1},
            "directions": [{"id": "N", "from": "base", "ops": [{"op": "noop"}]}],
            "anims": [{"name": "idle", "fps": 2, "loop": true, "dirs": ["N"],
                       "frames": [[{"op": "noop"}], [{"op": "shift", "dx": 1, "dy": 0, "wrap": true}]]}]
        }],
        "sheet": {"sources": ["water"], "layout": "rows-by-anim", "pad": 1, "file": "tile.sheet.png"}
    });
    let (code, report, dir) = build_tile_recipe("anim", recipe, Some(shapes));
    assert_eq!(
        code, 0,
        "a full-bleed animated tile must pass default QA: {report}"
    );
    let pxl_meta = sheet_pxl(&dir);
    let listed = pxl_meta["tiles"]["water"]["frames"].as_array().unwrap();
    assert!(
        !listed.is_empty(),
        "animated tile frames are not listed: {pxl_meta}"
    );
    let _ = fs::remove_dir_all(&dir);
}
