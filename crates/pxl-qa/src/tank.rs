use std::collections::BTreeMap;

use pxl_core::{IndexFrame, Palette};

use crate::common::{bbox_wh, fail, fmt_pct};
use crate::model::{AnimSeq, Check, ValidateInput};

pub(crate) fn tread_motion(anim: &AnimSeq, palette: &Palette) -> Vec<Check> {
    let mut out = Vec::new();
    let label = anim.dir.clone().unwrap_or_else(|| "seq".into());
    if anim.frames.is_empty() {
        return out;
    }
    let base = &anim.frames[0];
    let mut mask = Vec::new();
    for (i, px) in base.pixels.iter().enumerate() {
        if is_tread(palette, *px) {
            mask.push(i);
        }
    }
    out.push(Check {
        id: format!("anim.move.{label}.tread_pixels"),
        pass: !mask.is_empty(),
        value: mask.len().to_string(),
        limit: ">= 1 tread pixel on frame 0".into(),
    });
    for i in 0..anim.frames.len() {
        let j = (i + 1) % anim.frames.len();
        if j == 0 && !anim.looped {
            continue;
        }
        let changed = mask
            .iter()
            .any(|&p| anim.frames[i].pixels[p] != anim.frames[j].pixels[p]);
        out.push(Check {
            id: format!("anim.move.{label}.tread_step_{i}"),
            pass: changed,
            value: if changed { "changed" } else { "static" }.into(),
            limit: "tread pixels change every frame".into(),
        });
    }
    out
}

pub(crate) fn hull_stability(anim: &AnimSeq, palette: &Palette) -> Vec<Check> {
    let label = anim.dir.clone().unwrap_or_else(|| "seq".into());
    let counts: Vec<usize> = anim
        .frames
        .iter()
        .map(|f| {
            f.pixels
                .iter()
                .filter(|p| **p != 0 && !palette.in_ramp("tread", **p))
                .count()
        })
        .collect();
    if counts.is_empty() {
        return vec![fail(
            &format!("anim.move.{label}.hull"),
            "0",
            "hull pixels present",
        )];
    }
    let min = *counts.iter().min().unwrap();
    let max = *counts.iter().max().unwrap();
    let sum: usize = counts.iter().sum();
    let n = counts.len();
    // (max-min) / mean <= 5%  => (max-min) * n * 100 <= sum * 5
    let pass = sum == 0 || (max - min) * n * 100 <= sum * 5;
    let bps = if sum == 0 {
        10000
    } else {
        ((max - min) as u64 * n as u64 * 10000 / sum as u64) as u32
    };
    vec![Check {
        id: format!("anim.move.{label}.hull_variance"),
        pass,
        value: fmt_pct(bps),
        limit: "<= 5% across move frames".into(),
    }]
}

pub(crate) fn tank_facing_checks(facings: &BTreeMap<String, IndexFrame>) -> Vec<Check> {
    let mut out = Vec::new();
    let need = ["N", "E", "S", "W"];
    for d in need {
        out.push(Check {
            id: format!("facing.{d}"),
            pass: facings.contains_key(d),
            value: if facings.contains_key(d) {
                "present"
            } else {
                "missing"
            }
            .into(),
            limit: "N,E,S,W".into(),
        });
    }
    if need.iter().all(|d| facings.contains_key(*d)) {
        let bn = bbox_wh(&facings["N"]);
        let bs = bbox_wh(&facings["S"]);
        let be = bbox_wh(&facings["E"]);
        let bw = bbox_wh(&facings["W"]);
        out.push(Check {
            id: "facing.ns_bbox".into(),
            pass: bn == bs && bn.0 > 0,
            value: format!("N {bn:?} S {bs:?}"),
            limit: "N and S bounding boxes match".into(),
        });
        out.push(Check {
            id: "facing.ew_bbox".into(),
            pass: be == (bn.1, bn.0) && bw == be,
            value: format!("E {be:?} W {bw:?} transpose_of_N ({}, {})", bn.1, bn.0),
            limit: "E and W are the transpose of N".into(),
        });
    }
    out
}

pub(crate) fn tank_counts(input: &ValidateInput) -> Vec<Check> {
    let mut out = Vec::new();
    // One group per tank: the animation owner is the asset, so `tank2_move` is
    // counted against tank2 and never merged into the first tank's totals.
    let mut owners: Vec<&str> = Vec::new();
    for a in input
        .anims
        .iter()
        .filter(|a| a.base_name == "move" || a.base_name == "idle")
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
        let find = |base: &str, dir: &str| {
            input
                .anims
                .iter()
                .find(|a| a.base_name == base && a.asset == *owner && a.dir.as_deref() == Some(dir))
                .map(|a| a.frames.len())
                .unwrap_or(0)
        };
        for dir in ["N", "E", "S", "W"] {
            let move_n = find("move", dir);
            let idle_n = find("idle", dir);
            out.push(Check {
                id: format!("count.{prefix}move_{dir}"),
                pass: move_n >= 4,
                value: move_n.to_string(),
                limit: ">= 4".into(),
            });
            out.push(Check {
                id: format!("count.{prefix}idle_{dir}"),
                pass: idle_n >= 2,
                value: idle_n.to_string(),
                limit: ">= 2".into(),
            });
        }
    }
    out
}

pub(crate) fn is_tread(palette: &Palette, c: u8) -> bool {
    c != 0 && c != palette.special.outline && palette.in_ramp("tread", c)
}
