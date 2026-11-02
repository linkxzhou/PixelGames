//! Volume shading from a distance field.
//!
//! Each material region is treated as a solid: the distance to its edge rises
//! toward the middle, and the slope of that field is the surface normal. Pixels
//! in the rim band whose normal faces the light step one ramp colour up, the ones
//! facing away step one down, and the middle keeps the base colour. A round
//! shape therefore gets a lit side and a shaded side, which a flat fill lacks.
//!
//! Everything is integer: the same input gives the same bytes on every target.

use alloc::vec;
use alloc::vec::Vec;

use pxl_core::IndexFrame;

use crate::analyze::{Material, SpriteGraph};
use crate::palette::ShadeMap;
use crate::pipeline::Light;
use crate::upscale::{chamfer, UNIT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadeMode {
    /// Leave the fill colours as scaled.
    None,
    /// Light and shade the rim of every region from its distance field.
    Volume,
}

impl ShadeMode {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "none" | "off" => Self::None,
            "volume" | "on" => Self::Volume,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Volume => "volume",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShadeReport {
    /// Regions that were thick enough to carry a volume.
    pub volumes: u32,
    /// Regions left flat because they are thinner than three pixels.
    pub flat: u32,
    /// Pixels moved to the light step.
    pub lit: u32,
    /// Pixels moved to the shadow step.
    pub shaded: u32,
    /// Highlight pixels placed on metal or glass.
    pub speculars: u32,
}

/// Regions whose deepest pixel is closer than this to the edge stay flat: a
/// strip two pixels wide has no middle to separate from its rim.
const MIN_DEPTH: i32 = 2 * UNIT;
/// Regions at least this deep also get a highlight when metal or glass.
const SPECULAR_DEPTH: i32 = 3 * UNIT;
/// Cosine of the lit / shaded cut-off, Q8.
const CUT: i64 = 96;
const METAL_CUT: i64 = 64;

const SHADED: [Material; 10] = [
    Material::Organic,
    Material::Metal,
    Material::Wood,
    Material::Stone,
    Material::Water,
    Material::Foliage,
    Material::Cloth,
    Material::Skin,
    Material::Hair,
    Material::Glass,
];

const NEIGHBOURS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];

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

/// Shade `frame` in place. `mask` marks the pixels that were fill before the
/// outline was drawn, so the ring is never mistaken for art. `k` is the scale
/// from the native frame `graph` was built on.
pub fn apply(
    frame: &mut IndexFrame,
    mask: &[bool],
    k: u16,
    graph: &SpriteGraph,
    shades: &ShadeMap,
    outline: u8,
    light: Light,
) -> ShadeReport {
    let mut report = ShadeReport::default();
    let (w, h) = (frame.w as i32, frame.h as i32);
    let n = (w * h) as usize;
    if mask.len() != n || k == 0 {
        return report;
    }
    let source = frame.clone();
    let ki = k as i32;
    let mats: Vec<Material> = (0..n)
        .map(|i| graph.at((i as i32 % w) / ki, (i as i32 / w) / ki))
        .collect();
    let (lx, ly) = light.vector();
    let lmag = isqrt(((lx * lx + ly * ly) as i64) << 16);
    let far = i32::MAX / 4;

    for &m in SHADED.iter() {
        // Outline pixels are the edge of a region, not part of it: the distance
        // is measured from the line, so a kept outline shades like a stripped one.
        let member = |i: usize| mask[i] && mats[i] == m;
        let mut field = vec![0i32; n];
        let mut present = false;
        for i in 0..n {
            if member(i) {
                // Past the frame edge counts as clear space one pixel away.
                let (x, y) = (i as i32 % w, i as i32 / w);
                let on_edge = x == 0 || y == 0 || x == w - 1 || y == h - 1;
                field[i] = if on_edge { UNIT } else { far };
                present |= mats[i] == m;
            }
        }
        if !present {
            continue;
        }
        chamfer(&mut field, w, h);

        let mut label = vec![u32::MAX; n];
        let mut depth: Vec<i32> = Vec::new();
        let mut touched: Vec<bool> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        for start in 0..n {
            if !member(start) || label[start] != u32::MAX {
                continue;
            }
            let id = depth.len() as u32;
            let (mut best, mut own) = (0, false);
            label[start] = id;
            stack.push(start);
            while let Some(i) = stack.pop() {
                if field[i] < far / 2 {
                    best = best.max(field[i]);
                }
                own |= mats[i] == m;
                let (x, y) = (i as i32 % w, i as i32 / w);
                for (dx, dy) in NEIGHBOURS {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        continue;
                    }
                    let j = (ny * w + nx) as usize;
                    if member(j) && label[j] == u32::MAX {
                        label[j] = id;
                        stack.push(j);
                    }
                }
            }
            depth.push(best);
            touched.push(own);
        }
        for (id, &d) in depth.iter().enumerate() {
            if touched[id] {
                if d >= MIN_DEPTH {
                    report.volumes += 1;
                } else {
                    report.flat += 1;
                }
            }
        }

        let cut = if m == Material::Metal { METAL_CUT } else { CUT };
        let mut spec_done = vec![false; depth.len()];
        for i in 0..n {
            if !member(i) || mats[i] != m {
                continue;
            }
            let c = source.pixels[i];
            let id = label[i] as usize;
            let deep = depth[id];
            if c == 0 || c == outline || deep < MIN_DEPTH || field[i] >= far / 2 {
                continue;
            }
            let (x, y) = (i as i32 % w, i as i32 / w);
            if graph.explicitly_protected(x / ki, y / ki) {
                continue;
            }

            if matches!(m, Material::Metal | Material::Glass)
                && deep >= SPECULAR_DEPTH
                && !spec_done[id]
                && field[i] >= deep * 3 / 4
                && shades.light[c as usize] != c
            {
                spec_done[id] = true;
                frame.pixels[i] = shades.light[c as usize];
                report.speculars += 1;
                // A second pixel to the right makes a short glint, not a dot.
                if x + 1 < w {
                    let j = i + 1;
                    if member(j)
                        && mats[j] == m
                        && label[j] as usize == id
                        && field[j] >= deep * 3 / 4
                        && source.pixels[j] == c
                    {
                        frame.pixels[j] = shades.light[c as usize];
                        report.speculars += 1;
                    }
                }
                continue;
            }
            if m == Material::Glass {
                continue;
            }

            let band = (deep / 3).clamp(UNIT, 3 * UNIT);
            if field[i] > band {
                continue;
            }
            let at = |px: i32, py: i32| -> i32 {
                if px < 0 || py < 0 || px >= w || py >= h {
                    return field[i];
                }
                let v = field[(py * w + px) as usize];
                if v >= far / 2 {
                    field[i]
                } else {
                    v
                }
            };
            let gx = (at(x + 1, y) - at(x - 1, y)) as i64;
            let gy = (at(x, y + 1) - at(x, y - 1)) as i64;
            let mag = isqrt(gx * gx + gy * gy);
            if mag == 0 {
                continue;
            }
            // The outward normal is minus the gradient of the distance field.
            let score = -(gx * lx as i64 + gy * ly as i64) * 65_536 / (mag * lmag);
            let target = if score >= cut {
                if m == Material::Skin {
                    c
                } else {
                    shades.light[c as usize]
                }
            } else if score <= -cut {
                shades.shadow[c as usize]
            } else {
                c
            };
            if target != c && target != 0 {
                frame.pixels[i] = target;
                if score > 0 {
                    report.lit += 1;
                } else {
                    report.shaded += 1;
                }
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use pxl_core::{Palette, Rgba};

    fn palette() -> Palette {
        let mut p = Palette {
            id: "t".into(),
            colors: alloc::vec![
                Rgba::TRANSPARENT,
                Rgba::rgb(20, 20, 28),
                Rgba::rgb(200, 60, 60),
                Rgba::rgb(110, 30, 40),
                Rgba::rgb(240, 120, 110),
                Rgba::rgb(60, 20, 30),
            ],
            ramps: Default::default(),
            cycles: Default::default(),
            special: pxl_core::Special::default(),
        };
        p.special.outline = 1;
        p.special.flash = 1;
        p.ramps.insert("red".into(), alloc::vec![5, 3, 2, 4]);
        p
    }

    fn shades() -> ShadeMap {
        let mut map = ShadeMap::identity();
        map.deep[2] = 5;
        map.shadow[2] = 3;
        map.light[2] = 4;
        map
    }

    fn square(side: u16) -> IndexFrame {
        let rows: Vec<String> = (0..side)
            .map(|_| (0..side).map(|_| '2').collect::<String>())
            .collect();
        pxl_core::decode_matrix(&rows).unwrap()
    }

    fn run(frame: &mut IndexFrame, materials: Option<&[Material]>, light: Light) -> ShadeReport {
        let pal = palette();
        let mask: Vec<bool> = frame.pixels.iter().map(|p| *p != 0).collect();
        let graph = SpriteGraph::build(frame, &pal, materials);
        apply(frame, &mask, 1, &graph, &shades(), 1, light)
    }

    #[test]
    fn the_lit_side_goes_up_a_step_and_the_far_side_down() {
        let mut f = square(12);
        let report = run(&mut f, None, Light::TopLeft);
        assert_eq!(report.volumes, 1);
        assert_eq!(f.get(5, 0), 4, "top edge faces the light");
        assert_eq!(f.get(0, 5), 4, "left edge faces the light");
        assert_eq!(f.get(5, 11), 3, "bottom edge is in shade");
        assert_eq!(f.get(11, 5), 3, "right edge is in shade");
        assert_eq!(f.get(5, 5), 2, "the middle keeps the base colour");
        assert!(report.lit > 0 && report.shaded > 0);
    }

    #[test]
    fn a_corner_across_from_the_light_stays_on_the_base_colour() {
        let mut f = square(12);
        run(&mut f, None, Light::TopLeft);
        assert_eq!(
            f.get(11, 0),
            2,
            "the top-right corner is side-on to the light"
        );
        assert_eq!(f.get(0, 11), 2);
    }

    #[test]
    fn the_light_direction_moves_the_lit_side() {
        let mut f = square(12);
        run(&mut f, None, Light::TopRight);
        assert_eq!(f.get(11, 5), 4);
        assert_eq!(f.get(0, 5), 3);
    }

    #[test]
    fn a_strip_thinner_than_three_pixels_stays_flat() {
        let mut f =
            pxl_core::decode_matrix(&["22222222".to_string(), "22222222".to_string()]).unwrap();
        let before = f.clone();
        let report = run(&mut f, None, Light::TopLeft);
        assert_eq!(f, before);
        assert_eq!(report.flat, 1);
        assert_eq!(report.volumes, 0);
    }

    #[test]
    fn the_outline_colour_and_colours_without_a_ramp_are_untouched() {
        let rows: Vec<String> =
            alloc::vec!["111111", "133331", "133331", "133331", "133331", "111111"]
                .into_iter()
                .map(|s| s.to_string())
                .collect();
        let mut f = pxl_core::decode_matrix(&rows).unwrap();
        let before = f.clone();
        run(&mut f, None, Light::TopLeft);
        assert_eq!(
            f, before,
            "3 has no ramp in the shade map and 1 is the outline"
        );
    }

    #[test]
    fn skin_takes_a_shadow_but_never_a_light() {
        let mut f = square(12);
        let skin = alloc::vec![Material::Skin; f.pixels.len()];
        run(&mut f, Some(&skin), Light::TopLeft);
        assert_eq!(f.get(5, 0), 2, "no light step on skin");
        assert_eq!(f.get(5, 11), 3);
    }

    #[test]
    fn metal_gets_a_highlight_inside_the_rim() {
        let mut f = square(14);
        let metal = alloc::vec![Material::Metal; f.pixels.len()];
        let report = run(&mut f, Some(&metal), Light::TopLeft);
        assert!(report.speculars >= 1);
        let lit_inside = (3..11)
            .flat_map(|y| (3..11).map(move |x| (x, y)))
            .filter(|&(x, y)| f.get(x, y) == 4)
            .count();
        assert!(lit_inside >= 1 && lit_inside <= 2, "{lit_inside}");
    }

    #[test]
    fn emissive_pixels_are_left_alone() {
        let mut f = square(12);
        let before = f.clone();
        let glow = alloc::vec![Material::Emissive; f.pixels.len()];
        run(&mut f, Some(&glow), Light::TopLeft);
        assert_eq!(f, before);
    }

    #[test]
    fn shading_is_deterministic_and_stays_inside_the_palette() {
        let mut a = square(12);
        let mut b = square(12);
        run(&mut a, None, Light::TopLeft);
        run(&mut b, None, Light::TopLeft);
        assert_eq!(a, b);
        for &p in &a.pixels {
            assert!((p as usize) < palette().colors.len());
        }
    }

    #[test]
    fn a_mask_shorter_than_the_frame_does_nothing() {
        let mut f = square(12);
        let before = f.clone();
        let graph = SpriteGraph::build(&f, &palette(), None);
        let report = apply(&mut f, &[true; 4], 1, &graph, &shades(), 1, Light::TopLeft);
        assert_eq!(f, before);
        assert_eq!(report, ShadeReport::default());
    }
}
