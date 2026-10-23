use pxl_core::{IndexFrame, Palette};

use crate::model::{Check, ValidateInput};

pub(crate) fn palette_checks(palette: &Palette) -> Vec<Check> {
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

pub(crate) fn frame_checks(
    palette: &Palette,
    name: &str,
    frame: &IndexFrame,
    structural: bool,
) -> Vec<Check> {
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

/// Non-sprite uses (effect, ui, overlay) have no silhouette limits, but an
/// invisible frame is never a deliverable unless it is a declared end state.
pub(crate) fn visible_check(name: &str, frame: &IndexFrame) -> Check {
    let n = frame.count_opaque();
    Check {
        id: format!("frame.{name}.visible"),
        pass: n > 0,
        value: n.to_string(),
        limit: ">= 1 opaque pixel".into(),
    }
}

/// True when the frame named `name` is fully transparent and every animation
/// that uses it uses it only as the last frame of a non-looping, non-static
/// animation (death, despawn). The decision follows frame identity, so another
/// empty frame with identical pixels never borrows the exemption, and a frame
/// that is also reused elsewhere (or never referenced at all) is not exempt.
pub(crate) fn is_terminal_empty(input: &ValidateInput, name: &str, frame: &IndexFrame) -> bool {
    if frame.count_opaque() != 0 {
        return false;
    }
    let mut used = false;
    for anim in &input.anims {
        let last = anim.frame_names.len().saturating_sub(1);
        for (i, n) in anim.frame_names.iter().enumerate() {
            if n != name {
                continue;
            }
            let terminal = !anim.looped && !anim.static_mode && i == last && last > 0;
            if !terminal {
                return false;
            }
            used = true;
        }
    }
    used
}

pub(crate) fn bbox_wh(frame: &IndexFrame) -> (i32, i32) {
    frame.bbox().map(|b| (b.w, b.h)).unwrap_or((0, 0))
}

pub(crate) fn pct_bps(changed: usize, total: usize) -> u32 {
    if total == 0 {
        return 0;
    }
    ((changed as u64) * 10000 / total as u64) as u32
}

pub(crate) fn fmt_pct(bps: u32) -> String {
    format!("{}.{:02}%", bps / 100, bps % 100)
}

pub(crate) fn fail(id: &str, value: &str, limit: &str) -> Check {
    Check {
        id: id.to_string(),
        pass: false,
        value: value.to_string(),
        limit: limit.to_string(),
    }
}
