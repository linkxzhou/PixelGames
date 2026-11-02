//! `prototype` — check a library, expand a frozen instance, write the lock.
//! Pixels must come out identically on every run, unapproved entries must be
//! refused unless the caller opts in, and a tampered source must be caught.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pxl_contract::prototype::{
    sha256_digest, AssetInstance, Connector, DrawLayer, Footprint, LayerSplit, ParamRange,
    ParamValue, Prototype, PrototypeKind, PrototypeSource, ReviewState, SizeTier, SurfaceRecipe,
    VariantRule, ViewKind, ASSET_INSTANCE_CONTRACT, PROTOTYPE_CONTRACT, SURFACE_RECIPE_CONTRACT,
};
use serde_json::{json, Value};

use crate::support::{json_line, pxl};

const BASE: &str = r#"{
  "w": 6, "h": 4,
  "draw": [{ "pixels": ["..22..", ".2222.", "333333", "3.44.3"], "x": 0, "y": 0 }],
  "regions": {
    "roof": { "pixels": ["..11..", ".1111.", "000000", "000000"] },
    "wall": { "pixels": ["......", "......", "111111", "1.11.1"] }
  }
}
"#;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pxl-proto-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn prototype(review: ReviewState) -> Prototype {
    Prototype {
        contract: PROTOTYPE_CONTRACT.into(),
        id: "tiny_house".into(),
        version: 1,
        kind: PrototypeKind::Building,
        view: ViewKind::TopDown,
        size_tiers: vec![SizeTier { id: "s".into(), size: [6, 4] }],
        params: BTreeMap::from([("has_chimney".into(), ParamRange::Flag { default: false })]),
        regions: vec!["roof".into(), "wall".into()],
        connectors: vec![Connector { id: "door".into(), at: [3, 3], dir: "S".into(), width: 2, accepts: vec!["path".into()] }],
        footprint: Footprint { visual: [0, 0, 6, 4], sort_y: 3, collision: vec![[0, 2, 6, 2]], interact: None },
        layers: vec![LayerSplit { id: "roof_front".into(), regions: vec!["roof".into()], layer: DrawLayer::Front }],
        variants: vec![VariantRule {
            param: "has_chimney".into(),
            equals: ParamValue::Flag(true),
            region: "roof".into(),
            rows: vec![".9".into()],
        }],
        source: PrototypeSource {
            kind: "shapes".into(),
            path: "tiny_house.shapes.json".into(),
            digest: sha256_digest(BASE.as_bytes()),
        },
        requires: vec!["roof_tile".into()],
        license: "original".into(),
        review,
    }
}

fn surface(review: ReviewState) -> SurfaceRecipe {
    SurfaceRecipe {
        contract: SURFACE_RECIPE_CONTRACT.into(),
        id: "roof_tile".into(),
        version: 1,
        material_id: "tile".into(),
        applies_to: vec!["roof".into()],
        recipe: "recolour".into(),
        params: BTreeMap::new(),
        scale_px: 2,
        license: "original".into(),
        review,
    }
}

fn library(tag: &str, review: ReviewState) -> PathBuf {
    let root = temp_dir(tag);
    write(&root.join("prototype/tiny_house.json"), &serde_json::to_string_pretty(&prototype(review)).unwrap());
    write(&root.join("surface/roof_tile.json"), &serde_json::to_string_pretty(&surface(review)).unwrap());
    write(&root.join("base/tiny_house.shapes.json"), BASE);
    write(&root.join("recolour/roof_tile.json"), r#"{ "2": "5" }"#);
    root
}

fn instance_file(root: &Path, review: ReviewState, chimney: bool, with_surface: bool) -> PathBuf {
    // The reference digest covers the whole record, review state included.
    let proto = prototype(review);
    let mut params = BTreeMap::new();
    if chimney {
        params.insert("has_chimney".to_string(), ParamValue::Flag(true));
    }
    let mut surfaces = BTreeMap::new();
    if with_surface {
        surfaces.insert("roof".to_string(), "roof_tile".to_string());
    }
    let instance = AssetInstance {
        contract: ASSET_INSTANCE_CONTRACT.into(),
        instance_id: "farm_house".into(),
        prototype: proto.reference(),
        size_tier: "s".into(),
        params,
        surfaces,
        seed: 1,
        patches: Vec::new(),
    };
    let path = root.join("instance.json");
    write(&path, &serde_json::to_string_pretty(&instance).unwrap());
    path
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn pixel_rows(shapes_path: &Path) -> Vec<String> {
    let value: Value = serde_json::from_str(&fs::read_to_string(shapes_path).unwrap()).unwrap();
    value["draw"][0]["pixels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn check_accepts_a_consistent_library_and_counts_pending_entries() {
    let root = library("check-ok", ReviewState::Pending);
    let (code, stdout, stderr) = pxl(&["--json", "prototype", "check", "--library", s(&root)]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let body = json_line(&stdout);
    assert_eq!(body["prototypes"], 1);
    assert_eq!(body["surfaces"], 1);
    assert_eq!(body["not_approved"], 2);
    assert_eq!(body["problems"], json!([]));
}

#[test]
fn check_catches_a_tampered_source_and_a_missing_dependency() {
    let root = library("check-bad", ReviewState::Approved);
    write(&root.join("base/tiny_house.shapes.json"), &BASE.replace("333333", "333334"));
    fs::remove_file(root.join("surface/roof_tile.json")).unwrap();
    let (code, stdout, _) = pxl(&["--json", "prototype", "check", "--library", s(&root)]);
    assert_ne!(code, 0);
    let text = format!("{stdout}");
    assert!(text.contains("source digest does not match"), "{text}");
    assert!(text.contains("requires missing entry 'roof_tile'"), "{text}");
}

#[test]
fn expand_refuses_unapproved_entries_unless_opted_in() {
    let root = library("expand-pending", ReviewState::Pending);
    let instance = instance_file(&root, ReviewState::Pending, false, true);
    let out = root.join("out.shapes.json");

    let (code, stdout, stderr) = pxl(&["--json", "prototype", "expand", "--library", s(&root), s(&instance), "-o", s(&out)]);
    assert_ne!(code, 0, "stdout={stdout}");
    assert!(format!("{stdout}{stderr}").contains("not approved"), "{stdout}{stderr}");
    assert!(!out.exists(), "nothing may be written on refusal");

    let (code, stdout, stderr) = pxl(&["--json", "prototype", "expand", "--library", s(&root), s(&instance), "-o", s(&out), "--allow-pending"]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    assert_eq!(pixel_rows(&out), vec!["..55..", ".5555.", "333333", "3.44.3"]);
}

#[test]
fn expand_applies_the_selected_variant_and_is_byte_identical_across_runs() {
    let root = library("expand-det", ReviewState::Approved);
    let instance = instance_file(&root, ReviewState::Approved, true, false);
    let first = root.join("a.shapes.json");
    let second = root.join("b.shapes.json");

    for out in [&first, &second] {
        let (code, stdout, stderr) = pxl(&["--json", "prototype", "expand", "--library", s(&root), s(&instance), "-o", s(out)]);
        assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
        let body = json_line(&stdout);
        assert_eq!(body["applied_variants"].as_array().unwrap().len(), 1);
    }
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    assert_eq!(pixel_rows(&first), vec!["..92..", ".2222.", "333333", "3.44.3"]);
}

#[test]
fn expand_rejects_an_instance_pinned_to_a_different_prototype_revision() {
    let root = library("expand-stale", ReviewState::Approved);
    // Pinned to the Pending record while the library now holds the Approved one.
    let instance = instance_file(&root, ReviewState::Pending, false, false);
    let out = root.join("out.shapes.json");
    let (code, stdout, stderr) = pxl(&["--json", "prototype", "expand", "--library", s(&root), s(&instance), "-o", s(&out)]);
    assert_ne!(code, 0, "stdout={stdout}");
    assert!(format!("{stdout}{stderr}").contains("digest does not match"), "{stdout}{stderr}");
    assert!(!out.exists());
}

#[test]
fn lock_is_stable_and_blocks_pending_entries_by_default() {
    let pending = library("lock-pending", ReviewState::Pending);
    let out = pending.join("catalog.lock.json");
    let (code, stdout, stderr) = pxl(&["--json", "prototype", "lock", "--library", s(&pending), "-o", s(&out)]);
    assert_ne!(code, 0, "stdout={stdout}");
    assert!(format!("{stdout}{stderr}").contains("not approved"), "{stdout}{stderr}");

    let approved = library("lock-ok", ReviewState::Approved);
    let a = approved.join("a.lock.json");
    let b = approved.join("b.lock.json");
    for out in [&a, &b] {
        let (code, stdout, stderr) = pxl(&["--json", "prototype", "lock", "--library", s(&approved), "-o", s(out)]);
        assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    }
    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
    let lock: Value = serde_json::from_str(&fs::read_to_string(&a).unwrap()).unwrap();
    let ids: Vec<&str> = lock["entries"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["roof_tile", "tiny_house"]);
}
