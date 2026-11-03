use std::collections::BTreeMap;

use pxl_core::{PxlError, PxlResult};
use pxl_format::{Op, PartsFile};

use super::args::arg_str;

pub(super) fn doll(
    args: &BTreeMap<String, String>,
    dir: Option<&str>,
    parts: Option<&PartsFile>,
) -> PxlResult<Vec<Vec<Op>>> {
    let rig = parts.ok_or_else(|| {
        PxlError::semantic("doll() needs a parts spec")
            .with_hint("set the sprite base to gen parts and pass --spec farmer.parts.json")
    })?;
    let anim = arg_str(args, "anim", "idle");
    let facing = dir.unwrap_or("down");
    let (frames, order) = rig.anim_pose(anim, facing)?;
    if frames.is_empty() || frames.len() > 16 {
        return Err(
            PxlError::schema(format!("doll anim '{anim}' has {} frames", frames.len()))
                .with_hint("a doll anim has 1..=16 keyframes"),
        );
    }
    let mut out = Vec::with_capacity(frames.len());
    for frame_pose in frames {
        let mut ops = Vec::new();
        for (part, key) in &frame_pose {
            rig.part(part)?;
            let identity = key.dx == 0 && key.dy == 0 && key.rot90.is_none() && key.flip.is_none();
            if identity {
                continue;
            }
            ops.push(Op::PartTransform {
                part: part.clone(),
                dx: key.dx,
                dy: key.dy,
                rot90: key.rot90.clone(),
                flip: key.flip.clone(),
            });
        }
        ops.push(Op::Compose {
            parts: order.clone(),
            dir: Some(facing.to_string()),
        });
        out.push(ops);
    }
    Ok(out)
}
