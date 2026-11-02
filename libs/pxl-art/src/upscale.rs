//! Upscalers that work on palette indices. Each one only ever copies a colour
//! that already exists in the frame: nothing is blended, so the result stays
//! indexed and the palette never grows behind the caller's back.
//!
//! `copy` is the honest baseline, `epx` the classic corner rounder, `scale3x`
//! its 3x sibling, `xbr` an edge-direction variant that follows a diagonal
//! instead of cutting it, `contour` a vector reconstruction that can express
//! geometry the source grid cannot, and `hybrid` picks per pixel from the
//! analysed sprite, `mmpx` the style-preserving 2x/4x rule set.

use pxl_core::{IndexFrame, Palette, PxlResult};

use crate::analyze::{Material, SpriteGraph};

/// Fixed-point scale shared by contour and distance-field shading.
pub(crate) const UNIT: i32 = 256;
/// Chamfer step for a diagonal move, rounded to 1/256 pixel.
const DIAG: i32 = 362;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Copy,
    Epx,
    Scale3x,
    Xbr,
    Contour,
    Hybrid,
    /// McGuire & Gagiu's MMPX: keeps lines, text and 2:1 slopes.
    Mmpx,
}

impl Mode {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "copy" => Self::Copy,
            "epx" | "corners" | "scale2x" => Self::Epx,
            "scale3x" => Self::Scale3x,
            "xbr" | "xbr-index" => Self::Xbr,
            "contour" => Self::Contour,
            "hybrid" => Self::Hybrid,
            "mmpx" => Self::Mmpx,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Epx => "epx",
            Self::Scale3x => "scale3x",
            Self::Xbr => "xbr",
            Self::Contour => "contour",
            Self::Hybrid => "hybrid",
            Self::Mmpx => "mmpx",
        }
    }

    pub fn supports(self, k: u16) -> bool {
        match self {
            Self::Epx | Self::Xbr => k == 2,
            Self::Scale3x => k == 3,
            Self::Copy | Self::Contour | Self::Hybrid => matches!(k, 2 | 3 | 4),
            Self::Mmpx => matches!(k, 2 | 4),
        }
    }
}

pub fn run(
    frame: &IndexFrame,
    k: u16,
    mode: Mode,
    graph: &SpriteGraph,
    palette: &Palette,
) -> PxlResult<IndexFrame> {
    let mut out = match mode {
        Mode::Copy => pxl_core::scale_copy(frame, k),
        Mode::Epx => pxl_core::epx2x(frame),
        Mode::Scale3x => scale3x(frame),
        Mode::Xbr => xbr_index(frame, palette),
        Mode::Contour => contour(frame, k, graph),
        Mode::Hybrid => hybrid(frame, k, graph, palette),
        Mode::Mmpx => crate::mmpx::mmpx(frame, k, palette),
    }?;
    if mode != Mode::Copy {
        restore_protected(frame, k, graph, &mut out);
    }
    Ok(out)
}

/// Keep every native block selected by S1 axis-aligned, regardless of which
/// upscaler produced the rest of the frame.
fn restore_protected(frame: &IndexFrame, k: u16, graph: &SpriteGraph, out: &mut IndexFrame) {
    let kk = k as i32;
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            if graph.editable(x, y) {
                continue;
            }
            let color = frame.get(x, y);
            for dy in 0..kk {
                for dx in 0..kk {
                    out.set(x * kk + dx, y * kk + dy, color);
                }
            }
        }
    }
}

/// Pixels whose shape must stay untouched are copied; everything else takes the
/// smoothing that suits its material.
fn hybrid(
    frame: &IndexFrame,
    k: u16,
    graph: &SpriteGraph,
    palette: &Palette,
) -> PxlResult<IndexFrame> {
    let mut out = pxl_core::scale_copy(frame, k)?;
    let rounded = contour(frame, k, graph)?;
    let edge = match k {
        2 => xbr_index(frame, palette)?,
        3 => scale3x(frame)?,
        _ => rounded.clone(),
    };
    let kk = k as i32;
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            if !graph.editable(x, y) {
                continue;
            }
            let source = match graph.at(x, y) {
                Material::Metal | Material::Glass => &edge,
                _ => &rounded,
            };
            for dy in 0..kk {
                for dx in 0..kk {
                    let (px, py) = (x * kk + dx, y * kk + dy);
                    out.set(px, py, source.get(px, py));
                }
            }
        }
    }
    Ok(out)
}

/// Scale3x: the 3x sibling of EPX. The centre pixel always survives, which is
/// what keeps single-pixel detail from being eaten at 3x.
pub fn scale3x(frame: &IndexFrame) -> PxlResult<IndexFrame> {
    let mut out = IndexFrame::canvas(frame.w * 3, frame.h * 3)?;
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let p = frame.get(x, y);
            let a = frame.get(x, y - 1);
            let b = frame.get(x + 1, y);
            let c = frame.get(x - 1, y);
            let d = frame.get(x, y + 1);
            let e = frame.get(x + 1, y - 1);
            let f = frame.get(x - 1, y - 1);
            let g = frame.get(x - 1, y + 1);
            let h = frame.get(x + 1, y + 1);
            if p == 0 {
                continue;
            }
            let mut cells = [p; 9];
            let top_left = c == a && a != b && c != d;
            let top_right = a == b && a != c && b != d;
            let bottom_left = c == d && c != a && d != b;
            let bottom_right = d == b && d != a && b != c;
            if top_left {
                cells[0] = c;
            }
            if top_right {
                cells[2] = b;
            }
            if bottom_left {
                cells[6] = c;
            }
            if bottom_right {
                cells[8] = b;
            }
            // Scale3x also fills the four edge centres. Each takes the colour
            // of its own edge, never the colour from the opposite diagonal.
            if (top_left && p != e) || (top_right && p != f) {
                cells[1] = a;
            }
            if (top_left && p != g) || (bottom_left && p != f) {
                cells[3] = c;
            }
            if (top_right && p != h) || (bottom_right && p != e) {
                cells[5] = b;
            }
            if (bottom_left && p != h) || (bottom_right && p != g) {
                cells[7] = d;
            }
            for (i, value) in cells.iter().enumerate() {
                out.set(x * 3 + (i as i32 % 3), y * 3 + (i as i32 / 3), *value);
            }
        }
    }
    Ok(out)
}

/// xBR over index colours. The corner rules are EPX's, but "these two pixels
/// are the same" widens to "these two pixels are neighbours on a ramp": a
/// lightness step inside a quarter of the palette's range reads as one surface,
/// not as an edge. Unrelated hues still act as edges, so a sprite's outer
/// contour rounds while the shading inside it stays flat.
pub fn xbr_index(frame: &IndexFrame, palette: &Palette) -> PxlResult<IndexFrame> {
    let mut out = IndexFrame::canvas(frame.w * 2, frame.h * 2)?;
    let light = |i: u8| -> i64 {
        palette
            .colors
            .get(i as usize)
            .map(|c| pxl_core::Oklab::from_rgba(*c).l as i64)
            .unwrap_or(0)
    };
    let mut min_l = i64::MAX;
    let mut max_l = i64::MIN;
    for i in 1..palette.colors.len().min(256) as u8 {
        let l = light(i);
        min_l = min_l.min(l);
        max_l = max_l.max(l);
    }
    let tolerance = ((max_l - min_l).max(1) / 4).max(1200);
    // Ramp steps of one surface: equal index, or a lightness step under the
    // tolerance. Two transparent pixels are not an edge either.
    let same = |a: u8, b: u8| -> bool {
        a == b || (a != 0 && b != 0 && (light(a) - light(b)).abs() <= tolerance)
    };
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let p = frame.get(x, y);
            if p == 0 {
                continue;
            }
            // Outside the frame repeats the edge pixel, so art that fills the
            // canvas does not develop a seam along the border.
            let at = |dx: i32, dy: i32| -> u8 {
                if frame.in_bounds(x + dx, y + dy) {
                    frame.get(x + dx, y + dy)
                } else {
                    p
                }
            };
            let (a, b, c, d) = (at(0, -1), at(1, 0), at(-1, 0), at(0, 1));
            let mut cells = [p; 4];
            // Top-left: the two neighbours touching the corner agree and differ
            // from the pixel and from the opposite pair, so the corner belongs
            // to them.
            if same(c, a) && !same(c, p) && !same(c, d) && !same(a, b) {
                cells[0] = a;
            }
            if same(a, b) && !same(a, p) && !same(a, c) && !same(b, d) {
                cells[1] = b;
            }
            if same(d, c) && !same(d, p) && !same(d, b) && !same(c, a) {
                cells[2] = c;
            }
            if same(b, d) && !same(b, p) && !same(b, a) && !same(d, c) {
                cells[3] = d;
            }
            for (i, value) in cells.iter().enumerate() {
                out.set(x * 2 + (i as i32 % 2), y * 2 + (i as i32 / 2), *value);
            }
        }
    }
    Ok(out)
}

/// Reconstruct region outlines as vector loops and rasterise them at `k`.
pub fn contour(frame: &IndexFrame, k: u16, graph: &SpriteGraph) -> PxlResult<IndexFrame> {
    crate::contour::reconstruct(frame, k, graph)
}

/// Two-pass 3-4 chamfer distance transform over the zero pixels of `field`.
pub(crate) fn chamfer(field: &mut [i32], w: i32, h: i32) {
    let far = i32::MAX / 4;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if field[i] == 0 {
                continue;
            }
            let mut d = field[i];
            for (dx, dy, step) in [(-1, 0, UNIT), (0, -1, UNIT), (-1, -1, DIAG), (1, -1, DIAG)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let neighbour = field[(ny * w + nx) as usize];
                if neighbour < far {
                    d = d.min(neighbour.saturating_add(step));
                }
            }
            field[i] = d;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = (y * w + x) as usize;
            if field[i] == 0 {
                continue;
            }
            let mut d = field[i];
            for (dx, dy, step) in [(1, 0, UNIT), (0, 1, UNIT), (1, 1, DIAG), (-1, 1, DIAG)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let neighbour = field[(ny * w + nx) as usize];
                if neighbour < far {
                    d = d.min(neighbour.saturating_add(step));
                }
            }
            field[i] = d;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    fn frame(rows: &[&str]) -> IndexFrame {
        let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
        pxl_core::decode_matrix(&rows).unwrap()
    }

    fn palette() -> Palette {
        let mut p = Palette {
            id: "t".into(),
            colors: alloc::vec![
                pxl_core::Rgba::TRANSPARENT,
                pxl_core::Rgba::rgb(32, 32, 40),
                pxl_core::Rgba::rgb(240, 240, 232),
            ],
            ramps: Default::default(),
            cycles: Default::default(),
            special: pxl_core::Special::default(),
        };
        p.special.outline = 1;
        p
    }

    #[test]
    fn upscalers_keep_the_size_and_never_invent_a_colour() {
        let f = frame(&["0220", "0220", "0220", "0220"]);
        let pal = palette();
        let graph = SpriteGraph::build(&f, &pal, None);
        for (mode, k) in [
            (Mode::Copy, 2),
            (Mode::Epx, 2),
            (Mode::Scale3x, 3),
            (Mode::Xbr, 2),
            (Mode::Contour, 2),
            (Mode::Contour, 3),
            (Mode::Hybrid, 2),
        ] {
            let out = run(&f, k, mode, &graph, &pal).unwrap();
            assert_eq!((out.w, out.h), (f.w * k, f.h * k), "{mode:?}");
            for &p in &out.pixels {
                assert!(p == 0 || p == 1 || p == 2, "{mode:?} invented colour {p}");
            }
        }
    }

    #[test]
    fn scale3x_turns_a_single_pixel_into_a_solid_block() {
        let f = frame(&["000", "020", "000"]);
        let out = scale3x(&f).unwrap();
        assert_eq!(out.get(4, 4), 2);
        assert_eq!(out.count_opaque(), 9);
        for y in 3..6 {
            for x in 3..6 {
                assert_eq!(out.get(x, y), 2);
            }
        }
    }

    #[test]
    fn contour_never_grows_into_transparency() {
        let f = frame(&["000000", "022200", "022200", "022330", "000000"]);
        let pal = palette();
        let graph = SpriteGraph::build(&f, &pal, None);
        for k in [2u16, 3, 4] {
            let out = run(&f, k, Mode::Contour, &graph, &pal).unwrap();
            let base = pxl_core::scale_copy(&f, k).unwrap();
            for y in 0..out.h as i32 {
                for x in 0..out.w as i32 {
                    if base.get(x, y) == 0 {
                        assert_eq!(out.get(x, y), 0, "k={k} grew at ({x}, {y})");
                    }
                }
            }
        }
    }

    #[test]
    fn contour_at_2x_re_rasterises_a_colour_boundary() {
        let f = frame(&[
            "22233333", "22233333", "22333333", "22333333", "23333333", "23333333", "33333333",
            "33333333",
        ]);
        let pal = palette();
        let mut graph = SpriteGraph::build(&f, &pal, None);
        graph.protect.fill(false);
        let out = run(&f, 2, Mode::Contour, &graph, &pal).unwrap();
        assert_ne!(out, pxl_core::scale_copy(&f, 2).unwrap());
        assert!(out.pixels.iter().all(|p| *p <= 3));
    }

    #[test]
    fn contour_keeps_the_body_and_rounds_the_outer_corner() {
        let mut rows: Vec<String> = (0..8).map(|_| "0".repeat(8)).collect();
        let mut f = pxl_core::decode_matrix(&rows).unwrap();
        for y in 1..7 {
            for x in 1..7 {
                f.set(x, y, 2);
            }
        }
        rows.clear();
        let pal = palette();
        let graph = SpriteGraph::build(&f, &pal, None);
        let out = run(&f, 4, Mode::Contour, &graph, &pal).unwrap();
        assert_eq!(out.get(4 * 1 + 2, 4 * 1 + 2), 2, "inside stays filled");
        assert_eq!(out.get(4 * 3, 4 * 3), 2, "the body core stays filled");
    }

    #[test]
    fn scale3x_does_not_copy_the_opposite_edge_into_an_edge_centre() {
        // At the centre, top/left are 2 and bottom/right are 3. The old rule
        // wrote 3 into the top edge centre, creating a detached pixel.
        let f = frame(&["222", "223", "233"]);
        let out = scale3x(&f).unwrap();
        assert_eq!(out.get(4, 3), 2);
        assert_eq!(
            out.get(5, 5),
            3,
            "the bottom-right corner still follows its edge"
        );
    }

    #[test]
    fn a_protected_pixel_is_copied_verbatim() {
        let f = frame(&["000000", "022200", "022230", "022200", "000000"]);
        let pal = palette();
        let graph = SpriteGraph::build(&f, &pal, None);
        assert!(!graph.editable(4, 2), "the lone pixel is a kept feature");
        for mode in [Mode::Scale3x, Mode::Contour, Mode::Hybrid] {
            let out = run(&f, 3, mode, &graph, &pal).unwrap();
            for y in 6..9 {
                for x in 12..15 {
                    assert_eq!(out.get(x, y), 3, "{mode:?} at ({x}, {y})");
                }
            }
        }
    }
}
