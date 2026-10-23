use std::collections::BTreeMap;

use pxl_core::IndexFrame;

use super::pal;
use crate::{validate, AnimSeq, Report, ValidateInput};

fn death_input(bad_pixel: bool) -> ValidateInput {
    let mut alive = IndexFrame::new(8, 8).unwrap();
    for y in 1..7 {
        for x in 1..7 {
            alive.set(x, y, 2);
        }
    }
    let mut half = alive.clone();
    for x in 1..7 {
        half.set(x, 1, 0);
    }
    let mut gone = alive.blank_like();
    if bad_pixel {
        gone.set(1, 1, 9);
    }
    let seq = vec![alive, half, gone];
    let frames = seq
        .iter()
        .enumerate()
        .map(|(i, f)| (format!("die_{i}"), f.clone()))
        .collect();
    ValidateInput {
        palette: pal(),
        frames,
        anims: vec![AnimSeq {
            name: "die".into(),
            asset: String::new(),
            base_name: "die".into(),
            dir: None,
            looped: false,
            frames: seq,
            frame_names: (0..3).map(|i| format!("die_{i}")).collect(),
            static_mode: false,
        }],
        facings: BTreeMap::new(),
        tiles: vec![],
        tile_ramp: "ground".into(),
        tile_ramps: BTreeMap::new(),
        asset_kinds: BTreeMap::new(),
        frame_assets: BTreeMap::new(),
    }
}

#[test]
fn a_one_pixel_bob_passes_but_a_flicker_does_not() {
    // A sprite filling most of the canvas: a 1px step rewrites ~half the cells.
    let mut high = IndexFrame::new(16, 16).unwrap();
    for y in 2..14 {
        for x in 2..14 {
            high.set(x, y, 2);
        }
    }
    let mut low = IndexFrame::new(16, 16).unwrap();
    for y in 3..15 {
        for x in 2..14 {
            low.set(x, y, 2);
        }
    }
    let bob = AnimSeq {
        name: "bob".into(),
        asset: String::new(),
        base_name: "bob".into(),
        dir: None,
        looped: true,
        frames: vec![high.clone(), low.clone()],
        frame_names: vec!["high".into(), "low".into()],
        static_mode: false,
    };
    let report = validate(
        &ValidateInput {
            palette: pal(),
            frames: vec![],
            anims: vec![bob],
            facings: BTreeMap::new(),
            tiles: vec![],
            tile_ramp: "ground".into(),
            tile_ramps: BTreeMap::new(),
            asset_kinds: BTreeMap::new(),
            frame_assets: BTreeMap::new(),
        },
        "default",
    )
    .unwrap();
    let failed: Vec<_> = report.checks.iter().filter(|c| !c.pass).collect();
    assert!(failed.is_empty(), "1px bob rejected: {failed:?}");

    // Same silhouette, every pixel recoloured. No integer shift explains a
    // flat colour swap, so the residual equals the raw diff and this fails.
    let mut a = IndexFrame::new(16, 16).unwrap();
    let mut b = IndexFrame::new(16, 16).unwrap();
    for y in 2..14 {
        for x in 2..14 {
            a.set(x, y, 2);
            b.set(x, y, 3);
        }
    }
    let flicker = AnimSeq {
        name: "flicker".into(),
        asset: String::new(),
        base_name: "flicker".into(),
        dir: None,
        looped: true,
        frames: vec![a, b],
        frame_names: vec!["a".into(), "b".into()],
        static_mode: false,
    };
    let report = validate(
        &ValidateInput {
            palette: pal(),
            frames: vec![],
            anims: vec![flicker],
            facings: BTreeMap::new(),
            tiles: vec![],
            tile_ramp: "ground".into(),
            tile_ramps: BTreeMap::new(),
            asset_kinds: BTreeMap::new(),
            frame_assets: BTreeMap::new(),
        },
        "default",
    )
    .unwrap();
    assert!(!report.ok(), "a full-canvas colour swap must still fail");
}

fn anim_input(anim: AnimSeq) -> ValidateInput {
    ValidateInput {
        palette: pal(),
        frames: vec![],
        anims: vec![anim],
        facings: BTreeMap::new(),
        tiles: vec![],
        tile_ramp: "ground".into(),
        tile_ramps: BTreeMap::new(),
        asset_kinds: BTreeMap::new(),
        frame_assets: BTreeMap::new(),
    }
}

fn one_frame() -> IndexFrame {
    let mut f = IndexFrame::new(16, 16).unwrap();
    for y in 3..13 {
        for x in 3..13 {
            f.set(x, y, 2);
        }
    }
    f
}

/// A single-frame idle/fly is legal when the recipe declares it static;
/// `anim_diff` must not force two frames onto it.
#[test]
fn declared_static_single_frame_passes() {
    let anim = AnimSeq {
        name: "fly".into(),
        asset: String::new(),
        base_name: "fly".into(),
        dir: None,
        looped: true,
        frames: vec![one_frame()],
        frame_names: vec!["fly_0".into()],
        static_mode: true,
    };
    let report = validate(&anim_input(anim), "default").unwrap();
    let failed: Vec<_> = report.checks.iter().filter(|c| !c.pass).collect();
    assert!(
        failed.is_empty(),
        "static single frame rejected: {failed:?}"
    );
}

/// The same single frame without the declaration is still a failure: the
/// mode is not a way to silently relax checks.
#[test]
fn undeclared_single_frame_still_fails() {
    let anim = AnimSeq {
        name: "fly".into(),
        asset: String::new(),
        base_name: "fly".into(),
        dir: None,
        looped: true,
        frames: vec![one_frame()],
        frame_names: vec!["fly_0".into()],
        static_mode: false,
    };
    let report = validate(&anim_input(anim), "default").unwrap();
    assert!(!report.ok());
    assert!(
        report
            .checks
            .iter()
            .any(|c| !c.pass && c.id.contains("frames")),
        "{:?}",
        report.checks
    );
    // The failure tells the author how to declare the intent.
    let check = report.checks.iter().find(|c| !c.pass).unwrap();
    assert!(check.limit.contains("mode=static"), "{}", check.limit);
}

/// Declaring static but supplying several frames is a spec error.
#[test]
fn static_declaration_with_multiple_frames_fails() {
    let mut second = one_frame();
    second.set(4, 4, 3);
    let anim = AnimSeq {
        name: "idle".into(),
        asset: String::new(),
        base_name: "idle".into(),
        dir: None,
        looped: true,
        frames: vec![one_frame(), second],
        frame_names: vec!["idle_0".into(), "idle_1".into()],
        static_mode: true,
    };
    let report = validate(&anim_input(anim), "default").unwrap();
    assert!(!report.ok(), "static with 2 frames must fail");
    assert!(report
        .checks
        .iter()
        .any(|c| c.id.contains("static_frames") && !c.pass));
}

#[test]
fn clear_final_frame_of_a_one_shot_anim_is_valid() {
    let report = validate(&death_input(false), "default").unwrap();
    let failed: Vec<_> = report.checks.iter().filter(|c| !c.pass).collect();
    assert!(failed.is_empty(), "{failed:?}");
}

#[test]
fn clear_final_frame_still_fails_on_out_of_palette_index() {
    let report = validate(&death_input(true), "default").unwrap();
    let check = report
        .checks
        .iter()
        .find(|c| c.id == "frame.die_2.palette")
        .unwrap();
    assert!(!check.pass);
}

fn solid() -> IndexFrame {
    one_frame()
}

fn named_anim(name: &str, looped: bool, static_mode: bool, names: &[&str]) -> AnimSeq {
    AnimSeq {
        name: name.into(),
        asset: String::new(),
        base_name: name.into(),
        dir: None,
        looped,
        frames: names
            .iter()
            .map(|n| {
                if n.starts_with("empty") {
                    solid().blank_like()
                } else {
                    solid()
                }
            })
            .collect(),
        frame_names: names.iter().map(|n| n.to_string()).collect(),
        static_mode,
    }
}

fn input_for(anims: Vec<AnimSeq>) -> ValidateInput {
    let mut frames: Vec<(String, IndexFrame)> = Vec::new();
    for a in &anims {
        for (n, f) in a.frame_names.iter().zip(&a.frames) {
            if !frames.iter().any(|(m, _)| m == n) {
                frames.push((n.clone(), f.clone()));
            }
        }
    }
    ValidateInput {
        palette: pal(),
        frames,
        anims,
        facings: BTreeMap::new(),
        tiles: vec![],
        tile_ramp: "ground".into(),
        tile_ramps: BTreeMap::new(),
        asset_kinds: BTreeMap::new(),
        frame_assets: BTreeMap::new(),
    }
}

fn bbox_failed(report: &Report, frame: &str) -> bool {
    report
        .checks
        .iter()
        .any(|c| c.id == format!("frame.{frame}.bbox") && !c.pass)
}

/// A death end state must not let an unrelated empty frame with identical
/// pixels skip the structure checks: the looping idle's empty frame is a
/// degenerate sprite.
#[test]
fn terminal_empty_exemption_does_not_leak_to_a_looping_anim() {
    let die = named_anim("die", false, false, &["body", "empty_end"]);
    let idle = named_anim("idle", true, false, &["empty_idle", "body"]);
    let report = validate(&input_for(vec![die, idle]), "default").unwrap();
    assert!(!bbox_failed(&report, "empty_end"), "{:?}", report.checks);
    assert!(bbox_failed(&report, "empty_idle"), "{:?}", report.checks);
}

/// One empty frame used as the end of a death and as a loop frame elsewhere
/// is exempt for neither use.
#[test]
fn a_shared_empty_frame_is_not_exempt_when_any_use_is_not_terminal() {
    let die = named_anim("die", false, false, &["body", "empty_shared"]);
    let idle = named_anim("idle", true, false, &["empty_shared", "body"]);
    let report = validate(&input_for(vec![die, idle]), "default").unwrap();
    assert!(bbox_failed(&report, "empty_shared"), "{:?}", report.checks);
}

/// An empty frame no animation uses has no end-state role to excuse it,
/// and a lone non-looping static empty frame does not excuse itself.
#[test]
fn unreferenced_and_static_empty_frames_are_not_exempt() {
    let report = validate(
        &input_for(vec![named_anim("keep", true, false, &["body", "body2"])]),
        "default",
    )
    .unwrap();
    let mut input = input_for(vec![named_anim("keep", true, false, &["body", "body2"])]);
    input
        .frames
        .push(("empty_orphan".into(), solid().blank_like()));
    assert!(!bbox_failed(&report, "body"));
    let orphan = validate(&input, "default").unwrap();
    assert!(bbox_failed(&orphan, "empty_orphan"), "{:?}", orphan.checks);

    let lone = named_anim("gone", false, true, &["empty_only"]);
    let report = validate(&input_for(vec![lone]), "default").unwrap();
    assert!(bbox_failed(&report, "empty_only"), "{:?}", report.checks);
}
