//! Ramps for shading. A colour the palette already places on a ramp keeps that
//! ramp; any other colour gets a generated one in OKLab with a hue shift
//! (shadows lean cool, lights lean warm), which is how a hand-picked pixel-art
//! ramp differs from "the same colour, darker".
//!
//! New colours are appended only when no existing colour is close enough, and
//! never past `max_colors`, so the palette stays text-encodable.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use pxl_core::{Oklab, Palette, Rgba};

/// Per-colour shading steps. A colour with no ramp maps to itself.
#[derive(Clone, Debug)]
pub struct ShadeMap {
    pub deep: [u8; 256],
    pub shadow: [u8; 256],
    pub light: [u8; 256],
}

impl ShadeMap {
    pub fn identity() -> Self {
        let mut ids = [0u8; 256];
        for (i, v) in ids.iter_mut().enumerate() {
            *v = i as u8;
        }
        Self {
            deep: ids,
            shadow: ids,
            light: ids,
        }
    }

    pub fn has_ramp(&self, c: u8) -> bool {
        let i = c as usize;
        self.shadow[i] != c || self.light[i] != c || self.deep[i] != c
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RampOptions {
    /// 2 = shadow + base, 3 adds a light step, 4 adds a deep shadow.
    pub levels: u8,
    /// 0..=100: how far shadows lean cool and lights lean warm.
    pub hue_shift: u8,
    /// Palette size cap. 36 keeps documents text-encodable.
    pub max_colors: usize,
    /// Generate ramps for colours that have none. Off = only declared ramps.
    pub generate: bool,
}

impl Default for RampOptions {
    fn default() -> Self {
        Self {
            levels: 4,
            hue_shift: 40,
            max_colors: 36,
            generate: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RampReport {
    pub appended: usize,
    pub generated: Vec<u8>,
    pub declared: Vec<u8>,
    /// Colours that needed a ramp but hit the palette cap.
    pub skipped: Vec<u8>,
}

/// Lightness step between ramp levels, Q16.
const STEP: i32 = 7200;
const DARK_STEP: i32 = 5200;
/// Below this distance (Q16 units squared) a palette colour is reused instead
/// of appending a near-duplicate.
const REUSE_DIST2: i64 = 1400 * 1400;

/// Build shading steps for every colour in `usage` (pixel counts per index),
/// most used first. Returns the possibly extended palette.
pub fn build_shades(
    palette: &Palette,
    usage: &[u32; 256],
    opts: RampOptions,
) -> (Palette, ShadeMap, RampReport) {
    let mut pal = palette.clone();
    let mut map = ShadeMap::identity();
    let mut report = RampReport::default();
    let mut order: Vec<u8> = (1..pal.colors.len().min(256))
        .map(|i| i as u8)
        .filter(|&i| usage[i as usize] > 0 && i != palette.special.outline)
        .collect();
    order.sort_by(|a, b| usage[*b as usize].cmp(&usage[*a as usize]).then(a.cmp(b)));
    for c in order {
        if let [only] = pal.ramps_containing(c).as_slice() {
            let name = String::from(*only);
            if pal.ramps[&name].len() >= 2 {
                map.deep[c as usize] = pal.shift_in_ramp(&name, c, -2);
                map.shadow[c as usize] = pal.shift_in_ramp(&name, c, -1);
                map.light[c as usize] = pal.shift_in_ramp(&name, c, 1);
                report.declared.push(c);
                continue;
            }
        }
        if !opts.generate {
            continue;
        }
        let base = Oklab::from_rgba(pal.colors[c as usize]);
        let step = if base.l < 22000 { DARK_STEP } else { STEP };
        let shift = opts.hue_shift.min(100) as i32;
        let shadow = tone(base, -step, shift * 35 / 100, Tone::Cool);
        let deep = tone(base, -step * 2, shift * 60 / 100, Tone::Cool);
        let light = tone(base, step * 3 / 4, shift * 30 / 100, Tone::Warm);
        let before = pal.colors.len();
        let mut ok = true;
        let mut ramp: Vec<u8> = Vec::new();
        let take = |pal: &mut Palette, lab: Oklab| -> Option<u8> {
            let found = nearest(pal, lab);
            if let Some(i) = found {
                return Some(i);
            }
            if pal.colors.len() >= opts.max_colors.min(256) {
                return None;
            }
            pal.colors.push(lab.to_rgba());
            Some((pal.colors.len() - 1) as u8)
        };
        let s = take(&mut pal, shadow);
        let d = if opts.levels >= 4 {
            take(&mut pal, deep)
        } else {
            s
        };
        let l = if opts.levels >= 3 {
            take(&mut pal, light)
        } else {
            Some(c)
        };
        match (d, s, l) {
            (Some(d), Some(s), Some(l)) => {
                map.deep[c as usize] = d;
                map.shadow[c as usize] = s;
                map.light[c as usize] = l;
                for i in [d, s, c, l] {
                    if !ramp.contains(&i) {
                        ramp.push(i);
                    }
                }
            }
            _ => ok = false,
        }
        if !ok {
            pal.colors.truncate(before);
            report.skipped.push(c);
            continue;
        }
        report.appended += pal.colors.len() - before;
        report.generated.push(c);
        if ramp.len() >= 2 {
            pal.ramps.insert(format!("auto_{c}"), ramp);
        }
    }
    (pal, map, report)
}

/// Ramps for every visible colour except the outline, with each counted once.
/// This is what `pxl palette ramp` runs: with no sprite to count, order falls
/// back to palette index, so the result depends on the palette alone.
pub fn ramp_palette(palette: &Palette, opts: RampOptions) -> (Palette, ShadeMap, RampReport) {
    let mut usage = [0u32; 256];
    for (i, u) in usage
        .iter_mut()
        .enumerate()
        .take(palette.colors.len().min(256))
        .skip(1)
    {
        *u = (palette.colors.len() - i) as u32;
    }
    build_shades(palette, &usage, opts)
}

#[derive(Clone, Copy)]
enum Tone {
    Cool,
    Warm,
}

/// Move lightness by `dl` and lean the hue `strength`% toward a cool blue or
/// a warm yellow, keeping chroma no larger than the base had.
fn tone(base: Oklab, dl: i32, strength: i32, toward: Tone) -> Oklab {
    let (ta, tb) = match toward {
        Tone::Cool => (-900, -3600),
        Tone::Warm => (700, 4200),
    };
    let l = (base.l + dl).clamp(3500, 63500);
    let a = base.a + (ta - base.a) * strength / 100;
    let b = base.b + (tb - base.b) * strength / 100;
    Oklab { l, a, b }
}

fn nearest(pal: &Palette, lab: Oklab) -> Option<u8> {
    let mut best: Option<(i64, u8)> = None;
    for (i, c) in pal.colors.iter().enumerate().skip(1) {
        if c.a == 0 {
            continue;
        }
        let d = Oklab::from_rgba(*c).dist2(lab);
        if d <= REUSE_DIST2 && best.map_or(true, |(bd, _)| d < bd) {
            best = Some((d, i as u8));
        }
    }
    best.map(|(_, i)| i)
}

/// Q16 lightness of a palette colour; transparent counts as 0.
pub fn lightness(pal: &Palette, c: u8) -> i32 {
    pal.colors
        .get(c as usize)
        .filter(|c| c.a != 0)
        .map(|c| Oklab::from_rgba(*c).l)
        .unwrap_or(0)
}

/// Q16 lightness of an arbitrary colour.
pub fn lightness_of(c: Rgba) -> i32 {
    Oklab::from_rgba(c).l
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        let mut p = Palette {
            id: "t".into(),
            colors: alloc::vec![
                Rgba::TRANSPARENT,
                Rgba::rgb(32, 32, 40),
                Rgba::rgb(0xE8, 0x40, 0x40),
                Rgba::rgb(0x38, 0x78, 0xE8),
            ],
            ramps: Default::default(),
            cycles: Default::default(),
            special: pxl_core::Special::default(),
        };
        p.special.outline = 1;
        p.special.flash = 1;
        p
    }

    #[test]
    fn generated_ramps_are_ordered_dark_to_light_and_validate() {
        let mut usage = [0u32; 256];
        usage[2] = 10;
        usage[3] = 5;
        usage[1] = 50;
        let (pal, map, report) = build_shades(&palette(), &usage, RampOptions::default());
        pal.validate().unwrap();
        assert_eq!(report.generated, alloc::vec![2, 3]);
        assert!(!map.has_ramp(1), "the outline gets no ramp");
        for c in [2u8, 3] {
            let l = |i: u8| lightness(&pal, i);
            assert!(l(map.deep[c as usize]) < l(map.shadow[c as usize]));
            assert!(l(map.shadow[c as usize]) < l(c));
            assert!(l(c) < l(map.light[c as usize]));
        }
        assert!(pal.colors.len() <= 36);
    }

    #[test]
    fn a_declared_ramp_wins() {
        let mut p = palette();
        p.colors.push(Rgba::rgb(0x80, 0x20, 0x20));
        p.ramps.insert("red".into(), alloc::vec![4, 2]);
        let mut usage = [0u32; 256];
        usage[2] = 1;
        let (pal, map, report) = build_shades(&p, &usage, RampOptions::default());
        assert_eq!(report.declared, alloc::vec![2]);
        assert_eq!(map.shadow[2], 4);
        assert_eq!(pal.colors.len(), 5, "nothing appended");
    }

    #[test]
    fn the_cap_is_respected() {
        let mut usage = [0u32; 256];
        usage[2] = 10;
        usage[3] = 5;
        // The first colour needs three new steps (4 -> 7); the second no longer fits.
        let opts = RampOptions {
            max_colors: 7,
            ..RampOptions::default()
        };
        let (pal, _, report) = build_shades(&palette(), &usage, opts);
        assert!(pal.colors.len() <= 7);
        assert_eq!(report.skipped, alloc::vec![3]);
    }

    #[test]
    fn ramping_a_whole_palette_covers_each_visible_colour_in_index_order() {
        let (pal, map, report) = ramp_palette(&palette(), RampOptions::default());
        pal.validate().unwrap();
        assert_eq!(
            report.generated,
            alloc::vec![2, 3],
            "the outline (1) is skipped"
        );
        assert!(map.has_ramp(2) && map.has_ramp(3) && !map.has_ramp(1));
        let again = ramp_palette(&palette(), RampOptions::default()).0;
        assert_eq!(pal, again);
    }

    #[test]
    fn same_input_same_palette() {
        let mut usage = [0u32; 256];
        usage[2] = 3;
        let a = build_shades(&palette(), &usage, RampOptions::default()).0;
        let b = build_shades(&palette(), &usage, RampOptions::default()).0;
        assert_eq!(a, b);
    }
}
