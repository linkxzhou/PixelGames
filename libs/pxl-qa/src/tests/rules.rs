use super::*;

#[test]
fn pixel_ratio_limits_use_exact_counts() {
    let mut f = IndexFrame::canvas(128, 128).unwrap();
    f.set(64, 64, 2);
    let config = QaConfig {
        max_coverage_bps: Some(0),
        max_checkerboard_bps: Some(0),
        ..QaConfig::default()
    };
    assert!(
        !check(
            &validate_with_config(&input(f.clone()), &config).unwrap(),
            "frame.base.coverage"
        )
        .pass
    );
    f.set(65, 64, 3);
    f.set(64, 65, 3);
    f.set(65, 65, 2);
    assert!(
        !check(
            &validate_with_config(&input(f), &config).unwrap(),
            "frame.base.checkerboard_bps"
        )
        .pass
    );
    assert!(crate::common::ratio_at_most(1, 10000, 1));
    assert!(!crate::common::ratio_at_most(2, 10001, 1));
    assert!(!crate::common::ratio_at_least(1, 10001, 1));
}

#[test]
fn holes_are_enclosed_transparent_components_not_exterior() {
    let mut f = solid();
    f.set(3, 3, 0);
    f.set(4, 3, 0);
    let r = validate(&input(f.clone()), "default").unwrap();
    assert_eq!(check(&r, "frame.base.holes").value, "1");
    f.set(3, 2, 0);
    assert_eq!(
        check(&validate(&input(f), "default").unwrap(), "frame.base.holes").value,
        "0"
    );
}

#[test]
fn fragmentation_and_singletons_report_positions() {
    let mut f = solid();
    f.set(0, 0, 2);
    f.set(7, 7, 3);
    let r = validate(&input(f), "default").unwrap();
    assert_eq!(check(&r, "frame.base.components").value, "3");
    assert_eq!(check(&r, "frame.base.isolated").value, "2");
    assert_eq!(check(&r, "frame.base.isolated").samples.len(), 2);
}

#[test]
fn checkerboard_pattern_is_detected_without_forbidding_dithering() {
    let mut f = IndexFrame::new(8, 8).unwrap();
    for y in 0..8 {
        for x in 0..8 {
            f.set(x, y, if (x + y) % 2 == 0 { 2 } else { 3 });
        }
    }
    let r = validate(&input(f), "default").unwrap();
    assert_eq!(check(&r, "frame.base.checkerboard_bps").value, "10000");
    assert!(!check(&r, "frame.base.checkerboard_bps").pass);
    assert!(r.ok());
}

#[test]
fn low_contrast_neighbors_are_localized() {
    let mut f = solid();
    f.set(3, 3, 3);
    let mut i = input(f);
    i.palette.colors[3] = pxl_core::Rgba::rgb(81, 81, 81);
    let r = validate(&i, "default").unwrap();
    assert!(!check(&r, "frame.base.low_contrast").pass);
    assert!(!check(&r, "frame.base.low_contrast").samples.is_empty());
}

#[test]
fn exposed_outline_is_a_heuristic_not_an_assumed_art_style() {
    let mut f = solid();
    f.set(2, 2, 1);
    let r = validate(&input(f), "default").unwrap();
    assert!(!check(&r, "frame.base.outline_gaps").pass);
    assert!(r.ok());
}

fn tiles(f: IndexFrame) -> ValidateInput {
    let mut i = input(solid());
    i.frames.clear();
    i.tiles.push(("tile".into(), f));
    i
}

#[test]
fn one_tile_is_enough_and_no_named_ramp_is_required() {
    assert!(
        validate(&tiles(IndexFrame::filled(4, 4, 2).unwrap()), "strict")
            .unwrap()
            .ok()
    );
}

#[test]
fn tile_transparent_border_counts_corners_only_once() {
    let mut f = IndexFrame::filled(4, 4, 2).unwrap();
    for x in 0..4 {
        f.set(x, 0, 0);
    }
    for y in 0..4 {
        f.set(0, y, 0);
    }
    let r = validate(&tiles(f), "default").unwrap();
    assert_eq!(
        check(&r, "tile.tile.full_bleed").value,
        "7 clear edge pixels"
    );
    assert!(check(&r, "tile.tile.full_bleed").failed());
}

#[test]
fn both_seam_axes_and_alpha_are_checked() {
    let mut f = IndexFrame::filled(4, 4, 2).unwrap();
    f.set(0, 0, 3);
    let r = validate(&tiles(f), "default").unwrap();
    assert!(!check(&r, "tile.tile.seam_x").pass);
    assert!(!check(&r, "tile.tile.seam_y").pass);
    let mut f = IndexFrame::filled(4, 4, 2).unwrap();
    f.set(0, 1, 0);
    assert!(!check(&validate(&tiles(f), "default").unwrap(), "tile.tile.seam_x").pass);
}

#[test]
fn layer_boundaries_are_not_silently_exempted() {
    let mut f = IndexFrame::filled(4, 4, 2).unwrap();
    for x in 0..4 {
        f.set(x, 3, 3);
    }
    let i = tiles(f);
    assert!(!check(&validate(&i, "default").unwrap(), "tile.tile.seam_y").pass);
    let c = QaConfig {
        tile_vertical: false,
        ..QaConfig::default()
    };
    let r = validate_with_config(&i, &c).unwrap();
    assert!(r.checks.iter().all(|c| c.id != "tile.tile.seam_y"));
    assert!(check(&r, "tile.tile.seam_x").pass);
}

#[test]
fn ramp_distance_and_rgb_fallback_do_not_skip_other_colors() {
    let mut f = IndexFrame::filled(4, 4, 2).unwrap();
    f.set(0, 0, 3);
    let mut i = tiles(f);
    i.tile_ramp = "custom".into();
    i.palette.ramps.insert("custom".into(), vec![2, 3]);
    assert!(check(&validate(&i, "default").unwrap(), "tile.tile.seam_x").pass);
    i.palette.ramps.insert("custom".into(), vec![1]);
    assert!(!check(&validate(&i, "default").unwrap(), "tile.tile.seam_x").pass);
}

#[test]
fn one_pixel_tiles_and_thin_canvases_are_safe() {
    for (w, h) in [(1, 1), (1, 8), (8, 1)] {
        let i = tiles(IndexFrame::filled(w, h, 2).unwrap());
        assert!(validate(&i, "default").unwrap().ok());
    }
}

#[test]
fn all_small_binary_masks_are_safe_and_have_bounded_metrics() {
    for mask in 0..512u16 {
        let mut f = IndexFrame::new(3, 3).unwrap();
        for i in 0..9 {
            f.pixels[i] = if mask & (1 << i) == 0 { 0 } else { 2 };
        }
        let r = validate(&input(f), "default").unwrap();
        let components: usize = check(&r, "frame.base.components").value.parse().unwrap();
        let holes: usize = check(&r, "frame.base.holes").value.parse().unwrap();
        assert!(components <= 5);
        assert!(holes <= 1);
        assert!(r
            .checks
            .iter()
            .all(|c| c.samples.iter().all(|p| p.x < 3 && p.y < 3)));
    }
}

fn resolution_input(frame: IndexFrame) -> ValidateInput {
    input(frame)
}

/// A deliberate 16x16 sprite is legal: resolution checks are measurements, not
/// gates, and nothing here penalises a small canvas.
#[test]
fn a_small_native_sprite_passes_resolution_diagnostics_by_default() {
    let mut f = IndexFrame::new(16, 16).unwrap();
    for y in 3..13 {
        for x in 3..13 {
            f.set(x, y, 2);
        }
    }
    let report = validate_with_config(&resolution_input(f), &QaConfig::default()).unwrap();
    assert!(report.ok(), "{:?}", report.checks);
    let uniform = check(&report, "frame.base.uniform_area_bps");
    assert_eq!(uniform.severity, Severity::Info);
    assert!(uniform.value.parse::<u32>().unwrap() > 0);
    let blocks = check(&report, "frame.base.block_corners");
    assert_eq!(blocks.severity, Severity::Info);
}

/// A 2x2 pixel block in a frame whose pixels are otherwise unique-shaped: the
/// measurement must actually see the blocks, unlike a coverage-only check.
#[test]
fn block_corners_count_uniform_squares_in_the_same_colour() {
    let mut f = IndexFrame::new(16, 16).unwrap();
    for y in 2..14 {
        for x in 2..14 {
            // Checkerboard: no two 4-adjacent pixels share a colour.
            f.set(x, y, if (x + y) % 2 == 0 { 2 } else { 3 });
        }
    }
    let report = validate_with_config(&resolution_input(f), &QaConfig::default()).unwrap();
    assert_eq!(check(&report, "frame.base.block_corners").value, "0");
    assert_eq!(check(&report, "frame.base.uniform_area_bps").value, "0");

    let mut solid = IndexFrame::new(16, 16).unwrap();
    for y in 2..14 {
        for x in 2..14 {
            solid.set(x, y, 2);
        }
    }
    let report = validate_with_config(&resolution_input(solid), &QaConfig::default()).unwrap();
    assert_eq!(check(&report, "frame.base.block_corners").value, "121");
    assert_eq!(check(&report, "frame.base.uniform_area_bps").value, "10000");
}

/// A canvas grown without growing the art shows up as low coverage; the check
/// is off by default and only warns once a caller asks for it.
#[test]
fn content_coverage_warns_when_the_canvas_outgrows_the_subject() {
    let f = solid();
    let off = validate_with_config(&resolution_input(f.clone()), &QaConfig::default()).unwrap();
    assert!(off
        .checks
        .iter()
        .all(|c| !c.id.ends_with(".content_coverage")));

    let config = QaConfig {
        min_content_coverage_bps: Some(3000),
        ..QaConfig::default()
    };
    let report = validate_with_config(&resolution_input(f), &config).unwrap();
    let coverage = check(&report, "frame.base.content_coverage");
    assert!(
        !coverage.pass,
        "16/64 px of an 8x8 canvas is under 3000 bps"
    );
    assert_eq!(coverage.severity, Severity::Warning);
    assert!(report.ok(), "a style warning does not block publishing");

    let strict = QaConfig {
        strict: true,
        ..config
    };
    let report = validate_with_config(&resolution_input(solid()), &strict).unwrap();
    assert!(!report.ok(), "strict turns the warning into a failure");
}

#[test]
fn copy2x_is_a_measurement_and_an_exact_copy_scores_full() {
    // Every pixel doubled: the frame is a plain 2x, so every aligned block is
    // uniform. It is still a legal frame; the check is Info, never a failure.
    let mut frame = IndexFrame::new(8, 8).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            let c = if (x + y) % 2 == 0 { 2 } else { 3 };
            for dy in 0..2 {
                for dx in 0..2 {
                    frame.set(x * 2 + dx, y * 2 + dy, c);
                }
            }
        }
    }
    let report = validate(&input(frame.clone()), "default").unwrap();
    let copy = check(&report, "frame.base.copy2x_bps");
    assert_eq!(copy.value, "10000");
    assert_eq!(copy.severity, Severity::Info);
    assert!(report.ok());

    // A single changed pixel inside one block breaks that block only.
    frame.set(0, 0, 3);
    let report = validate(&input(frame), "default").unwrap();
    assert_eq!(
        check(&report, "frame.base.copy2x_bps").value,
        "9375",
        "1 of 16 aligned blocks is no longer uniform"
    );
}

#[test]
fn copy2x_is_skipped_when_the_sides_are_odd() {
    let mut frame = IndexFrame::new(9, 9).unwrap();
    frame.set(4, 4, 2);
    let report = validate(&input(frame), "default").unwrap();
    assert!(
        !report
            .checks
            .iter()
            .any(|c| c.id == "frame.base.copy2x_bps"),
        "no aligned 2x2 grid exists on odd sides"
    );
}
