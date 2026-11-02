//! Raster to pixel art: recover the pixel grid of a "fake" pixel-art image,
//! or pixelate any picture to a target size, then index it.
//!
//! Stages (all integer, all deterministic):
//! 1. `detect_grid` — find the block size and phase of an upscaled or
//!    AI-generated image from the periodicity of its edge profile.
//! 2. `expand_outlines` — contrast-aware outline expansion (after PixelOE):
//!    thin dark strokes are thickened before downscaling so they survive it.
//! 3. `downscale` — one output pixel per cell: `nearest`, `mode` (most common
//!    colour), `kcentroid` (mean of the larger of two OKLab clusters, which
//!    rejects anti-aliasing fringe) or `contrast` (the extreme that differs
//!    most from the median, which keeps 1-px detail).
//! 4. palette — map to a given palette (optionally Bayer-dithered) or build
//!    one with median cut + k-means.

use alloc::vec;
use alloc::vec::Vec;

use pxl_core::{IndexFrame, Oklab, Palette, PxlError, PxlResult, Rgba};

use crate::quantize::{map_to_palette, median_cut};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Downscale {
    Nearest,
    Mode,
    KCentroid,
    Contrast,
}

impl Downscale {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "nearest" | "center" => Self::Nearest,
            "mode" | "majority" | "dominant" => Self::Mode,
            "kcentroid" | "k-centroid" | "k_centroid" => Self::KCentroid,
            "contrast" => Self::Contrast,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Nearest => "nearest",
            Self::Mode => "mode",
            Self::KCentroid => "kcentroid",
            Self::Contrast => "contrast",
        }
    }
}

/// An RGBA picture, row-major, 4 bytes per pixel.
#[derive(Clone, Debug)]
pub struct Rgba8 {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

impl Rgba8 {
    pub fn px(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.w + x) * 4;
        [
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ]
    }
}

/// Block size and phase of an image's pixel grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub cell: usize,
    pub phase_x: usize,
    pub phase_y: usize,
    /// How strongly edges line up on the grid, 100 = no preference.
    pub score: u32,
}

fn luma(p: [u8; 4]) -> i32 {
    if p[3] < 128 {
        return -1;
    }
    (p[0] as i32 * 54 + p[1] as i32 * 183 + p[2] as i32 * 19) >> 8
}

fn diff(a: [u8; 4], b: [u8; 4]) -> u32 {
    let ta = a[3] < 128;
    let tb = b[3] < 128;
    if ta && tb {
        return 0;
    }
    if ta != tb {
        return 255;
    }
    (0..3)
        .map(|i| (a[i] as i32 - b[i] as i32).unsigned_abs())
        .max()
        .unwrap_or(0)
}

/// Edge strength between column `x-1` and `x` (or row `y-1` and `y`).
fn profiles(img: &Rgba8) -> (Vec<u64>, Vec<u64>) {
    let mut cols = vec![0u64; img.w];
    let mut rows = vec![0u64; img.h];
    for y in 0..img.h {
        for x in 0..img.w {
            let p = img.px(x, y);
            if x > 0 {
                cols[x] += diff(p, img.px(x - 1, y)) as u64;
            }
            if y > 0 {
                rows[y] += diff(p, img.px(x, y - 1)) as u64;
            }
        }
    }
    (cols, rows)
}

/// Best phase for period `p`: its density score (x100 against the mean, so
/// 200 = boundaries carry twice the average edge) and the share of all edge
/// energy that falls on the grid, in basis points. Entry 0 has no left
/// neighbour, so it is never a boundary.
fn period_score(profile: &[u64], p: usize) -> (usize, u32, u32) {
    let inner = profile.len().saturating_sub(1);
    let total: u64 = profile.iter().skip(1).sum();
    if total == 0 || p < 2 || p > inner / 2 {
        return (0, 0, 0);
    }
    let mean = total as u128 * 100 / inner as u128;
    let mut best = (0usize, 0u32, 0u32);
    for phase in 0..p {
        let (mut sum, mut n) = (0u64, 0u64);
        let mut i = if phase == 0 { p } else { phase };
        while i < profile.len() {
            sum += profile[i];
            n += 1;
            i += p;
        }
        if n == 0 {
            continue;
        }
        let score = (sum as u128 * 10_000 / n as u128 / mean.max(1)) as u32;
        let capture = (sum as u128 * 10_000 / total as u128) as u32;
        if score > best.1 {
            best = (phase, score, capture);
        }
    }
    best
}

/// Find the pixel grid. A multiple of the true cell is as dense as the cell
/// (every second boundary is still a boundary) but captures only part of the
/// edge energy; a divisor of the cell captures as much but is half as dense.
/// So the smallest divisor of the densest period that keeps most of its
/// density and almost all of its capture wins. Returns cell 1 when no period
/// stands out (the image is already native).
pub fn detect_grid(img: &Rgba8, max_cell: usize) -> Grid {
    let (cols, rows) = profiles(img);
    let limit = max_cell.min(img.w / 2).min(img.h / 2).max(1);
    let mut scores: Vec<(usize, usize, usize, u32, u32)> = Vec::new();
    for p in 2..=limit {
        let (px, sx, cx) = period_score(&cols, p);
        let (py, sy, cy) = period_score(&rows, p);
        scores.push((p, px, py, (sx + sy) / 2, (cx + cy) / 2));
    }
    let Some(best) = scores
        .iter()
        .max_by_key(|s| (s.3, core::cmp::Reverse(s.0)))
        .copied()
    else {
        return Grid {
            cell: 1,
            phase_x: 0,
            phase_y: 0,
            score: 0,
        };
    };
    if best.3 < 200 {
        return Grid {
            cell: 1,
            phase_x: 0,
            phase_y: 0,
            score: best.3,
        };
    }
    let pick = scores
        .iter()
        .find(|s| {
            best.0 % s.0 == 0
                && s.3 as u64 * 100 >= best.3 as u64 * 75
                && s.4 as u64 * 100 >= best.4 as u64 * 95
        })
        .copied()
        .unwrap_or(best);
    Grid {
        cell: pick.0,
        phase_x: pick.1 % pick.0,
        phase_y: pick.2 % pick.0,
        score: pick.3,
    }
}

/// Thicken thin dark strokes: inside a `(2r+1)^2` window with real contrast,
/// a pixel takes the darkest neighbour when dark pixels are the minority. A
/// stroke thinner than a cell would otherwise be averaged away.
pub fn expand_outlines(img: &Rgba8, radius: usize, min_contrast: i32) -> Rgba8 {
    if radius == 0 {
        return img.clone();
    }
    let mut out = img.clone();
    let r = radius as isize;
    for y in 0..img.h as isize {
        for x in 0..img.w as isize {
            let mut lo = (i32::MAX, [0u8; 4]);
            let mut hi = i32::MIN;
            let mut lums: Vec<i32> = Vec::new();
            for dy in -r..=r {
                for dx in -r..=r {
                    let (sx, sy) = (x + dx, y + dy);
                    if sx < 0 || sy < 0 || sx >= img.w as isize || sy >= img.h as isize {
                        continue;
                    }
                    let p = img.px(sx as usize, sy as usize);
                    let l = luma(p);
                    if l < 0 {
                        continue;
                    }
                    if l < lo.0 {
                        lo = (l, p);
                    }
                    hi = hi.max(l);
                    lums.push(l);
                }
            }
            let me = img.px(x as usize, y as usize);
            if luma(me) < 0 || lums.is_empty() || hi - lo.0 < min_contrast {
                continue;
            }
            let mid = (hi + lo.0) / 2;
            let dark = lums.iter().filter(|l| **l < mid).count();
            if dark * 3 < lums.len() {
                let i = (y as usize * img.w + x as usize) * 4;
                out.data[i..i + 4].copy_from_slice(&lo.1);
            }
        }
    }
    out
}

fn lab_of(p: [u8; 4]) -> [i64; 3] {
    let o = Oklab::from_rgba(Rgba::rgb(p[0], p[1], p[2]));
    [o.l as i64, o.a as i64, o.b as i64]
}

fn d2(a: [i64; 3], b: [i64; 3]) -> i64 {
    (0..3).map(|i| (a[i] - b[i]).pow(2)).sum()
}

/// One cell to one colour. Cells that are mostly clear stay clear.
fn reduce(cell: &[[u8; 4]], method: Downscale, centre: [u8; 4]) -> [u8; 4] {
    let solid: Vec<[u8; 4]> = cell.iter().copied().filter(|p| p[3] >= 128).collect();
    if solid.len() * 2 <= cell.len() {
        return [0, 0, 0, 0];
    }
    match method {
        Downscale::Nearest => {
            if centre[3] >= 128 {
                centre
            } else {
                solid[0]
            }
        }
        Downscale::Mode => {
            let mut sorted = solid.clone();
            sorted.sort_unstable();
            let mut best = (sorted[0], 0usize);
            let mut run = (sorted[0], 0usize);
            for p in sorted {
                if p == run.0 {
                    run.1 += 1;
                } else {
                    run = (p, 1);
                }
                if run.1 > best.1 {
                    best = run;
                }
            }
            best.0
        }
        Downscale::KCentroid => {
            let labs: Vec<[i64; 3]> = solid.iter().map(|p| lab_of(*p)).collect();
            let (mut a, mut b) = (labs[0], labs[0]);
            for l in &labs {
                if l[0] < a[0] {
                    a = *l;
                }
                if l[0] > b[0] {
                    b = *l;
                }
            }
            let mut members = vec![false; labs.len()];
            for _ in 0..4 {
                let (mut sa, mut na, mut sb, mut nb) = ([0i64; 3], 0i64, [0i64; 3], 0i64);
                for (i, l) in labs.iter().enumerate() {
                    members[i] = d2(*l, b) < d2(*l, a);
                    let (s, n) = if members[i] {
                        (&mut sb, &mut nb)
                    } else {
                        (&mut sa, &mut na)
                    };
                    for c in 0..3 {
                        s[c] += l[c];
                    }
                    *n += 1;
                }
                if na > 0 {
                    a = [sa[0] / na, sa[1] / na, sa[2] / na];
                }
                if nb > 0 {
                    b = [sb[0] / nb, sb[1] / nb, sb[2] / nb];
                }
            }
            let nb = members.iter().filter(|m| **m).count();
            let target = if nb * 2 > labs.len() { b } else { a };
            // The member closest to the dominant centroid: a real colour.
            let mut pick = (solid[0], i64::MAX);
            for (p, l) in solid.iter().zip(&labs) {
                let d = d2(*l, target);
                if d < pick.1 {
                    pick = (*p, d);
                }
            }
            pick.0
        }
        Downscale::Contrast => {
            let mut by: Vec<(i32, [u8; 4])> = solid.iter().map(|p| (luma(*p), *p)).collect();
            by.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.cmp(&y.1)));
            let lo = by[0];
            let hi = by[by.len() - 1];
            let med = by[by.len() / 2];
            let (dl, dh) = (med.0 - lo.0, hi.0 - med.0);
            if dl.max(dh) < 24 {
                med.1
            } else if dl >= dh {
                lo.1
            } else {
                hi.1
            }
        }
    }
}

/// Reduce `img` to `out_w x out_h` cells on the grid `(cell, phase)`; with
/// `cell == 0` the cell size is derived from the target size.
pub fn downscale(img: &Rgba8, out_w: usize, out_h: usize, method: Downscale) -> Rgba8 {
    let mut data = vec![0u8; out_w * out_h * 4];
    for oy in 0..out_h {
        let y0 = oy * img.h / out_h;
        let y1 = ((oy + 1) * img.h / out_h).max(y0 + 1).min(img.h);
        for ox in 0..out_w {
            let x0 = ox * img.w / out_w;
            let x1 = ((ox + 1) * img.w / out_w).max(x0 + 1).min(img.w);
            let mut cell = Vec::with_capacity((x1 - x0) * (y1 - y0));
            for y in y0..y1 {
                for x in x0..x1 {
                    cell.push(img.px(x, y));
                }
            }
            let centre = img.px((x0 + x1) / 2, (y0 + y1) / 2);
            let c = reduce(&cell, method, centre);
            let i = (oy * out_w + ox) * 4;
            data[i..i + 4].copy_from_slice(&c);
        }
    }
    Rgba8 {
        w: out_w,
        h: out_h,
        data,
    }
}

/// Crop to the detected grid so cells line up with `downscale`'s cells.
pub fn crop_to_grid(img: &Rgba8, grid: Grid) -> (Rgba8, usize, usize) {
    let c = grid.cell.max(1);
    let (px, py) = (grid.phase_x % c, grid.phase_y % c);
    let cols = (img.w - px) / c;
    let rows = (img.h - py) / c;
    let (w, h) = (cols * c, rows * c);
    let mut data = vec![0u8; w * h * 4];
    for y in 0..h {
        let src = ((y + py) * img.w + px) * 4;
        data[y * w * 4..(y + 1) * w * 4].copy_from_slice(&img.data[src..src + w * 4]);
    }
    (Rgba8 { w, h, data }, cols, rows)
}

#[derive(Clone, Debug)]
pub struct PixelizeOptions {
    /// Target size. `None` recovers the native grid with `detect_grid`.
    pub size: Option<(u16, u16)>,
    pub method: Downscale,
    /// Outline expansion radius in source pixels; `None` = a quarter cell.
    pub outline_radius: Option<usize>,
    /// Palette to map onto; `None` builds `colors` with median cut.
    pub palette: Option<Palette>,
    pub colors: usize,
    /// 0..=100 Bayer strength when mapping.
    pub dither: u8,
}

impl Default for PixelizeOptions {
    fn default() -> Self {
        Self {
            size: None,
            method: Downscale::KCentroid,
            outline_radius: None,
            palette: None,
            colors: 16,
            dither: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Pixelized {
    pub frame: IndexFrame,
    pub palette: Palette,
    pub grid: Grid,
    pub outline_radius: usize,
}

pub fn pixelize(img: &Rgba8, opts: &PixelizeOptions) -> PxlResult<Pixelized> {
    if img.w == 0 || img.h == 0 || img.data.len() != img.w * img.h * 4 {
        return Err(PxlError::schema(
            "pixelize: the image buffer does not match its size",
        ));
    }
    let (src, grid, w, h) = match opts.size {
        Some((w, h)) => {
            let cell = (img.w / w.max(1) as usize)
                .min(img.h / h.max(1) as usize)
                .max(1);
            (
                img.clone(),
                Grid {
                    cell,
                    phase_x: 0,
                    phase_y: 0,
                    score: 0,
                },
                w as usize,
                h as usize,
            )
        }
        None => {
            let grid = detect_grid(img, 64);
            let (cropped, cols, rows) = crop_to_grid(img, grid);
            (cropped, grid, cols, rows)
        }
    };
    if w == 0 || h == 0 || w > pxl_core::MAX_DIM as usize || h > pxl_core::MAX_DIM as usize {
        return Err(PxlError::semantic(alloc::format!(
            "pixelize.bad_size: the result would be {w}x{h}"
        ))
        .with_hint("pass a target size of at most 256x256"));
    }
    let radius = opts.outline_radius.unwrap_or(grid.cell / 4);
    let expanded = if grid.cell >= 3 {
        expand_outlines(&src, radius, 48)
    } else {
        src
    };
    let small = downscale(&expanded, w, h, opts.method);
    let palette = match &opts.palette {
        Some(p) => p.clone(),
        None => {
            let colors = median_cut(&small.data, opts.colors.clamp(1, 61), 6);
            let mut p = Palette {
                id: "pixelize".into(),
                colors,
                ramps: Default::default(),
                cycles: Default::default(),
                special: pxl_core::Special::default(),
            };
            let darkest = (1..p.colors.len())
                .min_by_key(|i| Oklab::from_rgba(p.colors[*i]).l)
                .unwrap_or(0) as u8;
            p.special.outline = darkest;
            p.special.flash = darkest;
            p
        }
    };
    let pixels = map_to_palette(w, h, &small.data, &palette, opts.dither);
    let mut frame = IndexFrame::new(w as u16, h as u16)?;
    frame.pixels = pixels;
    Ok(Pixelized {
        frame,
        palette,
        grid,
        outline_radius: radius,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 6x6 native picture blown up by `k` with an offset and a soft fringe.
    fn fake(k: usize, offset: usize) -> (Rgba8, [[u8; 4]; 36]) {
        let mut native = [[0u8; 4]; 36];
        for y in 0..6 {
            for x in 0..6 {
                native[y * 6 + x] = match (x + y) % 3 {
                    0 => [200, 40, 40, 255],
                    1 => [40, 160, 60, 255],
                    _ => [30, 30, 120, 255],
                };
            }
        }
        let n = 6 * k + offset;
        let mut data = vec![0u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                let (nx, ny) = (
                    (x.saturating_sub(offset)) / k,
                    (y.saturating_sub(offset)) / k,
                );
                let mut c = native[ny.min(5) * 6 + nx.min(5)];
                // A one-pixel blurred seam, like JPEG or AI fringing.
                if (x + k - offset % k) % k == 0 {
                    c[0] = c[0].saturating_add(12);
                }
                data[(y * n + x) * 4..(y * n + x) * 4 + 4].copy_from_slice(&c);
            }
        }
        (Rgba8 { w: n, h: n, data }, native)
    }

    #[test]
    fn the_grid_of_an_upscaled_image_is_found() {
        let (img, _) = fake(5, 0);

        assert_eq!(detect_grid(&img, 32).cell, 5);
        let (img, _) = fake(8, 3);
        let g = detect_grid(&img, 32);
        assert_eq!((g.cell, g.phase_x), (8, 3));
    }

    #[test]
    fn a_native_image_has_no_grid() {
        let mut data = Vec::new();
        for i in 0..32 * 32u32 {
            let v = (i.wrapping_mul(2654435761) >> 24) as u8;
            data.extend_from_slice(&[v, v / 2, 255 - v, 255]);
        }
        let img = Rgba8 { w: 32, h: 32, data };
        assert_eq!(detect_grid(&img, 16).cell, 1);
    }

    #[test]
    fn recovering_a_fake_image_gives_back_the_native_pixels() {
        let (img, native) = fake(6, 0);
        let opts = PixelizeOptions {
            colors: 3,
            ..Default::default()
        };
        let out = pixelize(&img, &opts).unwrap();
        assert_eq!((out.frame.w, out.frame.h), (6, 6));
        // Same colour classes as the source: equal native pixels share an index.
        for i in 0..36 {
            for j in 0..36 {
                let same = native[i] == native[j];
                assert_eq!(same, out.frame.pixels[i] == out.frame.pixels[j], "{i} {j}");
            }
        }
    }

    #[test]
    fn a_thin_dark_line_survives_downscaling_with_expansion() {
        let n = 32;
        let mut data = vec![0u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                let c = if x == 13 {
                    [10, 10, 10, 255]
                } else {
                    [230, 220, 200, 255]
                };
                data[(y * n + x) * 4..(y * n + x) * 4 + 4].copy_from_slice(&c);
            }
        }
        let img = Rgba8 { w: n, h: n, data };
        let plain = downscale(&img, 4, 4, Downscale::KCentroid);
        assert!(
            plain.data.chunks(4).all(|p| p[0] > 100),
            "the line is lost without expansion"
        );
        let opts = PixelizeOptions {
            size: Some((4, 4)),
            method: Downscale::KCentroid,
            outline_radius: Some(2),
            colors: 2,
            ..Default::default()
        };
        let out = pixelize(&img, &opts).unwrap();
        let dark = out.palette.special.outline;
        assert!(
            (0..4).all(|y| out.frame.get(1, y) == dark),
            "{:?}",
            out.frame.pixels
        );
    }

    #[test]
    fn contrast_keeps_the_minority_extreme() {
        let mut cell = vec![[200u8, 200, 200, 255]; 15];
        cell.push([0, 0, 0, 255]);
        assert_eq!(reduce(&cell, Downscale::Contrast, cell[0]), [0, 0, 0, 255]);
        assert_eq!(
            reduce(&cell, Downscale::Mode, cell[0]),
            [200, 200, 200, 255]
        );
        let flat = vec![[120u8, 120, 120, 255]; 16];
        assert_eq!(
            reduce(&flat, Downscale::Contrast, flat[0]),
            [120, 120, 120, 255]
        );
    }
}
