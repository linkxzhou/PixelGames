//! Per-use rules: a tile must be full-bleed, an effect is judged by its
//! lifecycle instead of the sprite motion cap, UI may be sparse but visible.

use std::collections::BTreeMap;

use pxl_core::IndexFrame;

use super::pal;
use crate::{validate, AnimSeq, Report, ValidateInput};

fn filled(w: u16, h: u16, c: u8) -> IndexFrame {
    let mut f = IndexFrame::new(w, h).unwrap();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            f.set(x, y, c);
        }
    }
    f
}

fn square(size: i32, c: u8) -> IndexFrame {
    let mut f = IndexFrame::new(16, 16).unwrap();
    let lo = 8 - size / 2;
    for y in lo..lo + size {
        for x in lo..lo + size {
            f.set(x, y, c);
        }
    }
    f
}

fn input(asset: &str, kind: &str, anims: Vec<AnimSeq>, tiles: bool) -> ValidateInput {
    let mut frames: Vec<(String, IndexFrame)> = Vec::new();
    for a in &anims {
        for (n, f) in a.frame_names.iter().zip(&a.frames) {
            if !frames.iter().any(|(m, _)| m == n) {
                frames.push((n.clone(), f.clone()));
            }
        }
    }
    let mut asset_kinds = BTreeMap::new();
    asset_kinds.insert(asset.to_string(), kind.to_string());
    let frame_assets = frames
        .iter()
        .map(|(n, _)| (n.clone(), asset.to_string()))
        .collect();
    ValidateInput {
        palette: pal(),
        tiles: if tiles { frames.clone() } else { vec![] },
        frames,
        anims,
        facings: BTreeMap::new(),
        tile_ramp: "ground".into(),
        tile_ramps: BTreeMap::new(),
        asset_kinds,
        frame_assets,
    }
}

fn anim(asset: &str, name: &str, looped: bool, frames: Vec<IndexFrame>) -> AnimSeq {
    let static_mode = frames.len() == 1;
    AnimSeq {
        name: name.into(),
        asset: asset.into(),
        base_name: name.into(),
        dir: None,
        looped,
        frame_names: (0..frames.len()).map(|i| format!("{name}_{i}")).collect(),
        frames,
        static_mode,
    }
}

fn check<'a>(report: &'a Report, id: &str) -> Option<&'a crate::Check> {
    report.checks.iter().find(|c| c.id == id)
}

/// v5b's ground tiles left the last row and column clear to pass sprite bbox
/// rules; on a 16px grid that is a visible gap.
#[test]
fn a_tile_with_a_clear_border_fails_full_bleed() {
    let full = filled(16, 16, 2);
    let report = validate(
        &input("b", "tiles", vec![anim("b", "idle", true, vec![full.clone()])], true),
        "default",
    )
    .unwrap();
    assert!(check(&report, "tile.idle_0.full_bleed").unwrap().pass);

    let mut padded = full;
    for i in 0..16 {
        padded.set(15, i, 0);
        padded.set(i, 15, 0);
    }
    let report = validate(
        &input("b", "tiles", vec![anim("b", "idle", true, vec![padded])], true),
        "default",
    )
    .unwrap();
    let c = check(&report, "tile.idle_0.full_bleed").unwrap();
    assert!(!c.pass, "{c:?}");
    assert!(c.value.starts_with("31 "), "{c:?}");
}

/// A brick that breaks may lose material after its first frame; its resting
/// frame must still be full-bleed.
#[test]
fn a_breaking_tile_may_open_after_its_first_frame() {
    let full = filled(16, 16, 2);
    let mut cracked = full.clone();
    cracked.set(0, 0, 0);
    cracked.set(15, 15, 0);
    let report = validate(
        &input("x", "tiles", vec![anim("x", "break", false, vec![full, cracked])], true),
        "default",
    )
    .unwrap();
    assert!(check(&report, "tile.break_0.full_bleed").unwrap().pass);
    assert!(check(&report, "tile.break_1.full_bleed").is_none());
}

fn burst(sizes: &[i32]) -> Vec<IndexFrame> {
    sizes.iter().map(|s| square(*s, 2)).collect()
}

/// An explosion rewrites most of its canvas between frames; under `sprite`
/// the 35% motion cap rejects that, under `effect` the lifecycle rules apply.
#[test]
fn an_effect_is_judged_by_its_lifecycle_not_the_motion_cap() {
    let frames = burst(&[2, 6, 12, 8, 4]);
    let as_sprite = validate(
        &input("boom", "sprite", vec![anim("boom", "burst", false, frames.clone())], false),
        "default",
    )
    .unwrap();
    assert!(!as_sprite.ok(), "sprite rules accept a 12x12 jump");

    let as_effect = validate(
        &input("boom", "effect", vec![anim("boom", "burst", false, frames)], false),
        "default",
    )
    .unwrap();
    let failed: Vec<_> = as_effect.checks.iter().filter(|c| !c.pass).collect();
    assert!(failed.is_empty(), "{failed:?}");
    assert!(check(&as_effect, "anim.burst.effect_grows").is_some());
    assert!(check(&as_effect, "anim.burst.effect_anchor").is_some());
}

#[test]
fn an_effect_that_never_grows_or_never_decays_fails() {
    let shrinking = validate(
        &input("boom", "effect", vec![anim("boom", "burst", false, burst(&[12, 8, 4]))], false),
        "default",
    )
    .unwrap();
    assert!(!check(&shrinking, "anim.burst.effect_grows").unwrap().pass);

    let stuck = validate(
        &input("boom", "effect", vec![anim("boom", "burst", false, burst(&[2, 6, 12]))], false),
        "default",
    )
    .unwrap();
    assert!(!check(&stuck, "anim.burst.effect_decays").unwrap().pass);
}

#[test]
fn an_effect_that_drifts_off_its_anchor_fails() {
    let mut frames = burst(&[2, 6, 4]);
    let mut moved = IndexFrame::new(16, 16).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            moved.set(x, y, 2);
        }
    }
    frames[2] = moved;
    let report = validate(
        &input("boom", "effect", vec![anim("boom", "burst", false, frames)], false),
        "default",
    )
    .unwrap();
    assert!(!check(&report, "anim.burst.effect_anchor").unwrap().pass);
}

/// A HUD label is sparse glyphs on a wide canvas: no bbox or component limits,
/// but an empty label is still a failure, and it is not a tile.
#[test]
fn ui_may_be_sparse_but_must_be_visible() {
    let mut label = IndexFrame::new(32, 8).unwrap();
    for x in (1..31).step_by(3) {
        label.set(x, 3, 3);
    }
    let report = validate(
        &input("hud", "ui", vec![anim("hud", "idle", true, vec![label.clone()])], false),
        "default",
    )
    .unwrap();
    let failed: Vec<_> = report.checks.iter().filter(|c| !c.pass).collect();
    assert!(failed.is_empty(), "{failed:?}");
    assert!(check(&report, "frame.idle_0.bbox").is_none());
    assert!(check(&report, "tile.idle_0.full_bleed").is_none());

    let as_sprite = validate(
        &input("hud", "sprite", vec![anim("hud", "idle", true, vec![label.clone()])], false),
        "default",
    )
    .unwrap();
    assert!(!as_sprite.ok(), "sprite rules should reject ten specks");

    let blank = label.blank_like();
    let report = validate(
        &input("hud", "ui", vec![anim("hud", "idle", true, vec![blank])], false),
        "default",
    )
    .unwrap();
    assert!(!check(&report, "frame.idle_0.visible").unwrap().pass);
}
