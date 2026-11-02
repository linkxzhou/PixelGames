use super::*;
use alloc::string::{String, ToString};
use pxl_core::{Rgba, SizeTier};

fn pal() -> Palette {
    let mut p = Palette {
        id: "t".into(),
        colors: alloc::vec![
            Rgba::TRANSPARENT,
            Rgba::rgb(24, 20, 30),
            Rgba::rgb(200, 90, 60),
            Rgba::rgb(190, 110, 70),
            Rgba::rgb(90, 160, 80),
        ],
        ramps: Default::default(),
        cycles: Default::default(),
        special: pxl_core::Special::default(),
    };
    p.special.outline = 1;
    p.special.flash = 1;
    p
}

fn disc(n: i32) -> IndexFrame {
    let mut f = IndexFrame::new(n as u16, n as u16).unwrap();
    let r = n / 2 - 2;
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x - n / 2, y - n / 2);
            let d = dx * dx + dy * dy;
            if d <= r * r {
                f.set(x, y, if dx < 0 { 2 } else { 3 });
            } else if d <= (r + 1) * (r + 1) {
                f.set(x, y, 1);
            }
        }
    }
    f
}

#[test]
fn every_tier_finishes_without_breaking_the_palette_or_silhouette() {
    for tier in SizeTier::ALL {
        let p = SizeProfile::of(tier);
        let n = p.side().min(64) as i32;
        let f = disc(n);
        let out = finish(&f, &pal(), &FinishOptions::for_profile(&p)).unwrap();
        out.palette.validate().unwrap();
        assert!(out.palette.colors.len() <= p.max_colors.max(pal().colors.len()));
        for &px in &out.frame.pixels {
            assert!((px as usize) < out.palette.colors.len());
        }
        let before = f.count_opaque() as i32;
        let after = out.frame.count_opaque() as i32;
        assert!(
            (before - after).abs() * 10 < before,
            "{tier:?}: {before} -> {after}"
        );
        let again = finish(&f, &pal(), &FinishOptions::for_profile(&p)).unwrap();
        assert_eq!(again.frame, out.frame, "{tier:?} deterministic");
    }
}

#[test]
fn close_colours_get_an_inner_line() {
    let f = disc(40);
    let mut opts = FinishOptions::for_profile(&SizeProfile::of(SizeTier::S128));
    opts.shade = ShadeMode::None;
    opts.aa = AaPolicy::Off;
    let out = finish(&f, &pal(), &opts).unwrap();
    assert!(out.report.inner_lines > 0, "{:?}", out.report);
    let off = FinishOptions {
        inner_lines: false,
        ..opts
    };
    assert_eq!(finish(&f, &pal(), &off).unwrap().report.inner_lines, 0);
}

#[test]
fn the_32_tier_adds_no_anti_aliasing() {
    let f = disc(32);
    let out = finish(
        &f,
        &pal(),
        &FinishOptions::for_profile(&SizeProfile::of(SizeTier::S32)),
    )
    .unwrap();
    assert_eq!(out.report.aa, 0);
    assert!(!out.report.stages.contains(&"aa"));
}

#[test]
fn protected_pixels_are_left_alone() {
    let rows: Vec<String> = [
        "........", ".111111.", ".122221.", ".124221.", ".122221.", ".111111.", "........",
        "........",
    ]
    .iter()
    .map(|r| r.to_string())
    .collect();
    let f = pxl_core::decode_matrix(&rows).unwrap();
    let mut protect = alloc::vec![false; 64];
    protect[3 * 8 + 3] = true;
    let opts = FinishOptions {
        protect: Some(protect),
        ..FinishOptions::for_profile(&SizeProfile::of(SizeTier::S64))
    };
    let out = finish(&f, &pal(), &opts).unwrap();
    assert_eq!(out.frame.get(3, 3), 4, "the single eye pixel survives");
}

#[test]
fn a_misaligned_protect_mask_is_a_structural_error() {
    let f = disc(16);
    let opts = FinishOptions {
        protect: Some(alloc::vec![true; 3]),
        ..FinishOptions::for_profile(&SizeProfile::of(SizeTier::S32))
    };
    let error = finish(&f, &pal(), &opts).unwrap_err();
    assert!(error.message.contains("protect mask"), "{error:?}");
}

#[test]
fn canonical_materials_keep_their_response() {
    assert_eq!(Material::parse("wood"), Some(Material::Wood));
    assert_eq!(Material::parse("stone"), Some(Material::Stone));
    assert_eq!(Material::parse("water"), Some(Material::Water));
    assert_eq!(Material::parse("foliage"), Some(Material::Foliage));
    assert!(Material::Stone.keeps_corners());
    assert!(!Material::Water.keeps_corners());
    assert_eq!(Material::parse("flesh"), None);
}
