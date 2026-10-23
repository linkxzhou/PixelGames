use pxl_core::IndexFrame;

use crate::animation::{anim_diff, scoped};
use crate::common::{bbox_wh, fail};
use crate::model::{AnimSeq, Check, ValidateInput};

pub(crate) fn character_checks(input: &ValidateInput) -> Vec<Check> {
    let mut out = Vec::new();
    let need = ["down", "up", "left", "right"];
    for d in need {
        out.push(Check {
            id: format!("facing.{d}"),
            pass: input.facings.contains_key(d),
            value: if input.facings.contains_key(d) {
                "present".into()
            } else {
                "missing".into()
            },
            limit: "down, up, left, right".into(),
        });
    }
    if let (Some(left), Some(right)) = (input.facings.get("left"), input.facings.get("right")) {
        let bl = bbox_wh(left);
        let br = bbox_wh(right);
        out.push(Check {
            id: "facing.lr_bbox".into(),
            pass: bl == br && bl.0 > 0,
            value: format!("left {bl:?} right {br:?}"),
            limit: "left and right bounding boxes match".into(),
        });
        let mirror = is_x_flip(left, right);
        out.push(Check {
            id: "facing.right_mirror".into(),
            pass: mirror,
            value: if mirror { "mirror" } else { "not a mirror" }.into(),
            limit: "right pixels are the x-flip of left".into(),
        });
    }
    if let (Some(down), Some(up)) = (input.facings.get("down"), input.facings.get("up")) {
        let bd = bbox_wh(down);
        let bu = bbox_wh(up);
        let dw = (bd.0 - bu.0).abs();
        let dh = (bd.1 - bu.1).abs();
        out.push(Check {
            id: "facing.du_bbox".into(),
            pass: dw <= 2 && dh <= 2 && bd.0 > 0,
            value: format!("down {bd:?} up {bu:?}"),
            limit: "down and up width/height within 2px".into(),
        });
    }
    // Facings come from the first asset only (the sheet keeps one reference per
    // direction); the animation checks below run once for every character.
    let mut owners: Vec<&str> = Vec::new();
    for a in input
        .anims
        .iter()
        .filter(|a| matches!(a.base_name.as_str(), "walk" | "idle" | "hoe"))
    {
        if !owners.contains(&a.asset.as_str()) {
            owners.push(a.asset.as_str());
        }
    }
    if owners.is_empty() {
        owners.push("");
    }
    for (n, owner) in owners.iter().enumerate() {
        let prefix = if n == 0 {
            String::new()
        } else {
            format!("{owner}_")
        };
        // The first character carries the full contract. A recolour may ship
        // only the animations it changes, so it must have a walk and whatever
        // else it declares is checked, but a missing idle or hoe is not an error.
        out.extend(character_anim_checks(input, owner, &prefix, n == 0));
    }
    out
}

/// Walk, idle and hoe checks for the character owned by `owner`. `prefix` is
/// empty for the first character and `<asset>_` for the rest, so ids never
/// collide and a failure names the character that caused it. With `full` false,
/// an absent idle or hoe is skipped instead of failing.
pub(crate) fn character_anim_checks(
    input: &ValidateInput,
    owner: &str,
    prefix: &str,
    full: bool,
) -> Vec<Check> {
    let mut out = Vec::new();
    let need = ["down", "up", "left", "right"];
    let find = |name: &str, dir: &str| find_anim_of(input, owner, name, dir);
    for dir in need {
        let walk = find("walk", dir);
        let idle = find("idle", dir);
        let walk_n = walk.map(|a| a.frames.len()).unwrap_or(0);
        let idle_n = idle.map(|a| a.frames.len()).unwrap_or(0);
        out.push(Check {
            id: format!("count.{prefix}walk_{dir}"),
            pass: walk_n >= 4,
            value: walk_n.to_string(),
            limit: ">= 4".into(),
        });
        if full || idle.is_some() {
            out.push(Check {
                id: format!("count.{prefix}idle_{dir}"),
                pass: idle_n >= 2,
                value: idle_n.to_string(),
                limit: ">= 2".into(),
            });
        }
        if let Some(walk) = walk {
            out.extend(scoped(walk, anim_diff(walk, 100, 3500, true)));
        }
        if let Some(idle) = idle {
            out.extend(scoped(idle, anim_diff(idle, 50, 2500, true)));
        }
        let mut feet = Vec::new();
        for anim in [walk, idle].into_iter().flatten() {
            for frame in &anim.frames {
                feet.push(foot_line(frame));
            }
        }
        if dir == "down" {
            if let Some(hoe) = find("hoe", "down") {
                for frame in &hoe.frames {
                    feet.push(foot_line(frame));
                }
            }
        }
        let known: Vec<i32> = feet.into_iter().flatten().collect();
        let stable = !known.is_empty() && known.iter().all(|y| *y == known[0]);
        out.push(Check {
            id: format!("feet.{prefix}{dir}"),
            pass: stable,
            value: if known.is_empty() {
                "missing".into()
            } else {
                format!("{known:?}")
            },
            limit: "one ground line across walk, idle, and hoe".into(),
        });
    }
    out.extend(mirror_anim_of(input, owner, prefix, "walk"));
    out.extend(mirror_anim_of(input, owner, prefix, "idle"));
    match find("hoe", "down") {
        Some(hoe) => {
            out.push(Check {
                id: format!("count.{prefix}hoe_down"),
                pass: hoe.frames.len() >= 4,
                value: hoe.frames.len().to_string(),
                limit: ">= 4".into(),
            });
            out.push(Check {
                id: format!("{prefix}hoe.loop"),
                pass: !hoe.looped,
                value: hoe.looped.to_string(),
                limit: "false".into(),
            });
            out.extend(scoped(hoe, anim_diff(hoe, 100, 5000, false)));
        }
        None if full => {
            out.push(fail(
                &format!("count.{prefix}hoe_down"),
                "missing",
                ">= 4 frames on down",
            ));
            out.push(fail(&format!("{prefix}hoe.loop"), "missing", "false"));
        }
        None => {}
    }
    out
}

pub(crate) fn find_anim_of<'a>(
    input: &'a ValidateInput,
    owner: &str,
    name: &str,
    dir: &str,
) -> Option<&'a AnimSeq> {
    input
        .anims
        .iter()
        .find(|a| a.base_name == name && a.asset == owner && a.dir.as_deref() == Some(dir))
}

pub(crate) fn mirror_anim_of(
    input: &ValidateInput,
    owner: &str,
    prefix: &str,
    name: &str,
) -> Vec<Check> {
    let Some(left) = find_anim_of(input, owner, name, "left") else {
        return Vec::new();
    };
    let Some(right) = find_anim_of(input, owner, name, "right") else {
        return Vec::new();
    };
    if left.frames.len() != right.frames.len() {
        return vec![fail(
            &format!("mirror.{prefix}{name}"),
            &format!("left {} right {}", left.frames.len(), right.frames.len()),
            "same frame count",
        )];
    }
    left.frames
        .iter()
        .zip(right.frames.iter())
        .enumerate()
        .map(|(i, (l, r))| {
            let ok = is_x_flip(l, r);
            Check {
                id: format!("mirror.{prefix}{name}.{i}"),
                pass: ok,
                value: if ok { "mirror" } else { "not a mirror" }.into(),
                limit: "right frame is the x-flip of left".into(),
            }
        })
        .collect()
}

pub(crate) fn is_x_flip(left: &IndexFrame, right: &IndexFrame) -> bool {
    if left.w != right.w || left.h != right.h {
        return false;
    }
    let w = left.w as i32;
    for y in 0..left.h as i32 {
        for x in 0..w {
            if right.get(x, y) != left.get(w - 1 - x, y) {
                return false;
            }
        }
    }
    true
}

pub(crate) fn foot_line(frame: &IndexFrame) -> Option<i32> {
    frame.bbox().map(|b| b.y + b.h - 1)
}
