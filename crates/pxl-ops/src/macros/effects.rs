use std::collections::BTreeMap;

use pxl_core::{PxlError, PxlResult};
use pxl_format::Op;

use super::args::{arg_i32, arg_str, steps_of};

pub(super) fn hurt_flash(args: &BTreeMap<String, String>) -> PxlResult<Vec<Vec<Op>>> {
    let n = steps_of(args, 2)?;
    let mut frames = Vec::new();
    for i in 0..n {
        if i % 2 == 0 {
            frames.push(vec![Op::PaletteSwap {
                map: "all->flash".into(),
                region: None,
            }]);
        } else {
            frames.push(vec![Op::Noop {}]);
        }
    }
    Ok(frames)
}

/// Frame 0 is the untouched sprite and the last frame is empty. `steps` is the
/// frame count, so a 6-frame fade removes 1/5 of the sprite per frame.
pub(super) fn death_dissolve(args: &BTreeMap<String, String>) -> PxlResult<Vec<Vec<Op>>> {
    let steps = steps_of(args, 6)?;
    if steps < 2 {
        return Err(PxlError::schema("death_dissolve needs steps >= 2")
            .with_path("/steps")
            .with_hint("one frame cannot show both the sprite and its absence"));
    }
    let seed = arg_i32(args, "seed", 7)? as u64;
    let pattern = arg_str(args, "pattern", "bayer4").to_string();
    let transitions = (steps - 1) as u32;
    let mut frames = Vec::new();
    for i in 0..steps {
        frames.push(vec![Op::Dissolve {
            seed,
            steps: transitions,
            step: i as u32,
            pattern: pattern.clone(),
        }]);
    }
    Ok(frames)
}

/// Four poses: 0, 90, 180, 270 degrees. Finer steps need arbitrary-angle
/// rotation, which `rotate` does not implement yet, so they are refused rather
/// than faked with repeated frames.
pub(super) fn spin_projectile(args: &BTreeMap<String, String>) -> PxlResult<Vec<Vec<Op>>> {
    let steps = steps_of(args, 4)?;
    if steps != 4 {
        return Err(
            PxlError::schema(format!("spin_projectile steps={steps} is not supported"))
                .with_path("/steps")
                .with_hint("only steps=4 (0/90/180/270 degrees); 8 poses need 45-degree rotation"),
        );
    }
    // Every pose is one transform of the untouched source. Chaining quarter turns
    // would crop back to the canvas after each step, which loses pixels on any
    // sprite that is not square.
    let poses = ["cw", "180", "ccw"];
    let mut frames = vec![vec![Op::Noop {}]];
    for dir in poses {
        frames.push(vec![Op::Rotate90 { dir: dir.into() }]);
    }
    Ok(frames)
}

pub(super) fn pulse_glow(args: &BTreeMap<String, String>) -> PxlResult<Vec<Vec<Op>>> {
    let ramp = arg_str(args, "ramp", "olive");
    let map = format!("ramp:{ramp}+1");
    Ok(vec![
        vec![Op::Noop {}],
        vec![Op::PaletteSwap { map, region: None }],
    ])
}
