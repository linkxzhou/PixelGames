use std::collections::BTreeMap;

use pxl_core::IndexFrame;

use super::pal;
use crate::{validate, AnimSeq, Check, ValidateInput};

#[test]
fn tile_seam_rejects_a_broken_edge() {
    let palette = pal();
    let mut tile = IndexFrame::new(4, 4).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            tile.set(x, y, 2);
        }
    }
    tile.set(0, 0, 2);
    tile.set(3, 0, 1);
    let input = ValidateInput {
        palette,
        frames: vec![],
        anims: vec![],
        facings: BTreeMap::new(),
        tiles: vec![("ground_tile_0".into(), tile)],
        tile_ramp: "ground".into(),
        tile_ramps: BTreeMap::new(),
        asset_kinds: BTreeMap::new(),
        frame_assets: BTreeMap::new(),
    };
    let report = validate(&input, "tank").unwrap();
    let seam = report
        .checks
        .iter()
        .find(|c| c.id == "tile.ground_tile_0.seam")
        .unwrap();
    assert!(!seam.pass);
}

fn seam_report(tile: IndexFrame) -> Check {
    let input = ValidateInput {
        palette: pal(),
        frames: vec![],
        anims: vec![],
        facings: BTreeMap::new(),
        tiles: vec![("surface".into(), tile)],
        tile_ramp: "ground".into(),
        tile_ramps: BTreeMap::new(),
        asset_kinds: BTreeMap::new(),
        frame_assets: BTreeMap::new(),
    };
    let report = validate(&input, "tank").unwrap();
    report
        .checks
        .into_iter()
        .find(|c| c.id == "tile.surface.seam")
        .unwrap()
}

/// Grass over dirt: the top row is the surface material, the bottom row is
/// what sits underneath. That junction is a layer, not a broken tile.
#[test]
fn a_uniform_layer_boundary_is_not_a_seam() {
    let mut tile = IndexFrame::new(4, 4).unwrap();
    for y in 0..3 {
        for x in 0..4 {
            tile.set(x, y, 3);
        }
    }
    for x in 0..4 {
        tile.set(x, 3, 1);
    }
    let seam = seam_report(tile);
    assert!(seam.pass, "{seam:?}");
    assert!(seam.value.contains("on a layer boundary"), "{seam:?}");
}

/// The same tile with a hole punched through the layer boundary: only some
/// columns change material, so this is a stray pixel pattern, not a layer.
#[test]
fn a_ragged_layer_boundary_still_fails() {
    let mut tile = IndexFrame::new(4, 4).unwrap();
    for y in 0..3 {
        for x in 0..4 {
            tile.set(x, y, 3);
        }
    }
    for x in 0..4 {
        tile.set(x, 3, 1);
    }
    // Keep the left and right edges on the surface ramp, so only the
    // top/bottom comparison can catch the break.
    tile.set(0, 3, 3);
    tile.set(3, 3, 3);
    let seam = seam_report(tile);
    assert!(!seam.pass, "{seam:?}");
}

fn flip_x(src: &IndexFrame) -> IndexFrame {
    let mut out = src.blank_like();
    for y in 0..src.h as i32 {
        for x in 0..src.w as i32 {
            out.set(src.w as i32 - 1 - x, y, src.get(x, y));
        }
    }
    out
}

fn pose(arm_x: i32, lift_left: bool, bob: bool) -> IndexFrame {
    let mut frame = IndexFrame::new(8, 16).unwrap();
    let head = if bob { 1 } else { 2 };
    frame.set(3, head, 2);
    frame.set(4, head, 3);
    for y in 4..12 {
        frame.set(3, y, 2);
        frame.set(4, y, 2);
    }
    frame.set(arm_x, 8, 3);
    frame.set(3, 14, 2);
    frame.set(4, 14, 2);
    if !lift_left {
        frame.set(3, 15, 2);
    }
    frame.set(4, 15, 2);
    frame
}

#[test]
fn character_rules_accept_a_mirrored_walk() {
    let palette = pal();
    let down = pose(2, false, false);
    let up = pose(3, false, false);
    let left = [
        pose(2, true, false),
        pose(2, false, true),
        pose(5, true, false),
        pose(4, false, true),
    ];
    let idle_l = [pose(3, false, false), pose(3, false, true)];
    let mut frames = Vec::new();
    let mut anims = Vec::new();
    let mut push_anim = |name: &str, dir: &str, looped: bool, seq: Vec<IndexFrame>| {
        for (i, frame) in seq.iter().enumerate() {
            frames.push((format!("{name}_{dir}_{i}"), frame.clone()));
        }
        anims.push(AnimSeq {
            name: name.into(),
            asset: String::new(),
            base_name: name.into(),
            dir: Some(dir.into()),
            looped,
            frame_names: (0..seq.len())
                .map(|i| format!("{name}_{dir}_{i}"))
                .collect(),
            frames: seq,
            static_mode: false,
        });
    };
    for dir in ["down", "up", "left", "right"] {
        let walk: Vec<IndexFrame> = if dir == "right" {
            left.iter().map(flip_x).collect()
        } else if dir == "left" {
            left.to_vec()
        } else {
            left.to_vec()
        };
        let idle: Vec<IndexFrame> = if dir == "right" {
            idle_l.iter().map(flip_x).collect()
        } else {
            idle_l.to_vec()
        };
        push_anim("walk", dir, true, walk);
        push_anim("idle", dir, true, idle);
    }
    let hoe = vec![
        pose(2, false, false),
        pose(3, false, false),
        pose(4, false, false),
        pose(5, false, true),
    ];
    push_anim("hoe", "down", false, hoe);
    let mut facings = BTreeMap::new();
    facings.insert("down".into(), down);
    facings.insert("up".into(), up);
    facings.insert("left".into(), idle_l[0].clone());
    facings.insert("right".into(), flip_x(&idle_l[0]));
    let input = ValidateInput {
        palette,
        frames,
        anims,
        facings,
        tiles: vec![],
        tile_ramp: "ground".into(),
        tile_ramps: BTreeMap::new(),
        asset_kinds: BTreeMap::new(),
        frame_assets: BTreeMap::new(),
    };
    let report = validate(&input, "character").unwrap();
    let failed: Vec<_> = report.checks.iter().filter(|c| !c.pass).collect();
    assert!(failed.is_empty(), "{failed:?}");
}
