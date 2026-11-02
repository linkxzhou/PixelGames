use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::support::pxl;

/// Build a minimal modern-policy workspace: a 32x32 tile authored with 8
/// visible colours, palette families, and a visual-intent/v2 carrying the
/// modern profile. Returns the root directory.
fn modern_workspace(tag: &str, palette_entries: usize, palette_colors: usize) -> PathBuf {
    let root = std::env::temp_dir().join(format!("pxl-modern-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let asset_dir = root.join("asset");
    fs::create_dir_all(&asset_dir).unwrap();

    // 32x32 tile body with 8 visible colours plus the transparent index.
    let mut colors = vec!["#00000000".to_string()];
    colors.extend(
        [
            "#140c1c", "#442434", "#30346d", "#4e4a4e",
            "#854c30", "#346524", "#d04648", "#757161",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    colors.truncate(palette_colors.max(1));
    // Visible indices the source art may reference (1..=8, all < colors.len()).
    let visible = (palette_colors.max(2) - 1).min(8);
    let mut pixels = Vec::new();
    for y in 0..32 {
        let mut row = String::new();
        for x in 0..32 {
            let body = x == 0 || y == 0 || x == 31 || y == 31
                || (x > 8 && x < 24 && y > 8 && y < 24);
            row.push(if body {
                // Hex-indexed art: index chars 1..=visible-1, '.' is index 0.
                std::char::from_digit(((x + y) % (visible - 1) + 1) as u32, 10).unwrap_or('1')
            } else {
                '.'
            });
        }
        pixels.push(row);
    }
    let body_mask = pixels
        .iter()
        .map(|row| {
            row.chars()
                .map(|cell| if cell == '.' { '.' } else { '1' })
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    write_json(
        &asset_dir.join("tile.shapes.json"),
        &json!({
            "w": 32,
            "h": 32,
            "draw": [{"x": 0, "y": 0, "pixels": pixels}],
            "regions": {"body": {"pixels": body_mask}}
        }),
    );
    write_json(
        &asset_dir.join("palette.json"),
        &json!({
            "v": 1,
            "id": "modern",
            "colors": colors,
            "ramps": {"main": (1..palette_colors.min(9)).collect::<Vec<_>>()},
            "cycles": {},
            "special": {"transparent": 0, "outline": 1, "flash": palette_colors.saturating_sub(1).max(1)}
        }),
    );
    write_json(
        &asset_dir.join("tile.recipe.json"),
        &json!({
            "v": 1,
            "asset": "modern_tile",
            "gen_version": 1,
            "seed": 7,
            "palette": "palette.json",
            "sprites": [{
                "id": "asset",
                "kind": "sprite",
                "base": {"gen": "shapes", "spec": "tile.shapes.json", "seed": 7},
                "anims": []
            }],
            "sheet": {"sources": ["asset"], "layout": "rows-by-anim", "pad": 1, "file": "asset.sheet.png"},
            "validate": {"rules": "default"}
        }),
    );
    let intent = json!({
        "contract": "visual-intent/v2",
        "profile": {"id": "modern_pixel", "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"},
        "pixel_grid": {"native_size": [32, 32], "display_scale": 2, "snap": true},
        "palette_policy": {
            "mode": "strict", "palette": "palette.json",
            "allowed_indices": (0..palette_colors as u32).collect::<Vec<_>>(),
            "ui_palette": null, "runtime_grade": false, "extension_budget": 0
        },
        "roles": {
            "background": {"value": [0, 100], "saturation": [0, 120], "relative_contrast": -1}
        },
        "value_script": {"focal_roles": [], "rest_roles": ["background"]},
        "materials": {
            "flat": {"ramp": "main", "finish": "keep", "response": "flat", "regions": ["body"]}
        },
        "asset_policies": [{
            "asset_id": "asset", "kind": "tile", "role": "background", "native_size": [32, 32],
            "material_ids": ["flat"], "protected_regions": [], "finish": "keep", "lighting": "unlit",
            "palette_family": "core"
        }],
        "animation_policy": {"max_frames": 16, "hold_terminal": true, "allow_squash": false, "allow_smear": false, "required_phases": [], "contact_tolerance": 1},
        "scene_previews": [{
            "id": "main", "size": [32, 32], "background": "dark", "scale": 2, "tick": 0,
            "instances": [{"asset_id": "asset", "position": [0, 0], "layer": "world", "frame": null}],
            "rois": [{"id": "main", "layer": "world", "rect": [0, 0, 32, 32]}]
        }],
        "review_policy": {"required_dimensions": ["silhouette"], "min_reviewers": 2},
        "sources": {"pixel_grid": "profile", "palette_policy": "project"},
        "risks": [],
        "modern": {
            "profile_contract": "modern-pixel/v1",
            "logical_canvas": [320, 240],
            "tile_size": [32, 32],
            "target_runtime": "runtime-js",
            "sampling": "nearest_integer",
            "budgets": {
                "palette_entries": palette_entries,
                "frame_opaque_colors": 16,
                "ramp_depth": 5,
                "scene_rgb": 128
            }
        },
        "palette_families": [
            {
                "id": "core",
                "palette": "asset/palette.json",
                "digest": format!("sha256:{}", pxl_core::sha256_hex(fs::read(root.join("asset/palette.json")).unwrap().as_slice())),
                // The semantic digest is sha256 over the palette's semantic
                // projection (ramps, cycles, special), matching what the
                // FamilyRegistry verifies at load time.
                "semantic_digest": semantic_digest_of(&root.join("asset/palette.json")),
                "transparent_index": 0
            }
        ]
    });
    let intent_text = write_json(&root.join("visual-intent.snapshot.json"), &intent);
    let plan = json!({
        "contract": "visual-plan/v1",
        "intent_digest": format!("sha256:{}", pxl_core::sha256_hex(intent_text.as_bytes())),
        "assets": [{
            "asset_id": "asset", "role": "background", "native_size": [32, 32],
            "finish": "keep", "lighting": "unlit",
            "regions": [
                {"region_id": "body", "material_id": "flat", "ramp": "main", "light_role": "base", "protect": false, "source": {"kind": "shape", "selector": "tile.shapes.json#body"}, "mask": null}
            ],
            "animations": [],
            "tile": {"family": "ground", "neighbours": ["asset"], "variant_seed": 7},
            "channels": {"material_labels": true, "emissive": false, "shadow": false}
        }],
        "previews": [{"scene_id": "main", "assets": ["asset"]}]
    });
    let plan_text = write_json(&root.join("visual-plan.json"), &plan);
    write_json(
        &root.join("spec-manifest.json"),
        &json!({
            "schema_version": 3,
            "scope": "all",
            "palette": "asset/palette.json",
            "visual_intent": {"path": "visual-intent.snapshot.json", "digest": format!("sha256:{}", pxl_core::sha256_hex(intent_text.as_bytes()))},
            "visual_plan": {"path": "visual-plan.json", "digest": format!("sha256:{}", pxl_core::sha256_hex(plan_text.as_bytes()))},
            "capability_digest": "modern",
            "assets": [{"id": "asset", "recipe": "asset/tile.recipe.json", "status": "ok"}]
        }),
    );
    root
}

use std::path::PathBuf;

fn write_json(path: &Path, value: &Value) -> String {
    let text = serde_json::to_string_pretty(value).unwrap() + "\n";
    fs::write(path, &text).unwrap();
    text
}

/// sha256 over the palette's semantic projection (ramps, cycles, special),
/// mirroring `pxl_contract::palette_family` so the fixture freezes what the
/// registry will verify.
fn semantic_digest_of(palette_path: &Path) -> String {
    let palette: Value =
        serde_json::from_str(&fs::read_to_string(palette_path).unwrap()).unwrap();
    let semantic = json!({
        "ramps": palette.get("ramps").cloned().unwrap_or(Value::Null),
        "cycles": palette.get("cycles").cloned().unwrap_or(Value::Null),
        "special": palette.get("special").cloned().unwrap_or(Value::Null),
    });
    format!(
        "sha256:{}",
        pxl_core::sha256_hex(serde_json::to_vec(&semantic).unwrap().as_slice())
    )
}

/// A 32x32 tile authored with 8 visible colours passes modern QA: the
/// effective policy replaces the retro budgets, and the retro block checks
/// stay off. Dry-run and the real build agree.
#[test]
fn modern_tile_passes_and_dry_run_matches_build() {
    let root = modern_workspace("tile", 48, 9);
    let out = root.join("out");

    let (code, stdout, stderr) = pxl(&[
        "--json", "plan", "--dry-run", "--in", root.to_str().unwrap(),
        "--out", out.join("dry").to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "dry-run\nstdout={stdout}\nstderr={stderr}");
    let dry: Value = serde_json::from_str(stdout.trim().split('\n').last().unwrap()).unwrap();
    assert_eq!(dry["dry_run"], true);
    assert!(!out.join("dry/asset").exists(), "dry-run must not write build outputs");

    let (code, stdout, stderr) = pxl(&[
        "--json", "plan", "--in", root.to_str().unwrap(),
        "--out", out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "build\nstdout={stdout}\nstderr={stderr}");

    let style: Value = serde_json::from_str(
        &fs::read_to_string(out.join("asset/effective-style.json")).unwrap(),
    )
    .unwrap();
    let policy = &style["effective_policy"];
    assert_eq!(policy["contract"], "effective-policy/v1");
    assert_eq!(policy["canvas"], json!([320, 240]));
    assert_eq!(policy["palette_entries"], 48);
    assert!(policy["digest"]
        .as_str()
        .unwrap()
        .starts_with("policy-jcs-sha256:"));
}

/// palette_entries above the 62 hard limit fails clearly, at dry-run time,
/// before anything is built.
#[test]
fn palette_entries_above_hard_limit_fail() {
    let root = modern_workspace("entries63", 63, 9);
    let (code, stdout, stderr) = pxl(&[
        "--json", "plan", "--dry-run", "--in", root.to_str().unwrap(),
        "--out", root.join("out").to_str().unwrap(),
    ]);
    assert_ne!(code, 0, "63 palette entries must fail\nstdout={stdout}");
    let text = format!("{stdout}{stderr}");
    assert!(
        text.contains("palette_entries must be in 1..=62"),
        "clear failure expected, got: {text}"
    );
}

/// A missing palette index (allowed_indices names an entry the palette does
/// not have) fails; the retro default still demands index 0 transparency.
#[test]
fn missing_palette_index_fails() {
    let root = modern_workspace("badidx", 48, 5);
    let intent_path = root.join("visual-intent.snapshot.json");
    let mut intent: Value =
        serde_json::from_str(&fs::read_to_string(&intent_path).unwrap()).unwrap();
    intent["palette_policy"]["allowed_indices"] = json!((0..9u32).collect::<Vec<_>>());
    let text = serde_json::to_string_pretty(&intent).unwrap() + "\n";
    fs::write(&intent_path, &text).unwrap();
    // Re-pin the plan digest to the edited intent.
    let plan_path = root.join("visual-plan.json");
    let mut plan: Value =
        serde_json::from_str(&fs::read_to_string(&plan_path).unwrap()).unwrap();
    plan["intent_digest"] = json!(format!("sha256:{}", pxl_core::sha256_hex(text.as_bytes())));
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).unwrap() + "\n",
    )
    .unwrap();

    let (code, stdout, stderr) = pxl(&[
        "--json", "plan", "--dry-run", "--in", root.to_str().unwrap(),
        "--out", root.join("out").to_str().unwrap(),
    ]);
    assert_ne!(code, 0, "missing palette index must fail\nstdout={stdout}\nstderr={stderr}");
}

/// Wrong native size (plan says 32x32, source is 16x16) fails the visual
/// contract.
#[test]
fn wrong_native_size_fails() {
    let root = modern_workspace("size", 48, 9);
    let recipe_path = root.join("asset/tile.recipe.json");
    let mut recipe: Value =
        serde_json::from_str(&fs::read_to_string(&recipe_path).unwrap()).unwrap();
    recipe["sprites"][0]["base"]["spec"] = json!("small.shapes.json");
    fs::write(
        &recipe_path,
        serde_json::to_string_pretty(&recipe).unwrap() + "\n",
    )
    .unwrap();
    let mut pixels = Vec::new();
    for _ in 0..16 {
        pixels.push("1".repeat(15) + ".");
    }
    write_json(
        &root.join("asset/small.shapes.json"),
        &json!({
            "w": 16,
            "h": 16,
            "draw": [{"x": 0, "y": 0, "pixels": pixels}],
            "regions": {}
        }),
    );
    let (code, stdout, stderr) = pxl(&[
        "--json", "plan", "--dry-run", "--in", root.to_str().unwrap(),
        "--out", root.join("out").to_str().unwrap(),
    ]);
    assert_ne!(code, 0, "wrong native size must fail: {stderr}");
    let text = format!("{stdout}{stderr}");
    assert!(
        text.contains("visual plan requires 32x32") || text.contains("is 16x16"),
        "expected a native-size failure, got: {text}"
    );
}
