use crate::{
    validate, validate_with_config, AnimSeq, Check, QaConfig, Report, Severity, ValidateInput,
};
use pxl_core::{IndexFrame, Palette, Rgba, Special};
use std::collections::BTreeMap;

mod animation;
mod profiles;
mod rules;

fn pal() -> Palette {
    Palette {
        id: "test".into(),
        colors: vec![
            Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            Rgba::rgb(10, 10, 10),
            Rgba::rgb(80, 80, 80),
            Rgba::rgb(200, 200, 200),
        ],
        ramps: BTreeMap::new(),
        cycles: BTreeMap::new(),
        special: Special {
            transparent: 0,
            outline: 1,
            flash: 3,
        },
    }
}

fn input(frame: IndexFrame) -> ValidateInput {
    ValidateInput {
        palette: pal(),
        frames: vec![("base".into(), frame)],
        anims: vec![],
        facings: BTreeMap::new(),
        tiles: vec![],
        tile_ramp: String::new(),
        tile_ramps: BTreeMap::new(),
        asset_kinds: BTreeMap::new(),
        frame_assets: BTreeMap::new(),
    }
}

fn solid() -> IndexFrame {
    let mut f = IndexFrame::new(8, 8).unwrap();
    for y in 2..6 {
        for x in 2..6 {
            f.set(x, y, 2);
        }
    }
    f
}

fn check<'a>(report: &'a Report, id: &str) -> &'a Check {
    report
        .checks
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("missing {id}: {:?}", report.checks))
}

fn sequence(frames: Vec<IndexFrame>) -> ValidateInput {
    let mut v = input(solid());
    v.frames.clear();
    v.anims.push(AnimSeq {
        name: "sequence".into(),
        asset: "owner".into(),
        base_name: "arbitrary".into(),
        dir: None,
        looped: false,
        frame_names: (0..frames.len()).map(|i| format!("f{i}")).collect(),
        static_mode: false,
        frames,
        ..Default::default()
    });
    v
}

#[test]
fn invalid_pixel_reports_location() {
    let mut f = solid();
    f.set(3, 4, 250);
    let report = validate(&input(f), "default").unwrap();
    let c = check(&report, "frame.base.palette");
    assert!(c.failed());
    assert_eq!((c.samples[0].x, c.samples[0].y), (3, 4));
}

#[test]
fn malformed_buffers_and_zero_dimensions_never_panic() {
    for (w, h, len) in [
        (0, 2, 0),
        (2, 0, 0),
        (2, 2, 3),
        (2, 2, 5),
        (65535, 65535, 0),
    ] {
        let report = validate(
            &input(IndexFrame {
                w,
                h,
                pixels: vec![2; len],
            }),
            "default",
        )
        .unwrap();
        assert!(!report.ok());
        assert!(check(&report, "frame.base.buffer").failed());
    }
}

#[test]
fn pixel_budgets_fail_before_analysis() {
    let c = QaConfig {
        max_total_pixels: 1,
        ..QaConfig::default()
    };
    assert!(check(
        &validate_with_config(&input(solid()), &c).unwrap(),
        "input.pixel_budget"
    )
    .failed());
    let c = QaConfig {
        max_frame_pixels: 1,
        ..QaConfig::default()
    };
    assert!(check(
        &validate_with_config(&input(solid()), &c).unwrap(),
        "frame.base.buffer"
    )
    .failed());
}

#[test]
fn empty_input_is_not_a_success() {
    let mut i = input(solid());
    i.frames.clear();
    assert!(!validate(&i, "default").unwrap().ok());
}

#[test]
fn invalid_palette_structures_fail() {
    let mut i = input(solid());
    i.palette.colors[2].a = 128;
    assert!(check(&validate(&i, "default").unwrap(), "palette.alpha").failed());
    i.palette = pal();
    i.palette.special.transparent = 1;
    assert!(check(&validate(&i, "default").unwrap(), "palette.transparent").failed());
    i.palette = pal();
    i.palette.ramps.insert("arbitrary".into(), vec![2, 2, 255]);
    assert!(check(
        &validate(&i, "default").unwrap(),
        "palette.ramp.arbitrary.indices"
    )
    .failed());
    i.palette = pal();
    i.palette.cycles.insert(
        "cycle".into(),
        pxl_core::Cycle {
            indices: vec![],
            period: 0,
        },
    );
    let r = validate(&i, "default").unwrap();
    assert!(check(&r, "palette.cycle.cycle.indices").failed());
    assert!(check(&r, "palette.cycle.cycle.period").failed());
}

#[test]
fn palette_size_and_extra_transparency_are_checked() {
    for n in [0, 257] {
        let mut i = input(solid());
        i.palette.colors.resize(n, Rgba::rgb(1, 1, 1));
        assert!(check(&validate(&i, "default").unwrap(), "palette.size").failed());
    }
    let mut i = input(solid());
    i.palette.colors[2].a = 0;
    assert!(check(&validate(&i, "default").unwrap(), "palette.alpha").failed());
}

#[test]
fn duplicate_rgba_is_a_style_warning() {
    let mut i = input(solid());
    i.palette.colors[3] = i.palette.colors[2];
    let r = validate(&i, "default").unwrap();
    assert!(!check(&r, "palette.duplicates").pass);
    assert!(r.ok());
}

#[test]
fn duplicate_frame_identity_is_rejected() {
    let mut i = input(solid());
    i.frames.push(i.frames[0].clone());
    assert!(check(&validate(&i, "default").unwrap(), "frame.base.unique").failed());
}

#[test]
fn invalid_tile_and_reference_buffers_never_panic() {
    let f = IndexFrame {
        w: 3,
        h: 3,
        pixels: vec![2],
    };
    let mut i = input(solid());
    i.tiles.push(("t".into(), f.clone()));
    i.facings.insert("custom".into(), f);
    let r = validate(&i, "default").unwrap();
    assert!(check(&r, "tile.t.buffer").failed());
    assert!(check(&r, "facing.custom.buffer").failed());
}

#[test]
fn sample_limit_is_enforced() {
    let mut f = solid();
    f.pixels.fill(255);
    let c = QaConfig {
        max_samples: 2,
        ..QaConfig::default()
    };
    let r = validate_with_config(&input(f), &c).unwrap();
    assert_eq!(check(&r, "frame.base.palette").samples.len(), 2);
    assert!(r.checks.iter().all(|c| c.samples.len() <= 2));
}

#[test]
fn metadata_budget_and_tile_identity_are_checked() {
    let mut i = input(solid());
    let config = QaConfig {
        max_entries: 1,
        ..QaConfig::default()
    };
    i.anims = sequence(vec![solid()]).anims;
    assert!(check(
        &validate_with_config(&i, &config).unwrap(),
        "input.entry_budget"
    )
    .failed());
    i.anims.clear();
    i.tiles
        .push(("base".into(), IndexFrame::filled(8, 8, 2).unwrap()));
    assert!(check(&validate(&i, "default").unwrap(), "tile.base.identity").failed());
}

#[test]
fn report_is_deterministic() {
    let i = input(solid());
    assert_eq!(
        validate(&i, "default").unwrap().checks,
        validate(&i, "default").unwrap().checks
    );
}
