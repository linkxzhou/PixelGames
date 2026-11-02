//! `scene compile` — module trees expand deterministically into
//! compiled-scene/v1; collision and decoration stay separate; 47-mask gaps
//! fail loudly; budgets fail closed; unapproved libraries need an opt-in.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pxl_contract::prototype::ReviewState;
use pxl_contract::tileset::{
    CollisionCell, Decor, DecorLayer, Module, ModuleCell, TileCell, TileDef, Tileset,
    TerrainLayer, MODULE_CONTRACT, TILESET_CONTRACT, MASK_E, MASK_N, MASK_NE, MASK_NW, MASK_S,
    MASK_SE, MASK_SW, MASK_W,
};
use serde_json::{json, Value};

use crate::support::{json_line, pxl};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pxl-scene-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A tileset that covers every mask exactly with one fill tile.
fn grass_tileset() -> Tileset {
    Tileset {
        contract: TILESET_CONTRACT.into(),
        id: "terrain_grass".into(),
        version: 1,
        palette_family: "environment".into(),
        tile_size: 16,
        tiles: vec![
            TileDef {
                id: "grass_fill".into(),
                resource_id: "res-env-grass-fill".into(),
                exact_masks: vec![255],
                approximate_masks: vec![],
                variants: vec![],
            },
            // The four corner cells of a 2x2 grid: one orthogonal edge pair
            // plus the diagonal corner, e.g. E+S+SE for the top-left cell.
            TileDef {
                id: "grass_c_ne".into(),
                resource_id: "res-env-grass-c-ne".into(),
                exact_masks: vec![MASK_E | MASK_S | MASK_SE],
                approximate_masks: vec![],
                variants: vec![],
            },
            TileDef {
                id: "grass_c_sw".into(),
                resource_id: "res-env-grass-c-sw".into(),
                exact_masks: vec![MASK_N | MASK_W | MASK_NW],
                approximate_masks: vec![],
                variants: vec![],
            },
            TileDef {
                id: "grass_c_se".into(),
                resource_id: "res-env-grass-c-se".into(),
                exact_masks: vec![MASK_N | MASK_E | MASK_NE],
                approximate_masks: vec![],
                variants: vec![],
            },
            TileDef {
                id: "grass_c_nw".into(),
                resource_id: "res-env-grass-c-nw".into(),
                exact_masks: vec![MASK_S | MASK_W | MASK_SW],
                approximate_masks: vec![],
                variants: vec![],
            },
        ],
        license: "original".into(),
        review: ReviewState::Approved,
    }
}

fn plaza_module() -> Module {
    Module {
        contract: MODULE_CONTRACT.into(),
        id: "landmark_plaza".into(),
        version: 1,
        size: [2, 2],
        terrain: vec![TerrainLayer {
            tileset: "terrain_grass".into(),
            cells: vec![[0, 0], [1, 0], [0, 1], [1, 1]],
        }],
        tiles: vec![],
        modules: vec![],
        decor: vec![Decor {
            id: "fountain".into(),
            at: [8, 8],
            resource_id: Some("res-env-fountain".into()),
            scene_kit: None,
            layer: DecorLayer::Front,
        }],
        collision: vec![CollisionCell { at: [1, 1], kind: "solid".into() }],
        license: "original".into(),
        review: ReviewState::Approved,
    }
}

fn library(tag: &str) -> PathBuf {
    let root = temp_dir(tag);
    write(
        &root.join("tileset/terrain_grass.json"),
        &serde_json::to_string_pretty(&grass_tileset()).unwrap(),
    );
    write(
        &root.join("module/landmark_plaza.json"),
        &serde_json::to_string_pretty(&plaza_module()).unwrap(),
    );
    root
}

fn compile(root: &Path, module: &str, scene: &str, out: &Path, seed: u64) -> (i32, Value) {
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "scene",
        "compile",
        "--library",
        root.to_str().unwrap(),
        "--module",
        module,
        "--scene",
        scene,
        "--seed",
        &seed.to_string(),
        "-o",
        out.to_str().unwrap(),
        "--overwrite",
    ]);
    (code, json_line(&stdout).with_stderr(&stderr))
}

trait WithStderr {
    fn with_stderr(self, stderr: &str) -> Value;
}

impl WithStderr for Value {
    fn with_stderr(self, _stderr: &str) -> Value {
        self
    }
}

#[test]
fn a_module_expands_into_a_deterministic_compiled_scene() {
    let root = library("happy");
    let out = root.join("scene.json");

    let (code, body) = compile(&root, "landmark_plaza", "scene_plaza", &out, 7);
    assert_eq!(code, 0, "{body}");
    assert_eq!(body["instances"], json!(5), "4 terrain tiles + 1 decor");
    assert_eq!(body["exact_masks"], json!(4), "fill + 4 corner cells resolve exactly");
    assert_eq!(body["approximated_masks"], json!(0));
    assert_eq!(body["bounds"], json!([32, 32]));

    let scene: Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(scene["contract"], json!("compiled-scene/v1"));
    assert_eq!(scene["seed"], json!(7));
    assert_eq!(scene["collision"], json!({ "1,1": "solid" }));
    let fountain = scene["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["resource_id"].as_str().unwrap().contains("fountain"))
        .unwrap();
    assert_eq!(fountain["layer"], json!("front"));
    assert_eq!(fountain["at"], json!([8, 8]));
    assert!(scene["depends"].as_object().unwrap().contains_key("landmark_plaza"));
    assert!(scene["depends"].as_object().unwrap().contains_key("terrain_grass"));

    // Same seed: byte-identical output.
    let out2 = root.join("scene2.json");
    let (code, _) = compile(&root, "landmark_plaza", "scene_plaza", &out2, 7);
    assert_eq!(code, 0);
    assert_eq!(
        fs::read_to_string(&out).unwrap(),
        fs::read_to_string(&out2).unwrap(),
        "same inputs must give byte-identical scenes"
    );
}

#[test]
fn changing_decorations_never_touches_collision() {
    let root = library("decor");
    let out = root.join("before.json");
    let (code, body) = compile(&root, "landmark_plaza", "scene_decor", &out, 3);
    assert_eq!(code, 0, "{body}");

    // Move the fountain.
    let mut moved = plaza_module();
    moved.decor[0].at = [4, 4];
    write(
        &root.join("module/landmark_plaza.json"),
        &serde_json::to_string_pretty(&moved).unwrap(),
    );
    let out2 = root.join("after.json");
    let (code, body) = compile(&root, "landmark_plaza", "scene_decor", &out2, 3);
    assert_eq!(code, 0, "{body}");

    let before: Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
    let after: Value = serde_json::from_str(&fs::read_to_string(&out2).unwrap()).unwrap();
    assert_eq!(before["collision"], after["collision"], "decor edits must not move collision");
    assert_ne!(before["instances"], after["instances"]);
}

#[test]
fn missing_tileset_coverage_fails_instead_of_picking_a_default_tile() {
    let root = library("gap");
    let mut set = grass_tileset();
    // Keep only the fill tile: the plaza 2x2 terrain grid needs corner masks
    // the reduced tileset cannot draw, and nothing approximates them either.
    set.tiles.truncate(1);
    set.tiles[0].exact_masks = vec![255];
    set.tiles[0].approximate_masks = vec![];
    write(
        &root.join("tileset/terrain_grass.json"),
        &serde_json::to_string_pretty(&set).unwrap(),
    );

    let out = root.join("scene.json");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "scene",
        "compile",
        "--library",
        root.to_str().unwrap(),
        "--module",
        "landmark_plaza",
        "--scene",
        "scene_gap",
        "--seed",
        "1",
        "-o",
        out.to_str().unwrap(),
        "--overwrite",
    ]);
    assert_ne!(code, 0, "a coverage gap must fail, got success\n{stdout}");
    assert!(!out.exists(), "a failed compile must not write the scene");
    let text = format!("{stdout}{stderr}");
    assert!(
        text.contains("do not cover every mask"),
        "error must name the coverage gap:\n{text}"
    );
}

#[test]
fn nested_modules_expand_with_offsets_and_a_cycle_fails() {
    let root = library("nested");
    let mut wrapper = Module {
        contract: MODULE_CONTRACT.into(),
        id: "landmark_row".into(),
        version: 1,
        size: [4, 2],
        terrain: vec![],
        tiles: vec![],
        modules: vec![
            ModuleCell { module: "landmark_plaza".into(), at: [0, 0], variant: None },
            ModuleCell { module: "landmark_plaza".into(), at: [2, 0], variant: None },
        ],
        decor: vec![],
        collision: vec![],
        license: "original".into(),
        review: ReviewState::Approved,
    };
    wrapper.size = [6, 2];
    write(
        &root.join("module/landmark_row.json"),
        &serde_json::to_string_pretty(&wrapper).unwrap(),
    );

    let out = root.join("scene.json");
    let (code, body) = compile(&root, "landmark_row", "scene_row", &out, 5);
    assert_eq!(code, 0, "{body}");
    assert_eq!(body["instances"], json!(10), "2 plazas × (4 tiles + 1 decor)");
    assert_eq!(body["max_depth"], json!(2));
    let scene: Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
    let fountains: Vec<&Value> = scene["instances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["resource_id"].as_str().unwrap().contains("fountain"))
        .collect();
    assert_eq!(fountains.len(), 2);
    assert_ne!(fountains[0]["at"], fountains[1]["at"]);

    // A cycle between two modules is rejected before any expansion.
    let mut a = plaza_module();
    a.id = "loop_a".into();
    a.terrain.clear();
    a.decor.clear();
    a.collision.clear();
    a.modules = vec![ModuleCell { module: "loop_b".into(), at: [0, 0], variant: None }];
    let mut b = plaza_module();
    b.id = "loop_b".into();
    b.terrain.clear();
    b.decor.clear();
    b.collision.clear();
    b.modules = vec![ModuleCell { module: "loop_a".into(), at: [0, 0], variant: None }];
    write(&root.join("module/loop_a.json"), &serde_json::to_string_pretty(&a).unwrap());
    write(&root.join("module/loop_b.json"), &serde_json::to_string_pretty(&b).unwrap());

    let (code, stdout, stderr) = pxl(&[
        "--json",
        "scene",
        "compile",
        "--library",
        root.to_str().unwrap(),
        "--module",
        "loop_a",
        "--scene",
        "scene_loop",
        "--seed",
        "1",
        "-o",
        root.join("loop.json").to_str().unwrap(),
        "--overwrite",
    ]);
    assert_ne!(code, 0);
    let text = format!("{stdout}{stderr}");
    assert!(text.contains("cycle"), "a module cycle must be named:\n{text}");
}

#[test]
fn unapproved_libraries_need_an_explicit_opt_in() {
    let root = library("pending");
    let mut set = grass_tileset();
    set.review = ReviewState::Pending;
    write(
        &root.join("tileset/terrain_grass.json"),
        &serde_json::to_string_pretty(&set).unwrap(),
    );

    let (code, stdout, stderr) = pxl(&[
        "--json",
        "scene",
        "compile",
        "--library",
        root.to_str().unwrap(),
        "--module",
        "landmark_plaza",
        "--scene",
        "scene_pending",
        "--seed",
        "1",
        "-o",
        root.join("scene.json").to_str().unwrap(),
        "--overwrite",
    ]);
    assert_ne!(code, 0, "pending review must fail without --allow-pending");
    let text = format!("{stdout}{stderr}");
    assert!(text.contains("not approved"), "{text}");
    assert!(text.contains("terrain_grass"), "the unapproved entry must be named: {text}");

    // --allow-pending builds the draft.
    let (code, stdout, _) = pxl(&[
        "--json",
        "scene",
        "compile",
        "--library",
        root.to_str().unwrap(),
        "--module",
        "landmark_plaza",
        "--scene",
        "scene_pending",
        "--seed",
        "1",
        "-o",
        root.join("scene.json").to_str().unwrap(),
        "--overwrite",
        "--allow-pending",
    ]);
    assert_eq!(code, 0, "{stdout}");
}

#[test]
fn terrain_masks_resolve_through_exact_then_approximate_coverage() {
    let root = library("approx");
    let mut set = grass_tileset();
    set.tiles.truncate(1);
    set.tiles[0].exact_masks = vec![255];
    set.tiles[0].approximate_masks = pxl_contract::tileset::reachable_masks()
        .into_iter()
        .filter(|m| *m != 255)
        .collect();
    write(
        &root.join("tileset/terrain_grass.json"),
        &serde_json::to_string_pretty(&set).unwrap(),
    );

    let out = root.join("scene.json");
    let (code, body) = compile(&root, "landmark_plaza", "scene_approx", &out, 9);
    assert_eq!(code, 0, "{body}");
    // In a 2x2 grid no cell is fully surrounded (each lacks two directions),
    // so all four cells fall through to the approximation list and none is
    // exact. Approximations are reported, never hidden.
    assert_eq!(body["exact_masks"], json!(0));
    assert_eq!(body["approximated_masks"], json!(4), "approximations must be reported, not hidden");
    assert_eq!(body["instances"], json!(5));
}
