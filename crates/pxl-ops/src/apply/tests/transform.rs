use pxl_format::{Op, Region};

use super::super::{apply_op, flip, rotate90};
use super::{cx, grid, pal, text};

#[test]
fn flip_matches_piskel_axes() {
    let f = grid(&["12", "34"]);
    // axis x is left-right, which Piskel calls VERTICAL.
    assert_eq!(text(&flip(&f, "x").unwrap()), vec!["21", "43"]);
    // axis y is up-down, which Piskel calls HORIZONTAL.
    assert_eq!(text(&flip(&f, "y").unwrap()), vec!["34", "12"]);
}

#[test]
fn rotate90_square_matches_hand_trace() {
    let f = grid(&["12", "34"]);
    assert_eq!(text(&rotate90(&f, "cw").unwrap()), vec!["31", "42"]);
    assert_eq!(text(&rotate90(&f, "ccw").unwrap()), vec!["24", "13"]);
    assert_eq!(text(&rotate90(&f, "180").unwrap()), vec!["43", "21"]);
}

#[test]
fn rotate90_nonsquare_matches_piskel_pad() {
    // Values 1..8 on a 4×2 canvas. Hand-traced from TransformUtils.rotate.
    let f = grid(&["1234", "5678"]);
    assert_eq!(text(&rotate90(&f, "cw").unwrap()), vec![".62.", ".73."]);
}

#[test]
fn shift_matches_piskel_move() {
    let pal = pal();
    let f = grid(&["12", "34"]);
    let out = apply_op(
        &f,
        &Op::Shift {
            dx: 1,
            dy: 0,
            wrap: false,
            region: None,
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    assert_eq!(text(&out), vec![".1", ".3"]);
    let up = apply_op(
        &f,
        &Op::Shift {
            dx: 0,
            dy: -1,
            wrap: false,
            region: None,
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    assert_eq!(text(&up), vec!["34", ".."]);
}

#[test]
fn shift_wrap_and_region() {
    let pal = pal();
    let f = grid(&["1234", "5678"]);
    let out = apply_op(
        &f,
        &Op::Shift {
            dx: 1,
            dy: 0,
            wrap: true,
            region: Some(Region::Rect([0, 0, 4, 1])),
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    assert_eq!(text(&out)[0], "4123");
    assert_eq!(text(&out)[1], "5678");
}

#[allow(clippy::too_many_arguments)]
fn row_shift(
    rows: Option<[i32; 2]>,
    dx: i32,
    dy: i32,
    pattern: Option<&str>,
    phase: i32,
    region: Option<Region>,
    axis: Option<&str>,
) -> Op {
    Op::RowShift {
        rows,
        dx,
        dy,
        pattern: pattern.map(str::to_string),
        phase,
        region,
        axis: axis.map(str::to_string),
        wrap: None,
    }
}

/// `row_shift` has two modes: per-row (`rows`, `pattern`, `phase`, `dx`) and
/// region scroll (`region`, `axis`, `dx`/`dy`). A parameter that belongs to
/// the other mode used to be dropped without a word.
#[test]
fn row_shift_rejects_parameters_its_mode_would_ignore() {
    let pal = pal();
    let f = grid(&["1234", "5678"]);
    let rect = || Some(Region::Rect([0, 0, 4, 2]));
    let run = |op: Op| apply_op(&f, &op, &cx(&pal), 0);

    // Region mode: rows / pattern / phase have no meaning.
    for bad in [
        row_shift(Some([0, 0]), 1, 0, None, 0, rect(), None),
        row_shift(None, 1, 0, Some("sine4"), 0, rect(), None),
        row_shift(None, 1, 0, None, 2, rect(), None),
    ] {
        let err = run(bad).unwrap_err();
        assert!(err.message.contains("region"), "{}", err.message);
    }
    // Row mode: axis and dy have no meaning.
    for bad in [
        row_shift(None, 1, 0, None, 0, None, Some("y")),
        row_shift(None, 1, 1, None, 0, None, None),
    ] {
        let err = run(bad).unwrap_err();
        assert!(err.message.contains("region"), "{}", err.message);
    }
    // Each mode's own parameters keep working.
    assert!(run(row_shift(None, 1, 0, None, 0, rect(), None)).is_ok());
    assert!(run(row_shift(None, 0, 1, None, 0, rect(), Some("y"))).is_ok());
    assert!(run(row_shift(Some([0, 1]), 1, 0, Some("sine4"), 1, None, None)).is_ok());
}

#[test]
fn shear_rejects_a_misspelt_axis_instead_of_shearing_x() {
    let pal = pal();
    let f = grid(&["1111", "2222", "3333", "4444"]);
    for axis in ["vertcial", "z", "", "X "] {
        let err = apply_op(
            &f,
            &Op::Shear {
                axis: axis.into(),
                k: 1,
                pivot: 0,
                region: None,
            },
            &cx(&pal),
            0,
        )
        .unwrap_err();
        assert!(
            err.message.contains("shear axis"),
            "{axis:?}: {}",
            err.message
        );
    }
    for axis in ["x", "horizontal", "y", "vertical"] {
        assert!(apply_op(
            &f,
            &Op::Shear {
                axis: axis.into(),
                k: 1,
                pivot: 0,
                region: None
            },
            &cx(&pal),
            0,
        )
        .is_ok());
    }
}

#[test]
fn row_shift_shear_squash_patch() {
    let pal = pal();
    let f = grid(&["1111", "2222", "3333", "4444"]);
    let rows = apply_op(
        &f,
        &Op::RowShift {
            rows: Some([1, 1]),
            dx: 1,
            dy: 0,
            pattern: None,
            phase: 0,
            region: None,
            axis: None,
            wrap: Some(true),
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    assert_eq!(text(&rows)[1], "2222");
    // wrap of a solid row stays solid. Use a varied row.
    let f2 = grid(&["1234", "5678"]);
    let rows = apply_op(
        &f2,
        &Op::RowShift {
            rows: Some([0, 0]),
            dx: 1,
            dy: 0,
            pattern: Some("sine4".into()),
            phase: 1,
            region: None,
            axis: None,
            wrap: Some(false),
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    // phase 1 => SINE4[1] = 1, so the row shifts by +1.
    assert_eq!(text(&rows)[0], ".123");
    let sheared = apply_op(
        &f,
        &Op::Shear {
            axis: "x".into(),
            k: 1,
            pivot: 0,
            region: None,
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    assert_eq!(sheared.get(1, 1), 2);
    let squashed = apply_op(
        &f,
        &Op::Squash {
            sx: 0,
            sy: -1,
            anchor: "bottom".into(),
            region: None,
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    assert_eq!(squashed.h, 4);
    assert_eq!(squashed.get(0, 3), 4);
    let patched = apply_op(
        &f,
        &Op::PatchPixels {
            x: 0,
            y: 0,
            rows: vec!["99".into()],
        },
        &cx(&pal),
        0,
    )
    .unwrap();
    assert_eq!(patched.get(0, 0), 9);
    assert_eq!(patched.get(1, 0), 9);
}
