//! Programmatic checks. `default` covers a single sprite document.
//! `tank` covers the 4-direction demo: bbox, tread motion, hull stability, tiles.

use std::collections::BTreeMap;

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub id: String,
    pub pass: bool,
    pub value: String,
    pub limit: String,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub rules: String,
    pub checks: Vec<Check>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.pass)
    }
}

#[derive(Clone, Debug)]
pub struct AnimSeq {
    pub name: String,
    pub dir: Option<String>,
    pub looped: bool,
    pub frames: Vec<IndexFrame>,
}

#[derive(Clone, Debug)]
pub struct ValidateInput {
    pub palette: Palette,
    pub frames: Vec<(String, IndexFrame)>,
    pub anims: Vec<AnimSeq>,
    /// Reference sprite for each facing, used by the tank bbox check.
    pub facings: BTreeMap<String, IndexFrame>,
    pub tiles: Vec<(String, IndexFrame)>,
    pub tile_ramp: String,
}

pub fn validate(input: &ValidateInput, rules: &str) -> PxlResult<Report> {
    let mut checks = Vec::new();
    match rules {
        "default" => {
            checks.extend(palette_checks(&input.palette));
            for (name, frame) in &input.frames {
                checks.extend(frame_checks(&input.palette, name, frame, true));
            }
            for anim in &input.anims {
                checks.extend(anim_diff(anim, 100, 3500, true));
            }
        }
        "tank" => {
            checks.extend(palette_checks(&input.palette));
            for (name, frame) in &input.frames {
                checks.extend(frame_checks(&input.palette, name, frame, false));
            }
            checks.extend(tank_facing_checks(&input.facings));
            for anim in input.anims.iter().filter(|a| a.name == "move") {
                checks.extend(anim_diff(anim, 200, 2500, true));
                checks.extend(tread_motion(anim, &input.palette));
                checks.extend(hull_stability(anim, &input.palette));
            }
            if !input.anims.iter().any(|a| a.name == "move") {
                checks.push(fail(
                    "tank.move_present",
                    "missing",
                    "move anim with >= 4 frames per dir",
                ));
            }
            checks.extend(tank_counts(input));
            checks.extend(tile_checks(input));
        }
        other => {
            return Err(PxlError::usage(format!("unknown ruleset '{other}'"))
                .with_hint("rules are default or tank"));
        }
    }
    Ok(Report {
        rules: rules.to_string(),
        checks,
    })
}

fn palette_checks(palette: &Palette) -> Vec<Check> {
    let mut out = Vec::new();
    let semi = palette.colors.iter().any(|c| c.a != 0 && c.a != 255);
    out.push(Check {
        id: "palette.alpha".into(),
        pass: !semi && palette.colors.first().map(|c| c.a == 0).unwrap_or(false),
        value: if semi {
            "semi-transparent".into()
        } else {
            "opaque-or-clear".into()
        },
        limit: "alpha 0 or 255, index 0 clear".into(),
    });
    out
}

fn frame_checks(palette: &Palette, name: &str, frame: &IndexFrame, structural: bool) -> Vec<Check> {
    let mut out = Vec::new();
    let mut bad = 0usize;
    for px in &frame.pixels {
        if *px as usize >= palette.len() {
            bad += 1;
        }
    }
    out.push(Check {
        id: format!("frame.{name}.palette"),
        pass: bad == 0,
        value: bad.to_string(),
        limit: "0 out-of-palette pixels".into(),
    });
    if !structural {
        return out;
    }
    let stats = frame.stats();
    let canvas = (frame.w as u32 * frame.h as u32).max(1);
    let ratio_bps = stats
        .bbox
        .map(|b| (b.area().max(0) as u64 * 10000 / canvas as u64) as u32)
        .unwrap_or(0);
    out.push(Check {
        id: format!("frame.{name}.bbox"),
        pass: (2000..=9500).contains(&ratio_bps),
        value: fmt_pct(ratio_bps),
        limit: "20%..95% of the canvas".into(),
    });
    out.push(Check {
        id: format!("frame.{name}.components"),
        pass: stats.components <= 8,
        value: stats.components.to_string(),
        limit: "<= 8 connected components".into(),
    });
    out.push(Check {
        id: format!("frame.{name}.isolated"),
        pass: stats.isolated <= 4,
        value: stats.isolated.to_string(),
        limit: "<= 4 isolated pixels".into(),
    });
    let outline = palette.special.outline;
    let uses_outline = frame.pixels.iter().any(|p| *p == outline);
    if uses_outline {
        let mut open = 0usize;
        for y in 0..frame.h as i32 {
            for x in 0..frame.w as i32 {
                let c = frame.get(x, y);
                if c == 0 || c == outline {
                    continue;
                }
                let exposed = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
                    frame.in_bounds(x + dx, y + dy) && frame.get(x + dx, y + dy) == 0
                });
                if exposed {
                    open += 1;
                }
            }
        }
        out.push(Check {
            id: format!("frame.{name}.outline"),
            pass: open == 0,
            value: open.to_string(),
            limit: "0 non-outline pixels touching transparency".into(),
        });
    }
    out
}

fn anim_diff(anim: &AnimSeq, min_bps: u32, max_bps: u32, include_loop: bool) -> Vec<Check> {
    let mut out = Vec::new();
    let label = match &anim.dir {
        Some(d) => format!("{}.{}", anim.name, d),
        None => anim.name.clone(),
    };
    if anim.frames.len() < 2 {
        out.push(fail(
            &format!("anim.{label}.frames"),
            &anim.frames.len().to_string(),
            ">= 2 frames",
        ));
        return out;
    }
    let total = (anim.frames[0].w as usize) * (anim.frames[0].h as usize);
    let mut pairs = Vec::new();
    for i in 0..anim.frames.len() - 1 {
        pairs.push((i, i + 1));
    }
    if include_loop && anim.looped {
        pairs.push((anim.frames.len() - 1, 0));
    }
    for (a, b) in pairs {
        let changed = anim.frames[a].diff_count(&anim.frames[b]);
        let bps = pct_bps(changed, total);
        let id = if b == 0 {
            format!("anim.{label}.loop_diff")
        } else {
            format!("anim.{label}.diff_{a}_{b}")
        };
        out.push(Check {
            id,
            pass: (min_bps..=max_bps).contains(&bps),
            value: fmt_pct(bps),
            limit: format!("{}..{}", fmt_pct(min_bps), fmt_pct(max_bps)),
        });
    }
    out
}

fn tread_motion(anim: &AnimSeq, palette: &Palette) -> Vec<Check> {
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

fn hull_stability(anim: &AnimSeq, palette: &Palette) -> Vec<Check> {
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

fn tank_facing_checks(facings: &BTreeMap<String, IndexFrame>) -> Vec<Check> {
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

fn tank_counts(input: &ValidateInput) -> Vec<Check> {
    let mut out = Vec::new();
    for dir in ["N", "E", "S", "W"] {
        let move_n = input
            .anims
            .iter()
            .find(|a| a.name == "move" && a.dir.as_deref() == Some(dir))
            .map(|a| a.frames.len())
            .unwrap_or(0);
        let idle_n = input
            .anims
            .iter()
            .find(|a| a.name == "idle" && a.dir.as_deref() == Some(dir))
            .map(|a| a.frames.len())
            .unwrap_or(0);
        out.push(Check {
            id: format!("count.move_{dir}"),
            pass: move_n >= 4,
            value: move_n.to_string(),
            limit: ">= 4".into(),
        });
        out.push(Check {
            id: format!("count.idle_{dir}"),
            pass: idle_n >= 2,
            value: idle_n.to_string(),
            limit: ">= 2".into(),
        });
    }
    out
}

fn tile_checks(input: &ValidateInput) -> Vec<Check> {
    let mut out = Vec::new();
    out.push(Check {
        id: "tiles.count".into(),
        pass: input.tiles.len() >= 4,
        value: input.tiles.len().to_string(),
        limit: ">= 4".into(),
    });
    for (name, frame) in &input.tiles {
        let mut worst = 0usize;
        let mut bad = false;
        let w = frame.w as i32;
        let h = frame.h as i32;
        for y in 0..h {
            match input.palette.ramp_distance(
                &input.tile_ramp,
                frame.get(0, y),
                frame.get(w - 1, y),
            ) {
                Some(d) => worst = worst.max(d),
                None => bad = true,
            }
        }
        for x in 0..w {
            match input.palette.ramp_distance(
                &input.tile_ramp,
                frame.get(x, 0),
                frame.get(x, h - 1),
            ) {
                Some(d) => worst = worst.max(d),
                None => bad = true,
            }
        }
        out.push(Check {
            id: format!("tile.{name}.seam"),
            pass: !bad && worst <= 1,
            value: if bad {
                "off-ramp".into()
            } else {
                worst.to_string()
            },
            limit: "opposite edges equal or adjacent on the ground ramp".into(),
        });
    }
    out
}

fn is_tread(palette: &Palette, c: u8) -> bool {
    c != 0 && c != palette.special.outline && palette.in_ramp("tread", c)
}

fn bbox_wh(frame: &IndexFrame) -> (i32, i32) {
    frame.bbox().map(|b| (b.w, b.h)).unwrap_or((0, 0))
}

fn pct_bps(changed: usize, total: usize) -> u32 {
    if total == 0 {
        return 0;
    }
    ((changed as u64) * 10000 / total as u64) as u32
}

fn fmt_pct(bps: u32) -> String {
    format!("{}.{:02}%", bps / 100, bps % 100)
}

fn fail(id: &str, value: &str, limit: &str) -> Check {
    Check {
        id: id.to_string(),
        pass: false,
        value: value.to_string(),
        limit: limit.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_core::{colors_from_hex, Cycle, IndexFrame, Palette, Special};

    fn pal() -> Palette {
        let colors = ["#00000000", "#14100c", "#4a4038", "#9a8c7a"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let mut ramps = BTreeMap::new();
        ramps.insert("tread".into(), vec![1, 2]);
        ramps.insert("ground".into(), vec![2, 3]);
        let mut cycles = BTreeMap::new();
        cycles.insert(
            "tread".into(),
            Cycle {
                indices: vec![2, 1],
                period: 2,
            },
        );
        Palette {
            id: "t".into(),
            colors: colors_from_hex(&colors).unwrap(),
            ramps,
            cycles,
            special: Special {
                transparent: 0,
                outline: 1,
                flash: 3,
            },
        }
    }

    #[test]
    fn out_of_palette_pixel_fails() {
        let palette = pal();
        let mut frame = IndexFrame::new(4, 4).unwrap();
        frame.set(1, 1, 9);
        let input = ValidateInput {
            palette,
            frames: vec![("base".into(), frame)],
            anims: vec![],
            facings: BTreeMap::new(),
            tiles: vec![],
            tile_ramp: "ground".into(),
        };
        let report = validate(&input, "default").unwrap();
        let check = report
            .checks
            .iter()
            .find(|c| c.id == "frame.base.palette")
            .unwrap();
        assert!(!check.pass);
        assert!(!report.ok());
    }

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
        };
        let report = validate(&input, "tank").unwrap();
        let seam = report
            .checks
            .iter()
            .find(|c| c.id == "tile.ground_tile_0.seam")
            .unwrap();
        assert!(!seam.pass);
    }
}
