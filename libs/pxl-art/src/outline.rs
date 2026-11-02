//! Outlines drawn after the fill has been scaled.
//!
//! Scaling a native frame that already carries an outline would give a line
//! `k` pixels thick. Instead the outer outline is removed before scaling and
//! redrawn here as a one-pixel ring, optionally in the darkest step of the
//! fill it surrounds ("selout") so the line follows the colour of the art
//! instead of being one flat dark that disappears on a dark background.

use alloc::vec;
use alloc::vec::Vec;

use pxl_core::{IndexFrame, Palette};

use crate::palette::{lightness, ShadeMap};
use crate::pipeline::Light;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutlineMode {
    /// Scale the outline with the art. Nothing is stripped or redrawn.
    Keep,
    /// Strip the outer outline and draw nothing.
    None,
    /// One-pixel ring in the palette's outline colour.
    Flat,
    /// One-pixel ring in the darkest ramp step of the neighbouring fill.
    Selout,
    /// Like `Selout`, one step lighter on the side facing the light.
    SeloutLit,
}

impl OutlineMode {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "keep" => Self::Keep,
            "none" => Self::None,
            "black" | "flat" => Self::Flat,
            "selout" => Self::Selout,
            "selout_lit" | "selout-lit" => Self::SeloutLit,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::None => "none",
            Self::Flat => "flat",
            Self::Selout => "selout",
            Self::SeloutLit => "selout_lit",
        }
    }

    /// Modes that remove the native outline before scaling.
    pub fn strips(self) -> bool {
        !matches!(self, Self::Keep)
    }

    /// Modes that need ramps for the fill colours.
    pub fn needs_ramps(self) -> bool {
        matches!(self, Self::Selout | Self::SeloutLit)
    }

    fn draws(self) -> bool {
        matches!(self, Self::Flat | Self::Selout | Self::SeloutLit)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutlineReport {
    pub painted: u32,
    pub repaired: u32,
}

const NEIGHBOURS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];

/// Remove outline-coloured pixels that touch clear space. Outline-coloured
/// pixels inside the art (an eye, a panel line) stay. Returns the stripped
/// frame and how many pixels went.
pub fn strip_outer(frame: &IndexFrame, outline: u8) -> (IndexFrame, u32) {
    let mut out = frame.clone();
    let mut removed = 0;
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            if frame.get(x, y) != outline || outline == 0 {
                continue;
            }
            let outer = NEIGHBOURS
                .iter()
                .any(|(dx, dy)| frame.in_bounds(x + dx, y + dy) && frame.get(x + dx, y + dy) == 0);
            if outer {
                out.set(x, y, 0);
                removed += 1;
            }
        }
    }
    (out, removed)
}

/// Draw the ring around every opaque pixel and close one-pixel gaps in it.
pub fn apply(
    frame: &mut IndexFrame,
    palette: &Palette,
    shades: &ShadeMap,
    mode: OutlineMode,
    light: Light,
) -> OutlineReport {
    let mut report = OutlineReport::default();
    if !mode.draws() {
        return report;
    }
    let (w, h) = (frame.w as i32, frame.h as i32);
    let source = frame.clone();
    let mut painted = vec![false; (w * h) as usize];
    let mut ring: Vec<(i32, i32, u8)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if source.get(x, y) != 0 {
                continue;
            }
            if let Some(color) = ring_color(&source, x, y, palette, shades, mode, light) {
                ring.push((x, y, color));
            }
        }
    }
    for (x, y, color) in &ring {
        frame.set(*x, *y, *color);
        painted[(*y * w + *x) as usize] = true;
        report.painted += 1;
    }
    // A clear pixel squeezed between two ring pixels on a line, with art on a
    // diagonal, is a break in the line rather than open space.
    let was_painted = |x: i32, y: i32| -> bool {
        x >= 0 && y >= 0 && x < w && y < h && painted[(y * w + x) as usize]
    };
    let mut gaps: Vec<(i32, i32, u8)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if frame.get(x, y) != 0 {
                continue;
            }
            let across = was_painted(x - 1, y) && was_painted(x + 1, y);
            let down = was_painted(x, y - 1) && was_painted(x, y + 1);
            if !across && !down {
                continue;
            }
            let art = (-1..=1).any(|dy| {
                (-1..=1).any(|dx| (dx != 0 || dy != 0) && source.get(x + dx, y + dy) != 0)
            });
            if art {
                let color = if across {
                    frame.get(x - 1, y)
                } else {
                    frame.get(x, y - 1)
                };
                gaps.push((x, y, color));
            }
        }
    }
    for (x, y, color) in &gaps {
        frame.set(*x, *y, *color);
        report.repaired += 1;
    }
    report
}

/// The colour for the ring pixel at `(x, y)`, or `None` when no art touches it.
fn ring_color(
    source: &IndexFrame,
    x: i32,
    y: i32,
    palette: &Palette,
    shades: &ShadeMap,
    mode: OutlineMode,
    light: Light,
) -> Option<u8> {
    // The darkest touching fill decides the colour; the fixed neighbour order
    // breaks ties so two runs agree.
    let mut pick: Option<(i32, u8, i32, i32)> = None;
    for (dx, dy) in NEIGHBOURS {
        let c = source.get(x + dx, y + dy);
        if c == 0 {
            continue;
        }
        let l = lightness(palette, c);
        if pick.map_or(true, |(best, ..)| l < best) {
            pick = Some((l, c, dx, dy));
        }
    }
    let (_, fill, dx, dy) = pick?;
    if mode == OutlineMode::Flat {
        return Some(palette.special.outline);
    }
    if !shades.has_ramp(fill) {
        return Some(palette.special.outline);
    }
    let (lx, ly) = light.vector();
    // The fill lies at (dx, dy) from the ring pixel, so the ring pixel sits at
    // (-dx, -dy) from the fill: lit when that points toward the light.
    let lit = -dx * lx + -dy * ly > 0;
    let color = if mode == OutlineMode::SeloutLit && lit {
        shades.shadow[fill as usize]
    } else {
        shades.deep[fill as usize]
    };
    // A fill already at the dark end of its ramp would draw an invisible line.
    if color == fill || color == 0 {
        Some(palette.special.outline)
    } else {
        Some(color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use pxl_core::Rgba;

    fn frame(rows: &[&str]) -> IndexFrame {
        let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
        pxl_core::decode_matrix(&rows).unwrap()
    }

    fn palette() -> Palette {
        let mut p = Palette {
            id: "t".into(),
            colors: alloc::vec![
                Rgba::TRANSPARENT,
                Rgba::rgb(32, 32, 40),
                Rgba::rgb(232, 64, 64),
                Rgba::rgb(120, 20, 20),
                Rgba::rgb(250, 120, 120),
            ],
            ramps: Default::default(),
            cycles: Default::default(),
            special: pxl_core::Special::default(),
        };
        p.special.outline = 1;
        p.special.flash = 1;
        p.ramps.insert("red".into(), alloc::vec![3, 2, 4]);
        p
    }

    fn shades() -> ShadeMap {
        let mut map = ShadeMap::identity();
        map.deep[2] = 3;
        map.shadow[2] = 3;
        map.light[2] = 4;
        map
    }

    #[test]
    fn strip_removes_the_rim_and_keeps_inner_outline_pixels() {
        let f = frame(&[
            "0000000", "0111110", "0121210", "0122210", "0111110", "0000000",
        ]);
        let (out, n) = strip_outer(&f, 1);
        assert_eq!(n, 14);
        assert_eq!(out.get(3, 2), 1, "the pixel inside the art stays");
        assert_eq!(out.get(1, 1), 0, "the rim is gone");
        assert_eq!(out.get(2, 2), 2);
    }

    #[test]
    fn flat_draws_a_diamond_ring() {
        let mut f = frame(&["000", "020", "000"]);
        let report = apply(
            &mut f,
            &palette(),
            &shades(),
            OutlineMode::Flat,
            Light::TopLeft,
        );
        assert_eq!(report.painted, 4);
        assert_eq!(f.get(1, 0), 1);
        assert_eq!(f.get(0, 0), 0, "corners are left open");
    }

    #[test]
    fn selout_uses_the_deep_step_and_lit_side_one_step_lighter() {
        let mut deep = frame(&["000", "020", "000"]);
        apply(
            &mut deep,
            &palette(),
            &shades(),
            OutlineMode::Selout,
            Light::TopLeft,
        );
        assert_eq!(deep.get(1, 0), 3);
        assert_eq!(deep.get(1, 2), 3);

        let mut map = shades();
        map.deep[2] = 1;
        let mut lit = frame(&["000", "020", "000"]);
        apply(
            &mut lit,
            &palette(),
            &map,
            OutlineMode::SeloutLit,
            Light::TopLeft,
        );
        assert_eq!(lit.get(1, 0), 3, "above the fill faces the top-left light");
        assert_eq!(lit.get(1, 2), 1, "below the fill is in shade");
    }

    #[test]
    fn a_colour_without_a_ramp_falls_back_to_the_outline_colour() {
        let mut f = frame(&["000", "030", "000"]);
        apply(
            &mut f,
            &palette(),
            &ShadeMap::identity(),
            OutlineMode::Selout,
            Light::TopLeft,
        );
        assert_eq!(f.get(1, 0), 1);
    }

    #[test]
    fn a_one_pixel_break_in_the_ring_is_closed() {
        let mut f = frame(&["2000", "0000", "0020", "0000"]);
        let report = apply(
            &mut f,
            &palette(),
            &shades(),
            OutlineMode::Flat,
            Light::TopLeft,
        );
        assert_eq!(report.painted, 6);
        assert_eq!(report.repaired, 1);
        assert_eq!(
            f.get(1, 1),
            1,
            "the gap between the two ring arms is closed"
        );
    }

    #[test]
    fn keep_and_none_draw_nothing() {
        for mode in [OutlineMode::Keep, OutlineMode::None] {
            let mut f = frame(&["000", "020", "000"]);
            let before = f.clone();
            let report = apply(&mut f, &palette(), &shades(), mode, Light::TopLeft);
            assert_eq!(report, OutlineReport::default());
            assert_eq!(f, before);
        }
    }
}
