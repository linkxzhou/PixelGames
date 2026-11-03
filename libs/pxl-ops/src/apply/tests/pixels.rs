use pxl_format::{ColorRef, Op};

use super::super::apply_op;
use super::{cx, grid, pal, text};

#[test]
fn outline_shade_dissolve_noop() {
    let pal = pal();
    let f = grid(&[".2.", "222", ".2."]);
    let outlined = apply_op(
        &f,
        &Op::Outline {
            color: ColorRef::Index(1),
            mode: "outer".into(),
            brush: "diamond".into(),
            width: 1,
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    assert_eq!(outlined.get(0, 0), 1);
    assert_eq!(outlined.get(1, 1), 2);
    assert_eq!(outlined.get(1, 0), 2);
    let shaded = apply_op(
        &f,
        &Op::Shade {
            dir: "top".into(),
            amount: 1,
            region: None,
            ramp: None,
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    assert_ne!(shaded.pixels, f.pixels);
    let same = apply_op(&f, &Op::Noop {}, &cx(&pal), 0).unwrap();
    assert_eq!(same.pixels, f.pixels);
    let gone = apply_op(
        &f,
        &Op::Dissolve {
            seed: 1,
            steps: 1,
            step: 1,
            pattern: "bayer4".into(),
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    assert_eq!(gone.count_opaque(), 0);
}

#[test]
fn dissolve_only_removes_pixels_as_steps_advance() {
    let pal = pal();
    let f = grid(&["2222", "2222", "2222", "2222"]);
    for pattern in ["bayer4", "random"] {
        let mut previous = f.count_opaque();
        let mut previous_set: Vec<bool> = f.pixels.iter().map(|p| *p != 0).collect();
        for step in 0..=5u32 {
            // Different frame names must not change which pixels go first.
            let mut c = cx(&pal);
            let name = format!("frame_{step}");
            c.frame_name = &name;
            let out = apply_op(
                &f,
                &Op::Dissolve {
                    seed: 7,
                    steps: 5,
                    step,
                    pattern: pattern.into(),
                },
                &c,
                0,
            )
            .unwrap();
            let now: Vec<bool> = out.pixels.iter().map(|p| *p != 0).collect();
            for (was, is) in previous_set.iter().zip(&now) {
                assert!(*was || !*is, "{pattern}: a pixel came back at step {step}");
            }
            assert!(out.count_opaque() <= previous, "{pattern} step {step}");
            previous = out.count_opaque();
            previous_set = now;
        }
        assert_eq!(previous, 0, "{pattern}: last step must be empty");
    }
}

fn material_op(from: &str, steps: i32, region: Option<[i32; 4]>) -> Op {
    Op::Material {
        from: from.into(),
        region: region.map(pxl_format::Region::Rect),
        mask: None,
        x: 0,
        y: 0,
        steps,
        to: None,
    }
}

/// Only pixels of the named ramp move: the shirt colour inside the region and
/// the outline stay put, and so do pixels outside the region.
#[test]
fn material_moves_only_pixels_of_its_ramp_inside_the_region() {
    let pal = pal();
    // olive = [2,3,4,5]; 8 is ground; 1 is the outline.
    let f = grid(&["1238", "2338", "2.88"]);
    let out = apply_op(
        &f,
        &material_op("olive", 1, Some([0, 0, 3, 3])),
        &cx(&pal),
        0,
    )
    .unwrap();
    assert_eq!(text(&out), vec!["1348", "3448", "3.88"]);
    // Nothing outside the region changes, and 8 (not olive) is never touched.
    let none = apply_op(
        &f,
        &material_op("olive", 1, Some([3, 0, 1, 3])),
        &cx(&pal),
        0,
    )
    .unwrap();
    assert_eq!(none.pixels, f.pixels);
}

#[test]
fn material_clamps_at_the_ramp_ends_and_can_retarget_a_ramp() {
    let pal = pal();
    let f = grid(&["25"]);
    let up = apply_op(&f, &material_op("olive", 3, None), &cx(&pal), 0).unwrap();
    assert_eq!(text(&up), vec!["55"], "clamped at the last step");
    let down = apply_op(&f, &material_op("olive", -3, None), &cx(&pal), 0).unwrap();
    assert_eq!(text(&down), vec!["22"]);
    let to = Op::Material {
        from: "olive".into(),
        region: None,
        mask: None,
        x: 0,
        y: 0,
        steps: 0,
        to: Some("ground".into()),
    };
    let mapped = apply_op(&f, &to, &cx(&pal), 0).unwrap();
    assert_eq!(
        text(&mapped),
        vec!["69"],
        "step-for-step onto the other ramp"
    );
}

#[test]
fn material_with_a_mask_selects_by_position_and_rejects_bad_input() {
    let pal = pal();
    let f = grid(&["222", "222"]);
    let op = Op::Material {
        from: "olive".into(),
        region: None,
        mask: Some(vec!["1.1".into()]),
        x: 0,
        y: 1,
        steps: 1,
        to: None,
    };
    let out = apply_op(&f, &op, &cx(&pal), 0).unwrap();
    assert_eq!(text(&out), vec!["222", "323"]);
    for bad in [
        material_op("nope", 1, None),
        material_op("olive", 99, None),
        Op::Material {
            from: "olive".into(),
            region: None,
            mask: None,
            x: 0,
            y: 0,
            steps: 0,
            to: Some("tread".into()),
        },
    ] {
        assert!(apply_op(&f, &bad, &cx(&pal), 0).is_err());
    }
}

fn detail_op(density: u32, inset: i32, seed: Option<u64>) -> Op {
    Op::Detail {
        ramp: "olive".into(),
        color: ColorRef::Index(5),
        region: pxl_format::Region::Rect([0, 0, 8, 8]),
        density,
        seed,
        inset,
    }
}

fn blob() -> pxl_core::IndexFrame {
    grid(&[
        "22222222", "22222222", "22222222", "22222222", "22222222", "22222222", "22222222",
        "22222222",
    ])
}

#[test]
fn detail_is_deterministic_and_bounded_by_density() {
    let pal = pal();
    let f = blob();
    let a = apply_op(&f, &detail_op(2500, 1, Some(7)), &cx(&pal), 0).unwrap();
    let b = apply_op(&f, &detail_op(2500, 1, Some(7)), &cx(&pal), 0).unwrap();
    assert_eq!(a.pixels, b.pixels, "same seed, same detail");
    let other = apply_op(&f, &detail_op(2500, 1, Some(8)), &cx(&pal), 0).unwrap();
    assert_ne!(a.pixels, other.pixels, "a different seed moves the detail");
    let n = a.pixels.iter().filter(|p| **p == 5).count();
    assert!(
        (1..=18).contains(&n),
        "~25% of the 36 inner pixels, got {n}"
    );
    let none = apply_op(&f, &detail_op(0, 1, Some(7)), &cx(&pal), 0).unwrap();
    assert_eq!(none.pixels, f.pixels);
    let all = apply_op(&f, &detail_op(10_000, 1, Some(7)), &cx(&pal), 0).unwrap();
    assert_eq!(all.pixels.iter().filter(|p| **p == 5).count(), 36);
}

/// Detail stays off the shape edge, never lands on another ramp's pixels, and
/// a pixel's choice does not depend on what the rest of the region holds.
#[test]
fn detail_respects_inset_ramp_and_is_position_stable() {
    let pal = pal();
    let f = blob();
    let out = apply_op(&f, &detail_op(10_000, 2, Some(1)), &cx(&pal), 0).unwrap();
    for y in 0..8 {
        for x in 0..8 {
            let edge = x < 2 || y < 2 || x > 5 || y > 5;
            assert_eq!(out.get(x, y) == 5, !edge, "({x},{y})");
        }
    }
    // Pixels of another ramp inside the region are never replaced.
    let mixed = grid(&["88888", "82228", "82228", "82228", "88888"]);
    let op = Op::Detail {
        ramp: "olive".into(),
        color: ColorRef::Index(5),
        region: pxl_format::Region::Rect([0, 0, 5, 5]),
        density: 10_000,
        seed: Some(3),
        inset: 0,
    };
    let m = apply_op(&mixed, &op, &cx(&pal), 0).unwrap();
    assert_eq!(m.get(0, 0), 8);
    assert_eq!(m.get(2, 2), 5);
    // Same seed, bigger canvas: the shared pixel keeps its decision.
    let small = apply_op(&f, &detail_op(3000, 0, Some(5)), &cx(&pal), 0).unwrap();
    let region_only = Op::Detail {
        ramp: "olive".into(),
        color: ColorRef::Index(5),
        region: pxl_format::Region::Rect([0, 0, 4, 4]),
        density: 3000,
        seed: Some(5),
        inset: 0,
    };
    let part = apply_op(&f, &region_only, &cx(&pal), 0).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(part.get(x, y), small.get(x, y), "({x},{y})");
        }
    }
    assert!(apply_op(&f, &detail_op(10_001, 1, Some(1)), &cx(&pal), 0).is_err());
    assert!(apply_op(&f, &detail_op(100, 99, Some(1)), &cx(&pal), 0).is_err());
}
