//! Colour quantisation in OKLab, all integer.
//!
//! * `nearest`: map every colour to the closest palette entry.
//! * `median_cut`: build a palette of at most `n` colours by splitting the
//!   box with the widest OKLab range at its median, then refine it with a few
//!   rounds of k-means (Lloyd) so each entry sits at its cluster mean.
//! * Ordered dithering with a 4x4 Bayer matrix: the threshold nudges the
//!   colour toward the second-nearest entry, which only ever produces a
//!   regular pattern of two palette colours, never noise.

use alloc::vec;
use alloc::vec::Vec;

use pxl_core::{Oklab, Palette, Rgba};

/// Bayer 4x4, values 0..16.
pub const BAYER4: [[i32; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// Palette entries as OKLab, with transparent and missing entries skipped.
pub struct LabPalette {
    pub entries: Vec<(u8, Oklab)>,
}

impl LabPalette {
    pub fn new(palette: &Palette) -> Self {
        let entries = palette
            .colors
            .iter()
            .enumerate()
            .take(256)
            .filter(|(_, c)| c.a != 0)
            .map(|(i, c)| (i as u8, Oklab::from_rgba(*c)))
            .collect();
        Self { entries }
    }

    /// Nearest and second-nearest entries with their squared distances.
    pub fn nearest2(&self, lab: Oklab) -> Option<((u8, i64), (u8, i64))> {
        let mut best: Option<(u8, i64)> = None;
        let mut second: Option<(u8, i64)> = None;
        for (i, e) in &self.entries {
            let d = e.dist2(lab);
            if best.map_or(true, |(_, bd)| d < bd) {
                second = best;
                best = Some((*i, d));
            } else if second.map_or(true, |(_, sd)| d < sd) {
                second = Some((*i, d));
            }
        }
        let b = best?;
        Some((b, second.unwrap_or(b)))
    }

    pub fn nearest(&self, lab: Oklab) -> u8 {
        self.nearest2(lab).map(|(b, _)| b.0).unwrap_or(0)
    }
}

/// Map an RGBA buffer onto `palette`. Alpha below 128 is transparent.
/// `dither` is 0 (off) to 100 (full Bayer strength).
pub fn map_to_palette(w: usize, h: usize, rgba: &[u8], palette: &Palette, dither: u8) -> Vec<u8> {
    let lab = LabPalette::new(palette);
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            if rgba[i + 3] < 128 {
                continue;
            }
            let c = Oklab::from_rgba(Rgba::rgb(rgba[i], rgba[i + 1], rgba[i + 2]));
            out[y * w + x] = pick(&lab, c, x, y, dither);
        }
    }
    out
}

/// One pixel: nearest entry, or with dithering the second entry when the
/// colour sits far enough toward it for this cell of the Bayer matrix.
pub fn pick(lab: &LabPalette, c: Oklab, x: usize, y: usize, dither: u8) -> u8 {
    let Some(((a, da), (b, db))) = lab.nearest2(c) else {
        return 0;
    };
    if dither == 0 || a == b {
        return a;
    }
    // Share of the way from `a` to `b`, 0..=8 of 16 (b is never nearer).
    let (ra, rb) = (isqrt(da), isqrt(db));
    if ra + rb == 0 {
        return a;
    }
    let t = ra * 16 / (ra + rb);
    let threshold = BAYER4[y % 4][x % 4] as i64;
    let t = t * dither as i64 / 100;
    if t * 2 > threshold + 1 {
        b
    } else {
        a
    }
}

fn isqrt(v: i64) -> i64 {
    if v <= 0 {
        return 0;
    }
    let mut x = v;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + v / x) / 2;
    }
    x
}

#[derive(Clone, Copy)]
struct Sample {
    lab: [i32; 3],
    weight: u32,
}

/// Build at most `n` colours (index 0 stays transparent) that represent the
/// opaque pixels of `rgba`. Deterministic: ties split on the lowest channel.
pub fn median_cut(rgba: &[u8], n: usize, kmeans_rounds: u32) -> Vec<Rgba> {
    let mut hist: Vec<(u32, u32)> = Vec::new();
    for px in rgba.chunks_exact(4) {
        if px[3] < 128 {
            continue;
        }
        let key = (px[0] as u32) << 16 | (px[1] as u32) << 8 | px[2] as u32;
        hist.push((key, 1));
    }
    hist.sort_unstable_by_key(|e| e.0);
    let mut uniq: Vec<(u32, u32)> = Vec::new();
    for (k, c) in hist {
        match uniq.last_mut() {
            Some(last) if last.0 == k => last.1 += c,
            _ => uniq.push((k, c)),
        }
    }
    let samples: Vec<Sample> = uniq
        .iter()
        .map(|(k, c)| {
            let o = Oklab::from_rgba(Rgba::rgb((k >> 16) as u8, (k >> 8) as u8, *k as u8));
            Sample {
                lab: [o.l, o.a, o.b],
                weight: *c,
            }
        })
        .collect();
    let mut colors = vec![Rgba::TRANSPARENT];
    if samples.is_empty() || n == 0 {
        return colors;
    }
    let mut boxes: Vec<Vec<Sample>> = vec![samples];
    while boxes.len() < n {
        // Split the box with the widest weighted range.
        let mut pick: Option<(usize, usize, i64)> = None;
        for (bi, b) in boxes.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            for ch in 0..3 {
                let lo = b.iter().map(|s| s.lab[ch]).min().unwrap_or(0);
                let hi = b.iter().map(|s| s.lab[ch]).max().unwrap_or(0);
                // Lightness counts double: it matters most for pixel art.
                let range = (hi - lo) as i64 * if ch == 0 { 2 } else { 1 };
                if range > 0 && pick.map_or(true, |(_, _, r)| range > r) {
                    pick = Some((bi, ch, range));
                }
            }
        }
        let Some((bi, ch, _)) = pick else {
            break;
        };
        let mut b = boxes.swap_remove(bi);
        b.sort_by(|x, y| x.lab[ch].cmp(&y.lab[ch]).then(x.lab.cmp(&y.lab)));
        let total: u64 = b.iter().map(|s| s.weight as u64).sum();
        let mut acc = 0u64;
        let mut cut = 1;
        for (i, s) in b.iter().enumerate() {
            acc += s.weight as u64;
            if acc * 2 >= total {
                cut = (i + 1).clamp(1, b.len() - 1);
                break;
            }
        }
        let rest = b.split_off(cut);
        boxes.push(b);
        boxes.push(rest);
        boxes.sort_by_key(|b| b.first().map(|s| s.lab).unwrap_or([0; 3]));
    }
    let all: Vec<Sample> = boxes.iter().flatten().copied().collect();
    let mut centres: Vec<[i64; 3]> = boxes.iter().map(|b| mean(b)).collect();
    for _ in 0..kmeans_rounds {
        let mut sums = vec![[0i64; 4]; centres.len()];
        for s in &all {
            let k = closest(&centres, s.lab);
            for ch in 0..3 {
                sums[k][ch] += s.lab[ch] as i64 * s.weight as i64;
            }
            sums[k][3] += s.weight as i64;
        }
        let mut moved = false;
        for (c, s) in centres.iter_mut().zip(&sums) {
            if s[3] == 0 {
                continue;
            }
            let next = [s[0] / s[3], s[1] / s[3], s[2] / s[3]];
            moved |= next != *c;
            *c = next;
        }
        if !moved {
            break;
        }
    }
    for c in centres {
        let rgb = Oklab {
            l: c[0] as i32,
            a: c[1] as i32,
            b: c[2] as i32,
        }
        .to_rgba();
        if !colors.contains(&rgb) {
            colors.push(rgb);
        }
    }
    colors
}

fn mean(b: &[Sample]) -> [i64; 3] {
    let w: i64 = b.iter().map(|s| s.weight as i64).sum::<i64>().max(1);
    let mut out = [0i64; 3];
    for s in b {
        for ch in 0..3 {
            out[ch] += s.lab[ch] as i64 * s.weight as i64;
        }
    }
    [out[0] / w, out[1] / w, out[2] / w]
}

fn closest(centres: &[[i64; 3]], lab: [i32; 3]) -> usize {
    let mut best = (0usize, i64::MAX);
    for (i, c) in centres.iter().enumerate() {
        let d: i64 = (0..3).map(|ch| (c[ch] - lab[ch] as i64).pow(2)).sum();
        if d < best.1 {
            best = (i, d);
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pal(colors: &[Rgba]) -> Palette {
        let mut all = alloc::vec![Rgba::TRANSPARENT];
        all.extend_from_slice(colors);
        Palette {
            id: "t".into(),
            colors: all,
            ramps: Default::default(),
            cycles: Default::default(),
            special: pxl_core::Special::default(),
        }
    }

    #[test]
    fn nearest_mapping_and_alpha() {
        let p = pal(&[Rgba::rgb(0, 0, 0), Rgba::rgb(255, 255, 255)]);
        let px = [10, 10, 10, 255, 250, 250, 250, 255, 250, 250, 250, 10];
        assert_eq!(map_to_palette(3, 1, &px, &p, 0), alloc::vec![1, 2, 0]);
    }

    #[test]
    fn a_mid_grey_dithers_into_a_regular_two_colour_pattern() {
        let p = pal(&[Rgba::rgb(0, 0, 0), Rgba::rgb(255, 255, 255)]);
        let grey: Vec<u8> = (0..16).flat_map(|_| [118u8, 118, 118, 255]).collect();
        let flat = map_to_palette(4, 4, &grey, &p, 0);
        assert!(flat.iter().all(|v| *v == flat[0]));
        let d = map_to_palette(4, 4, &grey, &p, 100);
        let whites = d.iter().filter(|v| **v == 2).count();
        assert!((4..=12).contains(&whites), "{d:?}");
        assert_eq!(map_to_palette(4, 4, &grey, &p, 100), d);
    }

    #[test]
    fn median_cut_finds_the_dominant_colours() {
        let mut px = Vec::new();
        for i in 0..64 {
            let c: [u8; 4] = match i % 4 {
                0 => [200, 30, 30, 255],
                1 => [205, 32, 28, 255],
                2 => [20, 40, 200, 255],
                _ => [22, 38, 205, 255],
            };
            px.extend_from_slice(&c);
        }
        let colors = median_cut(&px, 2, 4);
        assert_eq!(colors.len(), 3);
        assert!(colors[1..].iter().any(|c| c.r > 150 && c.b < 80));
        assert!(colors[1..].iter().any(|c| c.b > 150 && c.r < 80));
        assert_eq!(median_cut(&px, 2, 4), colors);
    }
}
