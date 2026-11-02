use super::*;

#[test]
fn explicit_anchor_and_regions_accept_local_motion_and_reject_violations() {
    let a = solid();
    let mut local = a.clone();
    local.set(3, 3, 3);
    let mut i = sequence(vec![a.clone(), local]);
    i.anims[0].anchor = Some([3, 5]);
    i.anims[0].anchor_tolerance_px = Some(0);
    i.anims[0].protected_regions = vec![pxl_core::Rect::new(2, 2, 1, 1)];
    i.anims[0].change_regions = vec![pxl_core::Rect::new(2, 2, 4, 4)];
    i.anims[0].allow_duplicate_frames = Some(true);
    let report = validate(&i, "default").unwrap();
    assert!(report.ok(), "{:?}", report.checks);
    assert!(check(&report, "anim.sequence.anchor_1").pass);
    assert!(check(&report, "anim.sequence.protected_1_2_2").pass);
    assert!(check(&report, "anim.sequence.change_region_1").pass);

    let mut outside = a.clone();
    outside.set(0, 0, 3);
    let mut bad = sequence(vec![a, outside]);
    bad.anims[0].anchor = Some([3, 5]);
    bad.anims[0].anchor_tolerance_px = Some(0);
    bad.anims[0].change_regions = vec![pxl_core::Rect::new(2, 2, 4, 4)];
    let report = validate(&bad, "default").unwrap();
    assert!(check(&report, "anim.sequence.anchor_1").failed());
    assert!(check(&report, "anim.sequence.change_region_1").failed());
}

#[test]
fn per_animation_policy_overrides_global_exemptions() {
    let i = sequence(vec![solid(), solid()]);
    let config = QaConfig {
        allow_duplicate_frames: true,
        ..QaConfig::default()
    };
    let mut restricted = i.clone();
    restricted.anims[0].allow_duplicate_frames = Some(false);
    assert!(check(
        &validate_with_config(&restricted, &config).unwrap(),
        "anim.sequence.duplicate_0_1"
    )
    .failed());

    let terminal = sequence(vec![solid(), solid().blank_like()]);
    let mut restricted = terminal.clone();
    restricted.anims[0].allow_terminal_empty = Some(false);
    assert!(check(
        &validate(&restricted, "default").unwrap(),
        "frame.f1.visible"
    )
    .failed());
}

#[test]
fn zero_limits_detect_sub_basis_point_changes() {
    let mut a = IndexFrame::canvas(128, 128).unwrap();
    a.pixels.fill(2);
    for color in [0, 3] {
        let mut b = a.clone();
        b.set(64, 64, color);
        let config = QaConfig {
            max_animation_diff_bps: Some(0),
            max_palette_change_bps: Some(0),
            max_area_change_bps: Some(0),
            ..QaConfig::default()
        };
        let report = validate_with_config(&sequence(vec![a.clone(), b]), &config).unwrap();
        assert!(!check(&report, "anim.sequence.diff_0_1").pass);
        assert!(!check(&report, "anim.sequence.palette_drift_0_1").pass);
        if color == 0 {
            assert!(!check(&report, "anim.sequence.area_0_1").pass);
        }
    }
}

#[test]
fn conflicting_versions_keep_all_pixel_diagnostics() {
    let mut bad = solid();
    bad.set(3, 4, 250);
    for reverse in [false, true] {
        for document_duplicate in [false, true] {
            let (a, b) = if reverse {
                (solid(), bad.clone())
            } else {
                (bad.clone(), solid())
            };
            let mut i = sequence(vec![b.clone()]);
            i.anims[0].static_mode = true;
            i.frames.push(("f0".into(), a));
            if document_duplicate {
                i.anims.clear();
                i.frames.push(("f0".into(), b));
            }
            let report = validate(&i, "default").unwrap();
            assert!(!report.ok());
            assert!(report.checks.iter().any(|c| c.id.ends_with(".palette")
                && c.failed()
                && c.samples.iter().any(|p| p.x == 3 && p.y == 4)));
        }
    }
}

#[test]
fn static_contract_is_explicit() {
    let mut i = sequence(vec![solid()]);
    assert!(check(&validate(&i, "default").unwrap(), "anim.sequence.frames").failed());
    i.anims[0].static_mode = true;
    assert!(validate(&i, "default").unwrap().ok());
    i.anims[0].frames.push(solid());
    i.anims[0].frame_names.push("f1".into());
    assert!(check(
        &validate(&i, "default").unwrap(),
        "anim.sequence.static_single_frame"
    )
    .failed());
}

#[test]
fn tiny_motion_is_not_lost_to_canvas_percentage_rounding() {
    let a = IndexFrame::filled(64, 64, 2).unwrap();
    let mut b = a.clone();
    b.set(32, 32, 3);
    let r = validate(&sequence(vec![a, b]), "default").unwrap();
    assert!(r.ok());
    assert!(check(&r, "anim.sequence.duplicate_0_1").pass);
}

#[test]
fn repeated_frames_fail_unless_holds_are_explicit() {
    let i = sequence(vec![solid(), solid()]);
    assert!(check(
        &validate(&i, "default").unwrap(),
        "anim.sequence.duplicate_0_1"
    )
    .failed());
    let c = QaConfig {
        allow_duplicate_frames: true,
        ..QaConfig::default()
    };
    assert!(validate_with_config(&i, &c).unwrap().ok());
}

#[test]
fn same_rgba_with_different_indices_is_a_visual_duplicate() {
    let a = solid();
    let mut b = a.clone();
    b.pixels.iter_mut().for_each(|p| {
        if *p == 2 {
            *p = 3
        }
    });
    let mut i = sequence(vec![a, b]);
    i.palette.colors[3] = i.palette.colors[2];
    assert_eq!(
        check(
            &validate(&i, "default").unwrap(),
            "anim.sequence.palette_drift_0_1"
        )
        .value,
        "0"
    );
    assert!(check(
        &validate(&i, "default").unwrap(),
        "anim.sequence.duplicate_0_1"
    )
    .failed());
}

#[test]
fn mismatched_dimensions_and_names_are_rejected() {
    let i = sequence(vec![solid(), IndexFrame::filled(4, 4, 2).unwrap()]);
    assert!(check(
        &validate(&i, "default").unwrap(),
        "anim.sequence.dimensions"
    )
    .failed());
    let mut i = sequence(vec![solid(), solid()]);
    i.anims[0].frame_names.clear();
    assert!(check(
        &validate(&i, "default").unwrap(),
        "anim.sequence.references"
    )
    .failed());
}

#[test]
fn animation_pixels_are_checked_even_without_document_frames() {
    let mut f = solid();
    f.set(0, 0, 200);
    let r = validate(&sequence(vec![solid(), f]), "default").unwrap();
    assert!(check(&r, "frame.f1.palette").failed());
}

#[test]
fn reference_identity_conflicts_are_rejected() {
    let mut i = sequence(vec![solid(), solid()]);
    let mut f = solid();
    f.set(0, 0, 3);
    i.frames.push(("f0".into(), f));
    assert!(check(&validate(&i, "default").unwrap(), "frame.f0.identity").failed());
}

#[test]
fn loop_boundary_is_checked() {
    let a = solid();
    let mut b = a.clone();
    b.set(2, 2, 3);
    let mut i = sequence(vec![a.clone(), b, a]);
    i.anims[0].looped = true;
    assert!(check(
        &validate(&i, "default").unwrap(),
        "anim.sequence.duplicate_2_0"
    )
    .failed());
}

#[test]
fn empty_terminal_is_identity_scoped() {
    let empty = solid().blank_like();
    let mut i = sequence(vec![solid(), empty.clone()]);
    assert!(validate(&i, "default").unwrap().ok());
    i.frames.push(("orphan".into(), empty.clone()));
    assert!(check(&validate(&i, "default").unwrap(), "frame.orphan.visible").failed());
    i.frames.clear();
    let mut reused = i.anims[0].clone();
    reused.name = "other".into();
    reused.looped = true;
    i.anims.push(reused);
    assert!(check(&validate(&i, "default").unwrap(), "frame.f1.visible").failed());
}

#[test]
fn terminal_exemption_can_be_disabled() {
    let i = sequence(vec![solid(), solid().blank_like()]);
    let c = QaConfig {
        allow_terminal_empty: false,
        ..QaConfig::default()
    };
    assert!(check(&validate_with_config(&i, &c).unwrap(), "frame.f1.visible").failed());
}

#[test]
fn shift_adjustment_preserves_small_translations() {
    let a = solid();
    let mut b = a.blank_like();
    for y in 0..8 {
        for x in 0..8 {
            b.set(x, y, a.get(x - 1, y));
        }
    }
    let c = QaConfig {
        max_animation_diff_bps: Some(0),
        ..QaConfig::default()
    };
    let r = validate_with_config(&sequence(vec![a, b]), &c).unwrap();
    assert!(check(&r, "anim.sequence.diff_0_1").pass);
}

#[test]
fn shift_adjustment_does_not_hide_clipping() {
    let a = IndexFrame::filled(8, 8, 2).unwrap();
    let mut b = a.clone();
    for y in 0..8 {
        b.set(0, y, 0);
    }
    let c = QaConfig {
        max_animation_diff_bps: Some(0),
        ..QaConfig::default()
    };
    assert!(
        !check(
            &validate_with_config(&sequence(vec![a, b]), &c).unwrap(),
            "anim.sequence.diff_0_1"
        )
        .pass
    );
}

#[test]
fn motion_area_centroid_and_palette_changes_are_measured() {
    let a = solid();
    let mut b = a.blank_like();
    b.set(7, 7, 3);
    let r = validate(&sequence(vec![a, b]), "default").unwrap();
    assert!(!check(&r, "anim.sequence.area_0_1").pass);
    assert!(!check(&r, "anim.sequence.centroid_0_1").pass);
    assert!(r.ok());
    let a = IndexFrame::filled(8, 8, 2).unwrap();
    let b = IndexFrame::filled(8, 8, 3).unwrap();
    let r = validate(&sequence(vec![a, b]), "default").unwrap();
    assert!(!check(&r, "anim.sequence.palette_drift_0_1").pass);
    assert!(!check(&r, "anim.sequence.diff_0_1").pass);
}

#[test]
fn every_animation_owner_is_checked() {
    let mut b = solid();
    b.set(2, 2, 3);
    let mut i = sequence(vec![solid(), b]);
    let mut second = i.anims[0].clone();
    second.name = "second_sequence".into();
    second.asset = "second".into();
    second.frame_names = vec!["s0".into(), "s1".into()];
    second.frames = vec![solid(), solid()];
    i.anims.push(second);
    assert!(check(
        &validate(&i, "default").unwrap(),
        "anim.second_sequence.duplicate_0_1"
    )
    .failed());
}
