use pxl_format::{ColorRef, Op};

use super::super::apply_op;
use super::{cx, grid, pal};

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
