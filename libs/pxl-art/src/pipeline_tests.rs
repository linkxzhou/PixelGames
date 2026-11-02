use super::*;
use alloc::string::ToString;

fn frame(rows: &[&str]) -> IndexFrame {
    let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
    pxl_core::decode_matrix(&rows).unwrap()
}

fn palette() -> Palette {
    let mut p = Palette {
        id: "t".into(),
        colors: alloc::vec![
            Rgba::TRANSPARENT,
            Rgba::rgb(32, 32, 40),
            Rgba::rgb(232, 64, 64),
            Rgba::rgb(250, 200, 80),
        ],
        ramps: Default::default(),
        cycles: Default::default(),
        special: pxl_core::Special::default(),
    };
    p.special.outline = 1;
    p.special.flash = 1;
    p
}

fn sprite() -> IndexFrame {
    frame(&[
        "00000000", "00111100", "01222210", "01223210", "01222210", "01222210", "00111100",
        "00000000",
    ])
}

#[test]
fn output_size_is_native_times_scale_and_no_colour_leaves_the_palette() {
    let f = sprite();
    let pal = palette();
    for (mode, k) in [
        (Upscale::Copy, 2),
        (Upscale::Hybrid, 2),
        (Upscale::Hybrid, 3),
        (Upscale::Contour, 4),
        (Upscale::Scale3x, 3),
        (Upscale::Xbr, 2),
        (Upscale::Mmpx, 2),
        (Upscale::Mmpx, 4),
    ] {
        let opts = EnhanceOptions {
            scale: k,
            upscale: mode,
            ..EnhanceOptions::default()
        };
        let out = enhance(&f, &pal, &opts).unwrap();
        assert_eq!((out.frame.w, out.frame.h), (f.w * k, f.h * k), "{mode:?}");
        out.palette.validate().unwrap();
        for &p in &out.frame.pixels {
            assert!((p as usize) < out.palette.colors.len(), "{mode:?}: {p}");
        }
    }
}

#[test]
fn the_same_input_gives_the_same_bytes() {
    let f = sprite();
    let pal = palette();
    let opts = EnhanceOptions::default();
    let a = enhance(&f, &pal, &opts).unwrap();
    let b = enhance(&f, &pal, &opts).unwrap();
    assert_eq!(a.frame, b.frame);
    assert_eq!(a.palette, b.palette);
    assert_eq!(a.report.metrics, b.report.metrics);
}

#[test]
fn enhance_to_chains_to_8x() {
    let f = sprite();
    let out = enhance_to(&f, &palette(), 8, &EnhanceOptions::default()).unwrap();
    assert_eq!((out.frame.w, out.frame.h), (64, 64));
    assert_eq!(out.report.scale, 8);
    out.palette.validate().unwrap();
    assert!(enhance_to(&f, &palette(), 5, &EnhanceOptions::default()).is_err());
}

#[test]
fn keep_with_copy_is_the_plain_nearest_scale() {
    let f = sprite();
    let opts = EnhanceOptions {
        scale: 2,
        upscale: Upscale::Copy,
        cleanup: CleanOptions::NONE,
        outline: OutlineMode::Keep,
        ..EnhanceOptions::default()
    };
    let out = enhance(&f, &palette(), &opts).unwrap();
    assert_eq!(out.frame, pxl_core::scale_copy(&f, 2).unwrap());
    assert_eq!(out.palette, palette(), "no ramps are generated");
    assert_eq!(out.report.metrics.new_info_bps, 0);
    assert_eq!(out.report.metrics.copy2x_bps, Some(10_000));
}

#[test]
fn a_redrawn_outline_is_one_pixel_thick() {
    let f = sprite();
    let opts = EnhanceOptions {
        scale: 3,
        upscale: Upscale::Copy,
        cleanup: CleanOptions::NONE,
        outline: OutlineMode::Flat,
        ..EnhanceOptions::default()
    };
    let out = enhance(&f, &palette(), &opts).unwrap();
    assert!(out.report.stripped > 0);
    // Native row 1 holds outline at x 2..=5; scaled to 3x the fill starts
    // at row 6, and the ring is the single row above it.
    assert_eq!(out.frame.get(8, 5), 1);
    assert_eq!(out.frame.get(8, 4), 0, "no second outline row");
}

#[test]
fn selout_adds_ramp_colours_without_breaking_the_palette() {
    let f = sprite();
    let out = enhance(&f, &palette(), &EnhanceOptions::default()).unwrap();
    assert!(out.palette.colors.len() > palette().colors.len());
    assert!(!out.report.ramps.generated.is_empty());
    assert!(out.report.stages.contains(&"outline"));
    out.palette.validate().unwrap();
}

#[test]
fn an_unsupported_scale_names_the_fix() {
    let opts = EnhanceOptions {
        scale: 3,
        upscale: Upscale::Epx,
        ..EnhanceOptions::default()
    };
    let err = enhance(&sprite(), &palette(), &opts).unwrap_err();
    assert!(err.message.contains("epx"), "{}", err.message);
    assert!(err.hint.is_some());
}

#[test]
fn a_wrong_sized_material_list_is_rejected() {
    let opts = EnhanceOptions {
        materials: Some(alloc::vec![Material::Metal; 3]),
        ..EnhanceOptions::default()
    };
    assert!(enhance(&sprite(), &palette(), &opts).is_err());
}

#[test]
fn a_small_feature_survives_scaling() {
    let f = sprite();
    let out = enhance(&f, &palette(), &EnhanceOptions::default()).unwrap();
    assert_eq!(out.report.metrics.small_feature_kept_bps, 10_000);
}

fn big_sprite() -> IndexFrame {
    let mut rows: Vec<String> = Vec::new();
    for y in 0..14 {
        let row: String = (0..14)
            .map(|x| {
                let edge = x == 0 || y == 0 || x == 13 || y == 13;
                if edge {
                    '1'
                } else {
                    '2'
                }
            })
            .collect();
        rows.push(row);
    }
    pxl_core::decode_matrix(&rows).unwrap()
}

#[test]
fn shading_is_off_by_default_and_changes_nothing() {
    let f = big_sprite();
    let off = enhance(&f, &palette(), &EnhanceOptions::default()).unwrap();
    assert_eq!(off.report.shade, ShadeReport::default());
    assert!(!off.report.stages.contains(&"shade"));
}

#[test]
fn volume_shading_adds_a_lit_and_a_shaded_side_and_raises_the_value_range() {
    let f = big_sprite();
    let flat = enhance(&f, &palette(), &EnhanceOptions::default()).unwrap();
    let opts = EnhanceOptions {
        shade: ShadeMode::Volume,
        ..EnhanceOptions::default()
    };
    let lit = enhance(&f, &palette(), &opts).unwrap();
    assert!(lit.report.stages.contains(&"shade"));
    assert!(
        lit.report.shade.lit > 0 && lit.report.shade.shaded > 0,
        "{:?}",
        lit.report.shade
    );
    assert!(lit.report.metrics.value_range >= flat.report.metrics.value_range);
    assert!(lit.report.metrics.ramp_coverage_bps >= flat.report.metrics.ramp_coverage_bps);
    lit.palette.validate().unwrap();
    for &p in &lit.frame.pixels {
        assert!((p as usize) < lit.palette.colors.len());
    }
    let again = enhance(&f, &palette(), &opts).unwrap();
    assert_eq!(again.frame, lit.frame);
}

#[test]
fn shading_never_changes_the_silhouette() {
    let f = big_sprite();
    let base = enhance(&f, &palette(), &EnhanceOptions::default()).unwrap();
    let shaded = enhance(
        &f,
        &palette(),
        &EnhanceOptions {
            shade: ShadeMode::Volume,
            ..EnhanceOptions::default()
        },
    )
    .unwrap();
    for (a, b) in base.frame.pixels.iter().zip(&shaded.frame.pixels) {
        assert_eq!(*a == 0, *b == 0, "opacity must match");
    }
}
