//! Resolution diagnostics. These describe the pixel budget an asset was drawn
//! at, never how attractive it is: a deliberate 16x16 sprite must be able to
//! pass, and none of these checks may be satisfied by adding noise or colour.
//!
//! Display scale is deliberately absent: the frame does not know the multiple a
//! game will draw it at, so `pxl render` and `pxl build` report that instead.

use crate::common::{ratio, ratio_at_least, style};
use crate::{Check, QaConfig, Severity};
use pxl_core::IndexFrame;

/// Two adjacent pixels holding the same index. A frame made only of `k x k`
/// blocks is the fingerprint of a small image scaled up, so its content budget
/// is `k*k` times finer than the canvas suggests.
pub(crate) fn inspect(frame: &IndexFrame, name: &str, config: &QaConfig) -> Vec<Check> {
    let prefix = format!("frame.{name}");
    let mut out = Vec::new();
    let (w, h) = (frame.w as i32, frame.h as i32);
    let mut pairs = 0usize;
    let mut same = 0usize;
    let mut blocks = 0usize;
    for y in 0..h {
        for x in 0..w {
            let c = frame.get(x, y);
            if c == 0 {
                continue;
            }
            for (dx, dy) in [(1, 0), (0, 1)] {
                if x + dx >= w || y + dy >= h {
                    continue;
                }
                let other = frame.get(x + dx, y + dy);
                if other == 0 {
                    continue;
                }
                pairs += 1;
                if other == c {
                    same += 1;
                }
            }
            // Top-left of a k>1 uniform square block, for both axes at once.
            if frame.get(x + 1, y) == c && frame.get(x, y + 1) == c && frame.get(x + 1, y + 1) == c
            {
                blocks += 1;
            }
        }
    }
    let uniform = ratio(same, pairs);
    out.push(Check::new(
        format!("{prefix}.uniform_area_bps"),
        true,
        uniform,
        "measurement only: share of opaque 4-adjacent pixel pairs of one colour",
        Severity::Info,
    ));
    out.push(Check::new(
        format!("{prefix}.block_corners"),
        true,
        blocks,
        "measurement only: 2x2 uniform block corners",
        Severity::Info,
    ));
    if let Some(bps) = copy2x_bps(frame) {
        // At 128 and above a hand-drawn sprite should not be made of 2x2
        // blocks; below that, a deliberately chunky style is normal.
        let gate = config.copy2x_warn_bps;
        out.push(match gate {
            Some(limit) => Check::new(
                format!("{prefix}.copy2x_bps"),
                bps >= limit,
                bps,
                format!("< {limit} bps of aligned 2x2 blocks are one colour (an upscaled image)"),
                style(config),
            ),
            None => Check::new(
                format!("{prefix}.copy2x_bps"),
                true,
                bps,
                "measurement only: share of aligned 2x2 blocks whose four pixels are equal (10000 = an exact 2x copy)",
                Severity::Info,
            ),
        });
    }
    if let Some(min) = config.min_content_coverage_bps {
        let coverage = ratio(frame.count_opaque(), frame.pixels.len());
        out.push(Check::new(
            format!("{prefix}.content_coverage"),
            ratio_at_least(frame.count_opaque(), frame.pixels.len(), min),
            coverage,
            format!(">= {min} bps of the canvas holds the subject (None disables)"),
            style(config),
        ));
    }
    out
}

/// Share of aligned 2x2 blocks that hold one colour, in basis points. A frame
/// produced by doubling every pixel scores 10000; hand-drawn detail lowers it.
/// Odd sides have no aligned 2x2 grid, so nothing is reported. This is a
/// measurement: a deliberate blocky style may score high and still be correct.
fn copy2x_bps(frame: &IndexFrame) -> Option<u32> {
    if frame.w % 2 != 0 || frame.h % 2 != 0 {
        return None;
    }
    let mut blocks = 0usize;
    let mut total = 0usize;
    for by in 0..(frame.h as i32) / 2 {
        for bx in 0..(frame.w as i32) / 2 {
            let (x, y) = (bx * 2, by * 2);
            total += 1;
            let c = frame.get(x, y);
            if frame.get(x + 1, y) == c && frame.get(x, y + 1) == c && frame.get(x + 1, y + 1) == c
            {
                blocks += 1;
            }
        }
    }
    Some(ratio(blocks, total))
}
