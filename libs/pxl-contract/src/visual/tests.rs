use crate::visual::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn load_schema(name: &str) -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../steps/contracts")
        .join(name);
    let mut schema: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    // The repository ids ("pxl/visual-intent/v1") are relative names, which
    // jsonschema cannot use as a base URL. Drop them for in-memory checks.
    schema.as_object_mut().unwrap().remove("$id");
    schema
}

#[test]
fn visual_intent_v1_schema_accepts_valid_and_rejects_invalid() {
    let schema = load_schema("visual-intent.schema.json");
    let compiled = jsonschema::JSONSchema::compile(&schema).unwrap();
    let valid = serde_json::to_value(valid_intent()).unwrap();
    assert!(compiled.is_valid(&valid), "valid intent should pass v1 schema");
    let mut bad_contract = valid.clone();
    bad_contract["contract"] = serde_json::json!("visual-intent/v99");
    assert!(!compiled.is_valid(&bad_contract), "bad contract should fail");
    let mut bad_size = valid.clone();
    bad_size["pixel_grid"]["native_size"] = serde_json::json!([0, 16]);
    assert!(!compiled.is_valid(&bad_size), "zero size should fail");
    let mut bad_scene_size = valid.clone();
    bad_scene_size["scene_previews"][0]["size"] = serde_json::json!([300, 300]);
    assert!(!compiled.is_valid(&bad_scene_size), "scene 300x300 over v1 limit 256 should fail");
}

#[test]
fn visual_intent_v2_schema_accepts_modern_profile() {
    let schema = load_schema("visual-intent.v2.schema.json");
    let compiled = jsonschema::JSONSchema::compile(&schema).unwrap();
    let mut modern_intent = serde_json::to_value(valid_intent()).unwrap();
    modern_intent["contract"] = serde_json::json!("visual-intent/v2");
    modern_intent["modern"] = serde_json::json!({
        "profile_contract": "modern-pixel/v1",
        "logical_canvas": [640, 360],
        "tile_size": [32, 32],
        "target_runtime": "runtime-js",
        "sampling": "nearest_integer",
        "budgets": {
            "palette_entries": 48,
            "frame_opaque_colors": 16,
            "ramp_depth": 5,
            "scene_rgb": 128
        }
    });
    modern_intent["palette_families"] = serde_json::json!([
        {
            "id": "environment",
            "palette": "palettes/environment.json",
            "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "semantic_digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "transparent_index": 0
        }
    ]);
    modern_intent["scene_previews"][0]["size"] = serde_json::json!([640, 360]);
    assert!(compiled.is_valid(&modern_intent), "valid v2 modern intent should pass: {:?}", compiled.validate(&modern_intent).err().map(|it| it.map(|e| e.to_string()).collect::<Vec<_>>()));
    let mut bad_canvas = modern_intent.clone();
    bad_canvas["modern"]["logical_canvas"] = serde_json::json!([4097, 360]);
    assert!(!compiled.is_valid(&bad_canvas), "canvas over 4096 should fail");
    let mut bad_entries = modern_intent.clone();
    bad_entries["modern"]["budgets"]["palette_entries"] = serde_json::json!(63);
    assert!(!compiled.is_valid(&bad_entries), "palette_entries 63 over text limit 62 should fail");
}

#[test]
fn v1_schema_rejects_unknown_fields() {
    let schema = load_schema("visual-intent.schema.json");
    let compiled = jsonschema::JSONSchema::compile(&schema).unwrap();
    let mut with_modern = serde_json::to_value(valid_intent()).unwrap();
    with_modern["modern"] = serde_json::json!({"profile_contract": "modern-pixel/v1"});
    assert!(!compiled.is_valid(&with_modern), "v1 schema should reject 'modern' field");
}

pub fn valid_intent() -> VisualIntent {
    VisualIntent {
        contract: VISUAL_INTENT_CONTRACT.into(),
        profile: ProfileRef { id: "strict_retro".into(), digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000".into() },
        pixel_grid: PixelGrid { native_size: [16, 16], display_scale: 4, snap: true },
        palette_policy: PalettePolicy { mode: PaletteMode::Strict, palette: "palette.json".into(), allowed_indices: vec![0, 1, 2], ui_palette: None, runtime_grade: false, extension_budget: 0 },
        roles: BTreeMap::from([
            ("background".into(), RolePolicy { value: [0, 80], saturation: [0, 100], relative_contrast: -1 }),
            ("actor".into(), RolePolicy { value: [60, 200], saturation: [20, 200], relative_contrast: 1 }),
        ]),
        value_script: ValueScript { focal_roles: vec!["actor".into()], rest_roles: vec!["background".into()] },
        materials: BTreeMap::from([
            ("flat".into(), MaterialPolicy { ramp: "main".into(), finish: FinishPolicy::Keep, response: MaterialResponse::Flat, regions: vec!["body".into(), "eye".into()] }),
        ]),
        asset_policies: vec![AssetPolicy { asset_id: "hero".into(), kind: "sprite".into(), role: "actor".into(), native_size: Some([16, 16]), material_ids: vec!["flat".into()], protected_regions: vec!["eye".into()], finish: FinishPolicy::Keep, lighting: LightingMode::Unlit, palette_family: None, facing: None, anchor: None }],
        animation_policy: AnimationPolicy { max_frames: 16, hold_terminal: true, allow_squash: true, allow_smear: true, required_phases: vec!["contact".into()], contact_tolerance: 1 },
        scene_previews: vec![ScenePreview { id: "main".into(), size: [64, 64], background: "dark".into(), scale: 4, tick: 0, instances: vec![SceneInstance { asset_id: "hero".into(), position: [0, 0], layer: "world".into(), frame: None }], rois: vec![ReviewRoi { id: "actor".into(), layer: "world".into(), rect: [0, 0, 16, 16] }], render_mode: Some("L0".into()), lights: Vec::new() }],
        review_policy: ReviewPolicy { required_dimensions: vec!["silhouette".into()], min_reviewers: 2 },
        sources: BTreeMap::from([("pixel_grid".into(), ConfigOrigin::Profile)]),
        risks: Vec::new(),
        modern: None,
        palette_families: Vec::new(),
    }
}

#[test]
fn valid_minimal_intent_closes_references() {
    assert_eq!(valid_intent().validate(), Vec::<String>::new());
}

#[test]
fn bad_region_and_fake_risk_resolution_are_rejected() {
    let mut intent = valid_intent();
    intent.asset_policies[0].protected_regions = vec!["missing".into()];
    intent.risks.push(VisualRisk {
        risk_id: "risk".into(), target: "hero".into(), check: "contrast".into(),
        evidence_required: "visual report".into(), status: RiskStatus::Resolved,
        resolved_by: Some(EvidenceRef { report: String::new(), entry: String::new(), digest: "text".into() }),
    });
    let errors = intent.validate();
    assert!(errors.iter().any(|error| error.contains("protected region")));
    assert!(errors.iter().any(|error| error.contains("real report entry")));
}

fn modern_v2_intent() -> VisualIntent {
    let mut intent = valid_intent();
    intent.contract = VISUAL_INTENT_CONTRACT_V2.into();
    intent.modern = Some(crate::policy::ModernProfile {
        profile_contract: crate::policy::MODERN_PROFILE_CONTRACT.into(),
        logical_canvas: [640, 360],
        tile_size: [32, 32],
        target_runtime: "runtime-js".into(),
        sampling: "nearest_integer".into(),
        budgets: crate::policy::Budgets { palette_entries: 48, frame_opaque_colors: 16, ramp_depth: 5, scene_rgb: 128 },
    });
    intent.palette_families = vec![PaletteFamily {
        id: "environment".into(),
        palette: "palettes/environment.json".into(),
        digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000".into(),
        semantic_digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000".into(),
        transparent_index: 0,
    }];
    intent.asset_policies[0].palette_family = Some("environment".into());
    intent.scene_previews[0].size = [640, 360];
    intent
}

#[test]
fn v2_intent_round_trips_and_validates() {
    let intent = modern_v2_intent();
    assert_eq!(intent.validate(), Vec::<String>::new());
    let text = serde_json::to_string(&intent).unwrap();
    let back: VisualIntent = serde_json::from_str(&text).unwrap();
    assert!(back.is_v2() && back.modern.is_some());
}

#[test]
fn v1_keeps_old_scene_limit_and_rejects_v2_fields() {
    let mut intent = valid_intent();
    intent.scene_previews[0].size = [640, 360];
    assert!(intent.validate().iter().any(|e| e.contains("1..=256")));
    let mut intent = valid_intent();
    intent.asset_policies[0].palette_family = Some("environment".into());
    assert!(intent.validate().iter().any(|e| e.contains("visual-intent/v2")));
    let v1_text = serde_json::to_string(&valid_intent()).unwrap();
    assert!(!v1_text.contains("modern") && !v1_text.contains("palette_famil"));
}

#[test]
fn v2_rejects_unknown_family_and_bad_budget() {
    let mut intent = modern_v2_intent();
    intent.asset_policies[0].palette_family = Some("missing".into());
    assert!(intent.validate().iter().any(|e| e.contains("unknown palette family")));
    let mut intent = modern_v2_intent();
    intent.asset_policies[0].palette_family = None;
    assert!(intent.validate().iter().any(|e| e.contains("must name a palette family")));
    let mut intent = modern_v2_intent();
    intent.modern.as_mut().unwrap().budgets.frame_opaque_colors = 48;
    assert!(intent.validate().iter().any(|e| e.starts_with("modern:")));
}
