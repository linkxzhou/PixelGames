use super::*;

#[test]
fn style_warnings_only_block_in_strict_mode() {
    let mut f = solid();
    f.set(3, 3, 0);
    let i = input(f);
    let normal = validate(&i, "default").unwrap();
    assert!(normal.ok());
    assert!(!check(&normal, "frame.base.holes").pass);
    let strict = validate(&i, "strict").unwrap();
    assert!(check(&strict, "frame.base.holes").failed());
    assert!(!strict.ok());
}

#[test]
fn metadata_does_not_choose_subject_rules() {
    let mut i = input(solid());
    let expected = validate(&i, "default").unwrap().checks;
    for kind in ["sprite", "parts", "effect", "ui", "overlay", "custom"] {
        i.asset_kinds.insert("owner".into(), kind.into());
        i.frame_assets.insert("base".into(), "owner".into());
        assert_eq!(expected, validate(&i, "default").unwrap().checks);
    }
}

#[test]
fn generic_profiles_reject_old_subject_names() {
    for name in ["tank", "character", "typo"] {
        assert!(validate(&input(solid()), name).is_err());
    }
}

#[test]
fn json_config_is_strict_and_roundtrips() {
    let config: QaConfig =
        serde_json::from_str(r#"{"strict":true,"max_holes":null,"max_samples":3}"#).unwrap();
    assert!(config.strict);
    assert_eq!(config.max_holes, None);
    assert!(serde_json::from_str::<QaConfig>(r#"{"max_holse":2}"#).is_err());
    let json = serde_json::to_string(&config).unwrap();
    assert_eq!(
        serde_json::from_str::<QaConfig>(&json).unwrap().max_samples,
        3
    );
}

#[test]
fn invalid_config_is_rejected_before_analysis() {
    for c in [
        QaConfig {
            max_coverage_bps: Some(10001),
            ..QaConfig::default()
        },
        QaConfig {
            min_coverage_bps: Some(9000),
            max_coverage_bps: Some(1000),
            ..QaConfig::default()
        },
        QaConfig {
            max_samples: 1025,
            ..QaConfig::default()
        },
        QaConfig {
            max_seam_channel_delta: 256,
            ..QaConfig::default()
        },
    ] {
        assert!(validate_with_config(&input(solid()), &c).is_err());
    }
}

#[test]
fn thresholds_can_be_disabled_without_disabling_integrity() {
    let mut f = solid();
    f.set(3, 3, 0);
    let c = QaConfig {
        strict: true,
        max_holes: None,
        ..QaConfig::default()
    };
    assert!(validate_with_config(&input(f.clone()), &c).unwrap().ok());
    f.set(3, 3, 250);
    assert!(!validate_with_config(&input(f), &c).unwrap().ok());
}

#[test]
fn sparse_and_full_canvas_are_not_arbitrarily_rejected() {
    let mut f = IndexFrame::new(32, 8).unwrap();
    for x in (1..31).step_by(3) {
        f.set(x, 3, 2);
    }
    assert!(validate(&input(f), "default").unwrap().ok());
    assert!(
        validate(&input(IndexFrame::filled(8, 8, 2).unwrap()), "default")
            .unwrap()
            .ok()
    );
    assert!(!validate(&input(solid().blank_like()), "default")
        .unwrap()
        .ok());
}

#[test]
fn explicit_coverage_and_border_limits_apply() {
    let c = QaConfig {
        strict: true,
        min_coverage_bps: Some(5000),
        max_border_pixels: Some(0),
        ..QaConfig::default()
    };
    assert!(check(
        &validate_with_config(&input(solid()), &c).unwrap(),
        "frame.base.coverage"
    )
    .failed());
    assert!(check(
        &validate_with_config(&input(IndexFrame::filled(8, 8, 2).unwrap()), &c).unwrap(),
        "frame.base.border"
    )
    .failed());
}

#[test]
fn outline_and_duplicate_palette_style_limits_can_be_disabled() {
    let mut f = solid();
    f.set(2, 2, 1);
    let mut i = input(f);
    i.palette.colors[3] = i.palette.colors[2];
    let config = QaConfig {
        strict: true,
        max_outline_gaps: None,
        max_duplicate_colors: None,
        ..QaConfig::default()
    };
    assert!(validate_with_config(&i, &config).unwrap().ok());
}

#[test]
fn diagnostics_are_serializable_and_keep_failed_warning_truthful() {
    let mut f = solid();
    f.set(3, 3, 0);
    let r = validate(&input(f), "default").unwrap();
    let v = serde_json::to_value(check(&r, "frame.base.holes")).unwrap();
    assert_eq!(v["severity"], "warning");
    assert_eq!(v["pass"], false);
    assert_eq!(v["samples"][0]["x"], 3);
}
