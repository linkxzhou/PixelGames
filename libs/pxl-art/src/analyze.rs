//! Reading the sprite before changing it: connected regions, which pixels are
//! too small to survive rounding, which ones are one pixel wide, and whether
//! the art is mirror-symmetric.
//!
//! Every later stage consults this, which is why an eye or an antenna keeps its
//! shape while a large area is free to round its corners.

use alloc::vec;
use alloc::vec::Vec;

use pxl_core::{IndexFrame, Palette};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Outline,
    Metal,
    Wood,
    Stone,
    Water,
    Foliage,
    Cloth,
    Skin,
    Hair,
    Glass,
    Emissive,
    Flat,
    Organic,
}

impl Material {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "outline" => Self::Outline,
            "metal" => Self::Metal,
            "wood" => Self::Wood,
            "stone" => Self::Stone,
            "water" => Self::Water,
            "foliage" => Self::Foliage,
            "cloth" => Self::Cloth,
            "skin" => Self::Skin,
            "hair" => Self::Hair,
            "glass" => Self::Glass,
            "emissive" => Self::Emissive,
            "flat" => Self::Flat,
            "organic" => Self::Organic,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Outline => "outline",
            Self::Metal => "metal",
            Self::Wood => "wood",
            Self::Stone => "stone",
            Self::Water => "water",
            Self::Foliage => "foliage",
            Self::Cloth => "cloth",
            Self::Skin => "skin",
            Self::Hair => "hair",
            Self::Glass => "glass",
            Self::Emissive => "emissive",
            Self::Flat => "flat",
            Self::Organic => "organic",
        }
    }

    /// Hard materials keep corners; soft and planar materials may round.
    pub fn keeps_corners(self) -> bool {
        matches!(self, Self::Metal | Self::Glass | Self::Outline | Self::Stone)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Left half mirrors the right half.
    Vertical,
    /// Top half mirrors the bottom half.
    Horizontal,
}

#[derive(Clone, Debug)]
pub struct SpriteGraph {
    pub w: u16,
    pub h: u16,
    pub materials: Vec<Material>,
    /// Pixels whose shape must not be altered: 1-px-wide runs, areas of four
    /// pixels or fewer, and every pixel of a material that keeps corners.
    pub protect: Vec<bool>,
    pub explicit_protect: Vec<bool>,
    pub thin: Vec<bool>,
    pub small: Vec<bool>,
    pub symmetry: Option<Axis>,
    pub regions: usize,
}

impl SpriteGraph {
    pub fn at(&self, x: i32, y: i32) -> Material {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            Material::Organic
        } else {
            self.materials[(y as usize) * (self.w as usize) + (x as usize)]
        }
    }

    pub fn explicitly_protected(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return true;
        }
        self.explicit_protect[(y as usize) * (self.w as usize) + (x as usize)]
    }

    fn protected(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            true
        } else {
            self.protect[(y as usize) * (self.w as usize) + (x as usize)]
        }
    }

    /// Pixels a later stage may round or smooth.
    pub fn editable(&self, x: i32, y: i32) -> bool {
        !self.protected(x, y)
    }

    /// Build the graph. `materials` overrides the heuristic when the spec
    /// declares per-pixel materials; it must be `w * h` long.
    pub fn build(frame: &IndexFrame, palette: &Palette, materials: Option<&[Material]>) -> Self {
        let w = frame.w;
        let h = frame.h;
        let n = (w as usize) * (h as usize);
        let mut mats = vec![Material::Organic; n];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let i = (y as usize) * (w as usize) + (x as usize);
                if frame.pixels[i] == 0 {
                    continue;
                }
                mats[i] = match materials {
                    Some(list) if list.len() == n => list[i],
                    _ if frame.pixels[i] == palette.special.outline => Material::Outline,
                    _ => Material::Organic,
                };
            }
        }
        // Region labels for the same-index 4-connected areas.
        let mut label = vec![u32::MAX; n];
        let mut sizes: Vec<u32> = Vec::new();
        let mut stack: Vec<(i32, i32)> = Vec::new();
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let start = (y as usize) * (w as usize) + (x as usize);
                if frame.pixels[start] == 0 || label[start] != u32::MAX {
                    continue;
                }
                let id = sizes.len() as u32;
                let color = frame.pixels[start];
                let mut size = 0u32;
                label[start] = id;
                stack.push((x, y));
                while let Some((cx, cy)) = stack.pop() {
                    size += 1;
                    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let Some(i) = frame.idx(cx + dx, cy + dy) else {
                            continue;
                        };
                        if label[i] == u32::MAX && frame.pixels[i] == color {
                            label[i] = id;
                            stack.push((cx + dx, cy + dy));
                        }
                    }
                }
                sizes.push(size);
            }
        }
        let mut small = vec![false; n];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let i = (y as usize) * (w as usize) + (x as usize);
                if frame.pixels[i] == 0 {
                    continue;
                }
                let id = label[i];
                if id != u32::MAX && sizes[id as usize] <= 4 {
                    small[i] = true;
                }
            }
        }
        // One-pixel-wide runs: no same-colour companion in one axis.
        let mut thin = vec![false; n];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let i = (y as usize) * (w as usize) + (x as usize);
                let c = frame.pixels[i];
                if c == 0 {
                    continue;
                }
                let across = frame.get(x - 1, y) != c && frame.get(x + 1, y) != c;
                let down = frame.get(x, y - 1) != c && frame.get(x, y + 1) != c;
                thin[i] = across || down;
            }
        }
        let mut protect = vec![false; n];
        for i in 0..n {
            protect[i] = frame.pixels[i] != 0 && (small[i] || thin[i] || mats[i].keeps_corners());
        }
        // A few editable pixels around a protected cluster are protected too,
        // so rounding cannot shave the corner off a kept feature.
        let mut grown = protect.clone();
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let i = (y as usize) * (w as usize) + (x as usize);
                if !protect[i] {
                    continue;
                }
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if let Some(j) = frame.idx(x + dx, y + dy) {
                            if frame.pixels[j] != 0 && mats[j] != Material::Outline {
                                grown[j] = true;
                            }
                        }
                    }
                }
            }
        }
        Self {
            w,
            h,
            materials: mats,
            protect: grown,
            explicit_protect: vec![false; n],
            thin,
            small,
            symmetry: detect_symmetry(frame),
            regions: sizes.len(),
        }
    }
}

/// Mirror tolerance: 3% of the opaque pixels, at least one pixel, so a clean
/// symmetric sprite is recognised and an intentionally lopsided one is not.
fn detect_symmetry(frame: &IndexFrame) -> Option<Axis> {
    let opaque = frame.count_opaque();
    if opaque < 8 {
        return None;
    }
    let budget = (opaque * 3 / 100).max(1);
    let w = frame.w as i32;
    let h = frame.h as i32;
    let mut vertical = 0usize;
    let mut horizontal = 0usize;
    for y in 0..h {
        for x in 0..w {
            if x < w / 2 && frame.get(x, y) != frame.get(w - 1 - x, y) {
                vertical += 1;
            }
            if y < h / 2 && frame.get(x, y) != frame.get(x, h - 1 - y) {
                horizontal += 1;
            }
        }
    }
    if vertical <= budget && vertical <= horizontal {
        Some(Axis::Vertical)
    } else if horizontal <= budget {
        Some(Axis::Horizontal)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};

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
                pxl_core::Rgba::rgb(232, 64, 64),
            ],
            ramps: Default::default(),
            cycles: Default::default(),
            special: pxl_core::Special::default(),
        };
        p.special.outline = 1;
        p
    }

    #[test]
    fn a_single_pixel_is_protected() {
        let g = SpriteGraph::build(&frame(&["000", "020", "000"]), &palette(), None);
        assert!(g.small[4]);
        assert!(g.protect[4]);
        assert_eq!(g.regions, 1);
    }

    #[test]
    fn a_large_block_is_editable_including_its_corners() {
        let g = SpriteGraph::build(
            &frame(&["33333", "33333", "33333", "33333", "33333"]),
            &palette(),
            None,
        );
        assert!(g.editable(2, 2), "the middle of a block may round");
        assert!(
            g.editable(0, 0),
            "a soft material rounds its silhouette corner"
        );
    }

    #[test]
    fn metal_keeps_corners() {
        let f = frame(&["33333", "33333", "33333", "33333", "33333"]);
        let mats = vec![Material::Metal; 25];
        let g = SpriteGraph::build(&f, &palette(), Some(&mats));
        assert!(!g.editable(2, 2), "metal never rounds");
    }

    #[test]
    fn symmetric_art_is_detected() {
        let g = SpriteGraph::build(
            &frame(&["03030", "03330", "03330", "00300"]),
            &palette(),
            None,
        );
        assert_eq!(g.symmetry, Some(Axis::Vertical));
        let lop = SpriteGraph::build(
            &frame(&["30000", "33000", "33300", "30000"]),
            &palette(),
            None,
        );
        assert_eq!(lop.symmetry, None);
    }
}
