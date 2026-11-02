use std::collections::BTreeMap;

use pxl_core::Palette;
use pxl_format::Op;

use super::super::apply_op;
use super::{cx, grid, pal, text};

#[test]
fn palette_cycle_and_swap() {
    let pal = pal();
    let f = grid(&["67", ".."]);
    let out = apply_op(
        &f,
        &Op::PaletteCycle {
            cycle: "tread".into(),
            step: 1,
            region: None,
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    // 6 -> 7, 7 -> 1
    assert_eq!(text(&out)[0], "71");
    let swapped = apply_op(
        &f,
        &Op::PaletteSwap {
            map: "6->4,all->flash".into(),
            region: None,
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    // 6 becomes 4 first, then `all` sends every opaque pixel to flash (11 = b).
    assert_eq!(text(&swapped)[0], "bb");
}

fn shade_op(ramp: Option<&str>) -> Op {
    Op::Shade {
        dir: "top".into(),
        amount: 1,
        region: None,
        ramp: ramp.map(str::to_string),
    }
}

/// Colour 8 sits on both `sideR` and `metal`, like the shaded-crate palette.
fn shared_ramp_palette() -> Palette {
    use pxl_core::{Rgba, Special};
    Palette {
        id: "shared".into(),
        colors: (0..12)
            .map(|i| {
                if i == 0 {
                    Rgba::TRANSPARENT
                } else {
                    Rgba::rgb(i as u8 * 20, 0, 0)
                }
            })
            .collect(),
        ramps: BTreeMap::from([
            ("metal".to_string(), vec![8, 10]),
            ("sideR".to_string(), vec![8, 4, 9]),
        ]),
        cycles: BTreeMap::new(),
        special: Special {
            transparent: 0,
            outline: 1,
            flash: 11,
        },
    }
}

#[test]
fn shade_leaves_a_colour_shared_by_two_ramps_alone() {
    let pal = shared_ramp_palette();
    let f = grid(&["888", "888"]);
    let out = apply_op(&f, &shade_op(None), &cx(&pal), 0).unwrap();
    // Top edge would move 8 along a ramp, but two ramps claim it.
    assert_eq!(out.pixels, f.pixels);
}

#[test]
fn shade_with_a_named_ramp_stays_on_that_ramp() {
    let pal = shared_ramp_palette();
    let f = grid(&["888", "888"]);
    let wood = apply_op(&f, &shade_op(Some("sideR")), &cx(&pal), 0).unwrap();
    let metal = apply_op(&f, &shade_op(Some("metal")), &cx(&pal), 0).unwrap();
    // Top row is the exposed edge, so it steps up one slot; bottom steps down.
    assert_eq!(text(&wood)[0], "444");
    assert_eq!(text(&metal)[0], "aaa");
    assert_eq!(text(&wood)[1], "888");
}

#[test]
fn shade_rejects_an_unknown_ramp() {
    let pal = shared_ramp_palette();
    let f = grid(&["88"]);
    let err = apply_op(&f, &shade_op(Some("nope")), &cx(&pal), 0).unwrap_err();
    assert!(err.message.contains("nope"), "{err:?}");
}

#[test]
fn shade_never_repaints_the_outline_colour() {
    // Index 1 is the tank outline and also sits on the tread ramp.
    let pal = pal();
    let f = grid(&["111", "121", "111"]);
    let out = apply_op(&f, &shade_op(None), &cx(&pal), 0).unwrap();
    for y in 0..3 {
        for x in 0..3 {
            if f.get(x, y) == 1 {
                assert_eq!(out.get(x, y), 1, "outline moved at {x},{y}");
            }
        }
    }
}
