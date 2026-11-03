//! Integer paper-doll compose. A part's pivot is placed at
//! parent_top_left + attach, then `part_transform` adds dx/dy and rotates
//! the sprite around that pivot in 90° steps.

use std::collections::{BTreeMap, BTreeSet};

use pxl_core::{decode_matrix, IndexFrame, PxlError, PxlResult};
use pxl_format::{PartDef, PartKey, PartsFile};

#[derive(Clone, Debug, Default)]
pub struct PartPose {
    pub dx: i32,
    pub dy: i32,
    pub rot: i32,
    pub flip_x: bool,
    pub flip_y: bool,
}

impl PartPose {
    pub fn from_key(key: &PartKey) -> PxlResult<Self> {
        Ok(Self {
            dx: key.dx,
            dy: key.dy,
            rot: parse_rot(key.rot90.as_deref())?,
            flip_x: matches!(key.flip.as_deref(), Some("x") | Some("xy")),
            flip_y: matches!(key.flip.as_deref(), Some("y") | Some("xy")),
        })
    }
}

pub fn parse_rot(spec: Option<&str>) -> PxlResult<i32> {
    Ok(match spec {
        None => 0,
        Some("cw") | Some("90") => 1,
        Some("180") => 2,
        Some("ccw") | Some("-90") | Some("270") => 3,
        Some(other) => {
            return Err(PxlError::schema(format!("unknown rot90 '{other}'"))
                .with_hint("rot90 is cw, ccw, or 180"));
        }
    })
}

pub fn parse_flip(spec: Option<&str>) -> PxlResult<(bool, bool)> {
    Ok(match spec {
        None => (false, false),
        Some("x") => (true, false),
        Some("y") => (false, true),
        Some("xy") | Some("both") => (true, true),
        Some(other) => {
            return Err(PxlError::schema(format!("unknown part flip '{other}'"))
                .with_hint("flip is x, y, or xy"));
        }
    })
}

/// Clockwise 90° in y-down pixel space, repeated `turns` times.
fn spin(x: i32, y: i32, turns: i32) -> (i32, i32) {
    let mut x = x;
    let mut y = y;
    for _ in 0..turns.rem_euclid(4) {
        let next_x = -y;
        let next_y = x;
        x = next_x;
        y = next_y;
    }
    (x, y)
}

fn apply_linear(x: i32, y: i32, pose: &PartPose) -> (i32, i32) {
    let mut x = x;
    let mut y = y;
    if pose.flip_x {
        x = -x;
    }
    if pose.flip_y {
        y = -y;
    }
    spin(x, y, pose.rot)
}

pub fn compose_rig(
    canvas: &IndexFrame,
    parts: &PartsFile,
    dir: &str,
    order_override: &[String],
    pose: &BTreeMap<String, PartPose>,
) -> PxlResult<IndexFrame> {
    if canvas.w != parts.w || canvas.h != parts.h {
        return Err(PxlError::semantic(format!(
            "frame is {}x{} but the parts canvas is {}x{}",
            canvas.w, canvas.h, parts.w, parts.h
        ))
        .with_hint("compose uses the parts spec width and height"));
    }
    let sprite_dir = parts.sprite_dir(dir);
    let order = if order_override.is_empty() {
        parts.order_for(dir)
    } else {
        order_override.to_vec()
    };
    let mut pivots = BTreeMap::new();
    let mut out = IndexFrame::canvas(parts.w, parts.h)?;
    for id in &order {
        let part = parts.part(id)?;
        let world = world_pivot(
            parts,
            part,
            sprite_dir,
            pose,
            &mut pivots,
            &mut BTreeSet::new(),
        )?;
        blit_part(
            &mut out,
            part,
            sprite_dir,
            world,
            pose.get(id).cloned().unwrap_or_default(),
        )?;
    }
    if parts.mirror.contains_key(dir) {
        out = flip_canvas_x(&out);
    }
    Ok(out)
}

fn world_pivot(
    file: &PartsFile,
    part: &PartDef,
    sprite_dir: &str,
    pose: &BTreeMap<String, PartPose>,
    cache: &mut BTreeMap<String, (i32, i32)>,
    stack: &mut BTreeSet<String>,
) -> PxlResult<(i32, i32)> {
    if let Some(hit) = cache.get(&part.id) {
        return Ok(*hit);
    }
    if !stack.insert(part.id.clone()) {
        return Err(PxlError::semantic(format!(
            "part '{}' is its own ancestor",
            part.id
        )));
    }
    let local = pose.get(&part.id).cloned().unwrap_or_default();
    let world = if part.id == file.root || part.parent.is_none() {
        (file.anchor[0] + local.dx, file.anchor[1] + local.dy)
    } else {
        let parent = file.part(part.parent.as_deref().unwrap())?;
        let parent_world = world_pivot(file, parent, sprite_dir, pose, cache, stack)?;
        let parent_pose = pose.get(&parent.id).cloned().unwrap_or_default();
        let parent_pivot = parent.pivot_for(sprite_dir);
        let attach = part.attach_for(sprite_dir);
        let rel_x = attach[0] - parent_pivot[0];
        let rel_y = attach[1] - parent_pivot[1];
        let (rel_x, rel_y) = apply_linear(rel_x, rel_y, &parent_pose);
        (
            parent_world.0 + rel_x + local.dx,
            parent_world.1 + rel_y + local.dy,
        )
    };
    cache.insert(part.id.clone(), world);
    Ok(world)
}

fn blit_part(
    canvas: &mut IndexFrame,
    part: &PartDef,
    sprite_dir: &str,
    world_pivot: (i32, i32),
    pose: PartPose,
) -> PxlResult<()> {
    let rows = part.rows(sprite_dir)?;
    let sprite = decode_matrix(&rows)?;
    let pivot = part.pivot_for(sprite_dir);
    for y in 0..sprite.h as i32 {
        for x in 0..sprite.w as i32 {
            let c = sprite.get(x, y);
            if c == 0 {
                continue;
            }
            let (rx, ry) = apply_linear(x - pivot[0], y - pivot[1], &pose);
            canvas.set(world_pivot.0 + rx, world_pivot.1 + ry, c);
        }
    }
    Ok(())
}

fn flip_canvas_x(src: &IndexFrame) -> IndexFrame {
    let mut out = src.blank_like();
    for y in 0..src.h as i32 {
        for x in 0..src.w as i32 {
            out.set(src.w as i32 - 1 - x, y, src.get(x, y));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_format::parse_parts;

    fn tiny() -> PartsFile {
        parse_parts(
            r#"{
              "w": 8, "h": 8, "anchor": [4, 4], "root": "torso",
              "mirror": {"right": "left"},
              "order": {"down": ["torso", "arm"], "left": ["torso", "arm"]},
              "parts": [
                {"id": "torso", "pivot": [0, 0], "z": 0, "sprites": {"down": ["9"], "left": ["8"]}},
                {"id": "arm", "pivot": [0, 0], "parent": "torso", "attach": [1, 0], "z": 1,
                 "sprites": {"down": ["3"], "left": ["2"]}}
              ]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn child_pivot_lands_on_parent_attach() {
        let rig = tiny();
        let canvas = IndexFrame::new(8, 8).unwrap();
        let frame = compose_rig(&canvas, &rig, "down", &[], &BTreeMap::new()).unwrap();
        assert_eq!(frame.get(4, 4), 9);
        assert_eq!(frame.get(5, 4), 3);
    }

    #[test]
    fn rot90_spins_around_the_pivot() {
        let rig = parse_parts(
            r#"{
              "w": 8, "h": 8, "anchor": [3, 3], "root": "tool",
              "parts": [
                {"id": "tool", "pivot": [0, 2], "sprites": {"down": ["f", "f", "g"]}}
              ]
            }"#,
        )
        .unwrap();
        let canvas = IndexFrame::new(8, 8).unwrap();
        let mut pose = BTreeMap::new();
        pose.insert(
            "tool".into(),
            PartPose {
                rot: 1,
                ..PartPose::default()
            },
        );
        let frame = compose_rig(&canvas, &rig, "down", &["tool".into()], &pose).unwrap();
        // Pivot pixel g (16) stays on the anchor. The f pixels above it swing
        // clockwise and land to the right of the pivot.
        assert_eq!(frame.get(3, 3), 16);
        assert_eq!(frame.get(4, 3), 15);
        assert_eq!(frame.get(5, 3), 15);
        assert_eq!(frame.get(3, 2), 0);
    }

    #[test]
    fn right_is_a_mirror_of_left() {
        let rig = tiny();
        let canvas = IndexFrame::new(8, 8).unwrap();
        let left = compose_rig(&canvas, &rig, "left", &[], &BTreeMap::new()).unwrap();
        let right = compose_rig(&canvas, &rig, "right", &[], &BTreeMap::new()).unwrap();
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(right.get(x, y), left.get(7 - x, y), "{x},{y}");
            }
        }
    }
}
