use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::support::pxl;

#[test]
fn benchmark_has_frozen_coverage_and_independent_cases() {
    let manifest: Value = serde_json::from_str(include_str!(
        "../fixtures/visual-benchmark/manifest.json"
    ))
    .unwrap();
    assert_eq!(manifest["contract"], "visual-benchmark/v1");
    let cases = manifest["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 48);
    let ids = cases
        .iter()
        .map(|case| case["id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), 48);
    assert_eq!(
        cases.iter().filter(|case| case["set"] == "development").count(),
        32
    );
    assert_eq!(
        cases.iter().filter(|case| case["set"] == "holdout").count(),
        16
    );
    assert_eq!(
        cases
            .iter()
            .filter(|case| matches!(case["family"].as_str(), Some("actor" | "prop")))
            .count(),
        12
    );
    for (family, expected) in [("tile", 12), ("animation", 8), ("ui", 8)] {
        assert_eq!(
            cases.iter().filter(|case| case["family"] == family).count(),
            expected
        );
    }
    assert_eq!(
        cases
            .iter()
            .filter(|case| matches!(case["family"].as_str(), Some("effect" | "overlay")))
            .count(),
        8
    );
    assert!(
        cases
            .iter()
            .filter(|case| case.get("anti_heuristic").is_some())
            .count()
            >= 12
    );
    for case in cases {
        assert!(!case["tamper"].as_str().unwrap().is_empty());
        assert_eq!(case["native_size"].as_array().unwrap().len(), 2);
        assert!(case["native_size"]
            .as_array()
            .unwrap()
            .iter()
            .all(|value| (1..=256).contains(&value.as_u64().unwrap())));
    }
}

#[test]
fn all_benchmark_cases_build_through_the_visual_pipeline() {
    let manifest: Value = serde_json::from_str(include_str!(
        "../fixtures/visual-benchmark/manifest.json"
    ))
    .unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let root = std::env::temp_dir().join(format!("pxl-visual-benchmark-{id}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        write_case(&root, case);
        let out = root.join("out");
        let (code, stdout, stderr) = pxl(&[
            "--json",
            "plan",
            "--in",
            root.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{id}: stdout={stdout}\nstderr={stderr}");
        let document: Value = serde_json::from_str(
            &fs::read_to_string(out.join("asset/asset.pxl.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(document["w"], case["native_size"][0], "{id}");
        assert_eq!(document["h"], case["native_size"][1], "{id}");
        if id.contains("variable-duration") {
            assert_eq!(document["meta"]["durations"]["motion"], json!([100, 250]));
            let sheet: Value = serde_json::from_str(
                &fs::read_to_string(out.join("asset/asset.sheet.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(sheet["frames"]["asset_motion_0"]["duration"], 100);
            assert_eq!(sheet["frames"]["asset_motion_1"]["duration"], 250);
        }
        for name in [
            "effective-style.json",
            "visual-report.json",
            "visual-metadata.json",
            "preview-manifest.json",
            "asset.native.png",
            "asset.silhouette.png",
            "asset.material.sheet.json",
        ] {
            assert!(out.join("asset").join(name).is_file(), "{id}: missing {name}");
        }
        let previews: Value = serde_json::from_str(
            &fs::read_to_string(out.join("asset/preview-manifest.json")).unwrap(),
        )
        .unwrap();
        assert!(previews["previews"]
            .as_array()
            .unwrap()
            .iter()
            .any(|preview| preview["mode"].as_str().is_some_and(|mode| mode.starts_with("nearest-"))));
        let report: Value = serde_json::from_str(
            &fs::read_to_string(out.join("asset/visual-report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["verdict"], "PASS", "{id}");
        assert_eq!(report["visual_review"], "not_run", "{id}");
        let _ = fs::remove_dir_all(root);
    }
}

fn write_case(root: &Path, case: &Value) {
    let asset_dir = root.join("asset");
    fs::create_dir_all(&asset_dir).unwrap();
    let id = case["id"].as_str().unwrap();
    let family = case["family"].as_str().unwrap();
    let style = case["style"].as_str().unwrap();
    let w = case["native_size"][0].as_u64().unwrap() as usize;
    let h = case["native_size"][1].as_u64().unwrap() as usize;
    let anti = case["anti_heuristic"].as_str().unwrap_or_default();
    let monochrome = style == "monochrome";
    let (material, response, finish) = if family == "ui" || monochrome {
        ("flat", "flat", "keep")
    } else if family == "effect" || family == "overlay" {
        ("emissive", "emissive", "flat")
    } else {
        match style {
            "strict_retro" => ("metal", "hard_edge", "hard_edge"),
            "soft_pixel" => ("cloth", "round", "volume"),
            _ => ("wood", "planar", "volume"),
        }
    };
    let role = match family {
        "ui" | "overlay" => "ui",
        "effect" => "emissive",
        "tile" => "background",
        _ => "actor",
    };
    let kind = match family {
        "tile" => "tile",
        "ui" => "ui",
        "effect" => "effect",
        "overlay" => "overlay",
        _ => "sprite",
    };
    let recipe_kind = if kind == "tile" { "tiles" } else { kind };
    let mut pixels = Vec::new();
    for y in 0..h {
        let mut row = String::new();
        for x in 0..w {
            let body = if family == "tile" {
                x == 0 || y == 0 || x + 1 == w || y + 1 == h
                    || (x > w / 4 && x < w * 3 / 4 && y > h / 4 && y < h * 3 / 4)
            } else {
                x >= w / 4 && x < (w * 3 / 4).max(w / 4 + 1)
                    && y >= h / 4 && y < (h * 3 / 4).max(h / 4 + 1)
            };
            let hole = anti == "legal_holes" && x > 0 && y > 0 && x + 1 < w && y + 1 < h
                && (x + y) % 3 == 0;
            row.push(if body && !hole {
                if monochrome { '2' } else { '4' }
            } else {
                '.'
            });
        }
        pixels.push(row);
    }
    let eye_x = w / 2;
    let eye_y = h / 2;
    pixels[eye_y].replace_range(eye_x..=eye_x, if monochrome { "2" } else { "3" });
    let body_mask = pixels
        .iter()
        .map(|row| {
            row.chars()
                .map(|cell| if cell == '.' { '.' } else { '1' })
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let eye_mask = (0..h)
        .map(|y| {
            (0..w)
                .map(|x| if x == eye_x && y == eye_y { '1' } else { '.' })
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    write_json(
        &asset_dir.join("case.shapes.json"),
        &json!({
            "w": w,
            "h": h,
            "draw": [{"x": 0, "y": 0, "pixels": pixels}],
            "regions": {
                "body": {"pixels": body_mask},
                "eye": {"pixels": eye_mask},
            }
        }),
    );
    write_json(
        &asset_dir.join("palette.json"),
        &json!({
            "v": 1,
            "id": "benchmark",
            "colors": ["#00000000", "#140c1c", "#442434", "#30346d", "#4e4a4e", "#854c30", "#346524", "#d04648", "#757161", "#597dce", "#d27d2c", "#8595a1", "#6daa2c", "#d2aa99", "#6dc2ca", "#deeed6"],
            "ramps": {"main": [1, 2, 3, 4, 5]},
            "cycles": {}
        }),
    );
    let animated = family == "animation";
    let nonloop = id.contains("nonloop");
    let anims = if animated {
        json!([{
            "name": "motion", "fps": 1, "loop": !nonloop, "dirs": [],
            "frames": [[{"op": "noop"}], [{"op": "shift", "dy": 1}]]
        }])
    } else {
        json!([])
    };
    write_json(
        &asset_dir.join("case.recipe.json"),
        &json!({
            "v": 1,
            "asset": "benchmark_case",
            "gen_version": 1,
            "seed": 7,
            "palette": "palette.json",
            "sprites": [{
                "id": "asset",
                "kind": recipe_kind,
                "base": {"gen": "shapes", "spec": "case.shapes.json", "seed": 7},
                "anims": anims,
            }],
            "sheet": {"sources": ["asset"], "layout": "rows-by-anim", "pad": 1, "file": "asset.sheet.png"},
            "validate": {"rules": "default"}
        }),
    );
    let profile = profile(style);
    let intent = json!({
        "contract": "visual-intent/v1",
        "profile": profile,
        "pixel_grid": {"native_size": [w, h], "display_scale": 2, "snap": true},
        "palette_policy": {
            "mode": "strict", "palette": "palette.json",
            "allowed_indices": (0..=15).collect::<Vec<_>>(),
            "ui_palette": null, "runtime_grade": false, "extension_budget": 0
        },
        "roles": {
            role: {"value": [40, 220], "saturation": [0, 240], "relative_contrast": 1},
            "background": {"value": [0, 100], "saturation": [0, 120], "relative_contrast": -1}
        },
        "value_script": {"focal_roles": [role], "rest_roles": if role == "background" { json!([]) } else { json!(["background"]) }}, 
        "materials": {
            material: {"ramp": "main", "finish": finish, "response": response, "regions": ["body", "eye"]}
        },
        "asset_policies": [{
            "asset_id": "asset", "kind": kind, "role": role, "native_size": [w, h],
            "material_ids": [material], "protected_regions": ["eye"], "finish": finish,
            "lighting": if family == "ui" || family == "overlay" { "unlit" } else if family == "effect" { "hybrid" } else { "baked" }
        }],
        "animation_policy": {
            "max_frames": 16, "hold_terminal": true,
            "allow_squash": anti == "large_area_change", "allow_smear": anti == "smear_frame",
            "required_phases": ["contact"], "contact_tolerance": 1
        },
        "scene_previews": [{
            "id": "main", "size": [w, h], "background": "dark", "scale": 2, "tick": 0,
            "instances": [{"asset_id": "asset", "position": [0, 0], "layer": if family == "ui" || family == "overlay" { "ui" } else { "world" }, "frame": null}],
            "rois": [{"id": "main", "layer": if family == "ui" || family == "overlay" { "ui" } else { "world" }, "rect": [0, 0, w, h]}]
        }],
        "review_policy": {"required_dimensions": ["silhouette", "material_form"], "min_reviewers": 2},
        "sources": {"pixel_grid": "profile", "palette_policy": "project"},
        "risks": []
    });
    let intent_text = write_json(&root.join("visual-intent.snapshot.json"), &intent);
    let durations = if id.contains("variable-duration") { vec![100, 250] } else { vec![1000, 1000] };
    let plan_animations = if animated {
        json!([{
            "anim_id": "motion", "anchor": [w / 2, h - 1], "contact": [w / 2, h - 1],
            "allow_squash": anti == "large_area_change", "allow_smear": anti == "smear_frame", "terminal_empty": false,
            "phases": [
                {"phase_id": "anticipation", "frames": ["asset_motion_0"], "duration_ms": [durations[0]], "transform": null},
                {"phase_id": "contact", "frames": ["asset_motion_1"], "duration_ms": [durations[1]], "transform": {"kind": "translate", "offset": [0, 1], "rotate90": 0}}
            ]
        }])
    } else {
        json!([])
    };
    let plan = json!({
        "contract": "visual-plan/v1",
        "intent_digest": format!("sha256:{}", pxl_core::sha256_hex(intent_text.as_bytes())),
        "assets": [{
            "asset_id": "asset", "role": role, "native_size": [w, h],
            "finish": finish, "lighting": if family == "ui" || family == "overlay" { "unlit" } else if family == "effect" { "hybrid" } else { "baked" },
            "regions": [
                {"region_id": "body", "material_id": material, "ramp": "main", "light_role": if family == "effect" { "emissive" } else { "base" }, "protect": false, "source": {"kind": "shape", "selector": "case.shapes.json#body"}, "mask": null},
                {"region_id": "eye", "material_id": material, "ramp": "main", "light_role": if family == "effect" { "emissive" } else { "base" }, "protect": true, "source": {"kind": "shape", "selector": "case.shapes.json#eye"}, "mask": null}
            ],
            "animations": plan_animations,
            "tile": if family == "tile" { json!({"family": "ground", "neighbours": ["asset"], "variant_seed": 7}) } else { Value::Null },
            "channels": {"material_labels": true, "emissive": family == "effect", "shadow": false}
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
            "capability_digest": "benchmark",
            "assets": [{"id": "asset", "recipe": "asset/case.recipe.json", "status": "ok"}]
        }),
    );
}

fn profile(style: &str) -> Value {
    let (id, digest) = match style {
        "strict_retro" => ("strict_retro", "ac4891d29b55ea5bce356d462ce331849ca4fa7d185c3d3e12525ad1876e3d46"),
        "monochrome" => ("monochrome", "0cbd05dd041f99f8f1f99e25b66e8d4fe4853834630bd4c4fc1e781159c597ff"),
        "soft_pixel" => ("soft_pixel", "28e1f164395c8b1af2af0adccb07b7c7424fce630ab9f6479fcf41cd578fe2a5"),
        _ => ("material_rich", "e0120c9ca838d131d4c0710e62da45ad6ca61dff53c7cc1e4e5fc3b23c335a26"),
    };
    json!({"id": id, "digest": format!("sha256:{digest}")})
}

fn write_json(path: &PathBuf, value: &Value) -> String {
    let text = serde_json::to_string_pretty(value).unwrap() + "\n";
    fs::write(path, &text).unwrap();
    text
}

