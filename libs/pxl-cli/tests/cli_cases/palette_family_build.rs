//! P04 — multi palette family orchestration. A v2 intent freezes two palette
//! families; the batch must resolve them once, bind every asset to its
//! family's palette, and record stable resource ids. Input order may not
//! change the result, a stale family palette fails the whole batch with the
//! family id, and a v1 build (no families) keeps its legacy outputs.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::support::pxl;

fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("pxl-family-build-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    root
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{}", pxl_core::sha256_hex(bytes))
}

fn write_json(path: &Path, value: &Value) -> String {
    let text = serde_json::to_string_pretty(value).unwrap() + "\n";
    fs::write(path, &text).unwrap();
    text
}

/// One 16x16 hex-indexed tile: a filled frame plus a `body` region mask.
fn shapes_json(w: usize, h: usize) -> (Vec<String>, Vec<String>) {
    let mut pixels = Vec::new();
    for y in 0..h {
        let mut row = String::new();
        for x in 0..w {
            let body = x == 0 || y == 0 || x == w - 1 || y == h - 1
                || (x > 3 && x < w - 4 && y > 3 && y < h - 4);
            row.push(if body { '1' } else { '.' });
        }
        pixels.push(row);
    }
    let mask = pixels
        .iter()
        .map(|row| {
            row.chars()
                .map(|cell| if cell == '.' { '.' } else { '1' })
                .collect::<String>()
        })
        .collect();
    (pixels, mask)
}

fn palette_doc(id: &str, colors: &[&str]) -> Value {
    json!({
        "v": 1,
        "id": id,
        "colors": colors,
        "ramps": {"main": [1, 2]},
        "cycles": {},
        "special": {"transparent": 0, "outline": 1, "flash": 2},
    })
}

/// A workspace with `assets.len()` recipes, each bound to its own family.
/// `order` reverses the intent's family list and the recipe discovery order,
/// so tests can prove the result is order-independent.
fn family_workspace(tag: &str, order: &[&str]) -> PathBuf {
    let root = temp_root(tag);
    let families = ["core", "enemy"];
    let ordered: Vec<&str> = order.to_vec();

    let core_colors = ["#00000000", "#140c1c", "#d04648"];
    let enemy_colors = ["#00000000", "#442434", "#757161"];
    for (family, colors) in [("core", core_colors.as_slice()), ("enemy", enemy_colors.as_slice())] {
        fs::create_dir_all(root.join("palettes")).unwrap();
        write_json(&root.join(format!("palettes/{family}.palette.json")), &palette_doc(family, colors));
    }

    let mut intent_families = Vec::new();
    for family in &ordered {
        let bytes = fs::read(root.join(format!("palettes/{family}.palette.json"))).unwrap();
        let semantic = json!({
            "ramps": {"main": [1, 2]},
            "cycles": {},
            "special": {"transparent": 0, "outline": 1, "flash": 2},
        });
        intent_families.push(json!({
            "id": family,
            "palette": format!("palettes/{family}.palette.json"),
            "digest": sha256(&bytes),
            "semantic_digest": sha256(serde_json::to_vec(&semantic).unwrap().as_slice()),
            "transparent_index": 0,
        }));
    }

    let mut asset_policies = Vec::new();
    let mut plan_assets = Vec::new();
    let mut manifest_assets = Vec::new();
    let family_of_asset = |asset: &str| -> &str {
        if asset.ends_with("enemy") { "enemy" } else { "core" }
    };
    for family in &ordered {
        let asset = format!("{family}_tile");
        let (pixels, mask) = shapes_json(16, 16);
        let dir = root.join(&asset);
        fs::create_dir_all(&dir).unwrap();
        write_json(
            &dir.join("tile.shapes.json"),
            &json!({
                "w": 16, "h": 16,
                "draw": [{"x": 0, "y": 0, "pixels": pixels}],
                "regions": {"body": {"pixels": mask}},
            }),
        );
        write_json(
            &dir.join("tile.recipe.json"),
            &json!({
                "v": 1,
                "asset": asset,
                "gen_version": 1,
                "seed": 7,
                "palette": format!("../palettes/{family}.palette.json"),
                "sprites": [{
                    "id": asset,
                    "kind": "sprite",
                    "base": {"gen": "shapes", "spec": "tile.shapes.json", "seed": 7},
                    "anims": []
                }],
                "sheet": {"sources": [asset], "layout": "rows-by-anim", "pad": 1, "file": "asset.sheet.png"},
                "validate": {"rules": "default"}
            }),
        );
        asset_policies.push(json!({
            "asset_id": asset, "kind": "tile", "role": "background", "native_size": [16, 16],
            "material_ids": ["flat"], "protected_regions": [], "finish": "keep", "lighting": "unlit",
            "palette_family": family,
        }));
        plan_assets.push(json!({
            "asset_id": asset, "role": "background", "native_size": [16, 16],
            "finish": "keep", "lighting": "unlit",
            "regions": [
                {"region_id": "body", "material_id": "flat", "ramp": "main", "light_role": "base", "protect": false,
                 "source": {"kind": "shape", "selector": "tile.shapes.json#body"}, "mask": null}
            ],
            "animations": [],
            "tile": {"family": "ground", "neighbours": [asset], "variant_seed": 7},
            "channels": {"material_labels": true, "emissive": false, "shadow": false},
            "palette_family": family,
        }));
        manifest_assets.push(json!({
            "id": asset, "recipe": format!("{asset}/tile.recipe.json"), "status": "ok"
        }));
        let _ = family_of_asset(&asset);
    }

    let intent = json!({
        "contract": "visual-intent/v2",
        "profile": {"id": "family_pixel", "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"},
        "pixel_grid": {"native_size": [16, 16], "display_scale": 1, "snap": true},
        "palette_policy": {
            "mode": "strict", "palette": "palettes/core.palette.json",
            "allowed_indices": [0, 1, 2], "ui_palette": null, "runtime_grade": false, "extension_budget": 0
        },
        "roles": {"background": {"value": [0, 100], "saturation": [0, 120], "relative_contrast": -1}},
        "value_script": {"focal_roles": [], "rest_roles": ["background"]},
        "materials": {"flat": {"ramp": "main", "finish": "keep", "response": "flat", "regions": ["body"]}},
        "asset_policies": asset_policies,
        "animation_policy": {"max_frames": 16, "hold_terminal": true, "allow_squash": false, "allow_smear": false, "required_phases": [], "contact_tolerance": 1},
        "scene_previews": [{
            "id": "main", "size": [16, 16], "background": "dark", "scale": 1, "tick": 0,
            "instances": [{"asset_id": "core_tile", "position": [0, 0], "layer": "world", "frame": null}],
            "rois": [{"id": "main", "layer": "world", "rect": [0, 0, 16, 16]}]
        }],
        "review_policy": {"required_dimensions": ["silhouette"], "min_reviewers": 2},
        "sources": {"pixel_grid": "profile", "palette_policy": "project"},
        "risks": [],
        "palette_families": intent_families,
    });
    let intent_text = write_json(&root.join("visual-intent.snapshot.json"), &intent);
    let plan = json!({
        "contract": "visual-plan/v2",
        "intent_digest": sha256(intent_text.as_bytes()),
        "assets": plan_assets,
        "previews": [{"scene_id": "main", "assets": ["core_tile"]}],
    });
    let plan_text = write_json(&root.join("visual-plan.json"), &plan);
    write_json(
        &root.join("spec-manifest.json"),
        &json!({
            "schema_version": 3,
            "scope": "all",
            "palette": "palettes/core.palette.json",
            "visual_intent": {"path": "visual-intent.snapshot.json", "digest": sha256(intent_text.as_bytes())},
            "visual_plan": {"path": "visual-plan.json", "digest": sha256(plan_text.as_bytes())},
            "capability_digest": "families",
            "assets": manifest_assets,
        }),
    );
    root
}

fn plan_run(root: &Path, tag: &str) -> (i32, String, String, PathBuf) {
    let out = root.join(format!("out-{tag}"));
    let (code, stdout, stderr) = pxl(&[
        "--json", "plan", "--in", root.to_str().unwrap(), "--out", out.to_str().unwrap(),
    ]);
    (code, stdout, stderr, out)
}

/// Two families build in one batch; every manifest records its asset's family,
/// a stable resource id, and sheet/frame references. The two assets share the
/// same resource id shape but differ because family is part of the identity.
#[test]
fn dual_family_build_binds_families_and_resource_ids() {
    let root = family_workspace("dual", &["core", "enemy"]);
    let (code, stdout, stderr, out) = plan_run(&root, "dual");
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");

    let index: Value =
        serde_json::from_str(&fs::read_to_string(out.join("index.json")).unwrap()).unwrap();
    assert_eq!(index["schema_version"], 5);
    for entry in index["recipes"].as_array().unwrap() {
        assert_eq!(entry["status"], "ok", "entry: {entry}");
    }
    for family in ["core", "enemy"] {
        let asset = format!("{family}_tile");
        let manifest_path = out
            .join(&asset)
            .join("asset.build.json");
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
        let resources = manifest["resources"].as_array().unwrap();
        assert_eq!(resources.len(), 1, "one resource per built asset: {manifest}");
        let resource = &resources[0];
        assert_eq!(resource["asset_id"], asset);
        assert_eq!(resource["palette_family"], family);
        let resource_id = resource["resource_id"].as_str().unwrap();
        assert!(resource_id.starts_with(&format!("res-{family}-")), "{resource_id}");
        assert_eq!(resource["sheet"], "asset.sheet.png");
        assert_eq!(resource["document"], format!("{asset}.pxl.json"));        assert!(resource["source_digest"].as_str().unwrap().starts_with("sha256:"));
        let frames = resource["frames"].as_array().unwrap();
        assert!(!frames.is_empty(), "sheet/frame references are required: {resource}");
        assert!(frames[0]["index"].is_u64());

        // The effective style summarises both families, ordered by id.
        let style: Value = serde_json::from_str(
            &fs::read_to_string(out.join(&asset).join("effective-style.json")).unwrap(),
        )
        .unwrap();
        let families = style["palette_families"].as_array().unwrap();
        let ids: Vec<&str> = families.iter().map(|f| f["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["core", "enemy"], "families are id-ordered: {style}");
    }

    // Cross-family identity: same asset id under different families never
    // collapses to one resource id.
    let core_manifest: Value = serde_json::from_str(
        &fs::read_to_string(out.join("core_tile/asset.build.json")).unwrap(),
    )
    .unwrap();
    let enemy_manifest: Value = serde_json::from_str(
        &fs::read_to_string(out.join("enemy_tile/asset.build.json")).unwrap(),
    )
    .unwrap();
    assert_ne!(
        core_manifest["resources"][0]["resource_id"],
        enemy_manifest["resources"][0]["resource_id"],
        "resource ids must differ across families"
    );
}

/// Input order may not change the result: reversing the intent's family list
/// and the recipe discovery order produces byte-identical per-asset outputs
/// and the same index verdict.
#[test]
fn family_input_order_does_not_change_the_result() {
    let forward = family_workspace("order-a", &["core", "enemy"]);
    let reverse = family_workspace("order-b", &["enemy", "core"]);
    let (code_a, stdout_a, stderr_a, out_a) = plan_run(&forward, "order");
    let (code_b, stdout_b, stderr_b, out_b) = plan_run(&reverse, "order");
    assert_eq!(code_a, 0, "stdout={stdout_a}\nstderr={stderr_a}");
    assert_eq!(code_b, 0, "stdout={stdout_b}\nstderr={stderr_b}");

    // The built artefacts themselves (indexed document, sheet json, sheet
    // png) must be byte-identical. The manifests legitimately differ: their
    // inputs record the frozen snapshot digests, which change with the
    // intent's family order even though nothing about the pixels changed.
    for family in ["core", "enemy"] {
        let asset = format!("{family}_tile");
        for name in [
            format!("{asset}.pxl.json"),
            "asset.sheet.json".to_string(),
            "asset.sheet.png".to_string(),
        ] {
            let a = fs::read(out_a.join(&asset).join(&name)).unwrap();
            let b = fs::read(out_b.join(&asset).join(&name)).unwrap();
            assert_eq!(a, b, "{name} differs between input orders");
        }
        // The resource identity must not move either.
        let manifest_a: Value = serde_json::from_str(
            &fs::read_to_string(out_a.join(&asset).join("asset.build.json")).unwrap(),
        )
        .unwrap();
        let manifest_b: Value = serde_json::from_str(
            &fs::read_to_string(out_b.join(&asset).join("asset.build.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            manifest_a["resources"], manifest_b["resources"],
            "resource ids must not depend on input order"
        );
    }
}

/// A stale family palette (bytes changed after the intent froze the digest)
/// fails the whole batch before any recipe builds, with the family id.
#[test]
fn stale_family_palette_fails_the_batch_with_family_id() {
    let root = family_workspace("stale", &["core", "enemy"]);
    let palette_path = root.join("palettes/enemy.palette.json");
    let mut palette: Value =
        serde_json::from_str(&fs::read_to_string(&palette_path).unwrap()).unwrap();
    palette["colors"][2] = json!("#ffffff");
    write_json(&palette_path, &palette);

    let (code, stdout, stderr) = pxl(&[
        "--json", "plan", "--dry-run",
        "--in", root.to_str().unwrap(),
        "--out", root.join("out-stale").to_str().unwrap(),
    ]);
    assert_ne!(code, 0, "a stale family palette must fail\nstdout={stdout}");
    let text = format!("{stdout}{stderr}");
    assert!(text.contains("enemy"), "the failure must name the family: {text}");
    assert!(text.contains("palette families"), "{text}");
    assert!(
        !root.join("out-stale").exists(),
        "nothing may be built when a family fails to resolve"
    );
}

/// A recipe that declares the wrong palette for its family fails with the
/// family id, at build time, instead of quietly quantising across families.
#[test]
fn wrong_family_palette_in_recipe_fails_with_family_id() {
    let root = family_workspace("wrongpal", &["core", "enemy"]);
    let recipe_path = root.join("core_tile/tile.recipe.json");
    let mut recipe: Value =
        serde_json::from_str(&fs::read_to_string(&recipe_path).unwrap()).unwrap();
    recipe["palette"] = json!("../palettes/enemy.palette.json");
    write_json(&recipe_path, &recipe);

    let (code, stdout, stderr, out) = plan_run(&root, "wrongpal");
    assert_eq!(code, 5, "one failed recipe fails the batch with exit 5\nstdout={stdout}\nstderr={stderr}");
    let index: Value =
        serde_json::from_str(&fs::read_to_string(out.join("index.json")).unwrap()).unwrap();
    let entry = index["recipes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "core_tile")
        .unwrap();
    assert_eq!(entry["status"], "parse_error");
    let error = entry["error"].as_str().unwrap();
    assert!(error.contains("core"), "the failure must name the family: {error}");
    assert!(error.contains("enemy"), "the failure must name the wrong palette: {error}");
    // The untouched family still built.
    let enemy = index["recipes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "enemy_tile")
        .unwrap();
    assert_eq!(enemy["status"], "ok");
}

/// A v1 workspace (no families) keeps its legacy outputs: the manifest still
/// records resources but with no palette_family, and the resource id falls
/// back to the `default` family.
#[test]
fn v1_workspace_keeps_legacy_outputs() {
    let root = temp_root("v1");
    let asset_dir = root.join("bullet");
    fs::create_dir_all(&asset_dir).unwrap();
    let (pixels, mask) = shapes_json(16, 16);
    write_json(
        &asset_dir.join("tile.shapes.json"),
        &json!({
            "w": 16, "h": 16,
            "draw": [{"x": 0, "y": 0, "pixels": pixels}],
            "regions": {"body": {"pixels": mask}},
        }),
    );
    let palette = palette_doc("legacy", &["#00000000", "#140c1c", "#d04648"]);
    write_json(&asset_dir.join("palette.json"), &palette);
    write_json(
        &asset_dir.join("bullet.recipe.json"),
        &json!({
            "v": 1,
            "asset": "bullet",
            "gen_version": 1,
            "seed": 7,
            "palette": "palette.json",
            "sprites": [{
                "id": "bullet", "kind": "sprite",
                "base": {"gen": "shapes", "spec": "tile.shapes.json", "seed": 7},
                "anims": []
            }],
            "sheet": {"sources": ["bullet"], "layout": "rows-by-anim", "pad": 1, "file": "bullet.sheet.png"},
            "validate": {"rules": "default"}
        }),
    );
    let (code, stdout, stderr, out) = plan_run(&root, "v1");
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(out.join("bullet/bullet.build.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["schema_version"], 3);
    let resources = manifest["resources"].as_array().unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0]["palette_family"], Value::Null);
    assert!(
        resources[0]["resource_id"].as_str().unwrap().starts_with("res-default-"),
        "legacy assets fall back to the default family: {}", resources[0]
    );
    assert!(!out.join("bullet/effective-style.json").exists());
}
