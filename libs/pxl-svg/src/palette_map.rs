//! Mapping SVG paint onto palette indices.
//!
//! `strict` (default) keeps the palette closed: a colour that is not in it is
//! an error that names the three nearest entries. `nearest` snaps to the
//! closest entry in OKLab and records the distance. `extend` appends unknown
//! colours until the palette cap, so a gradient can become a real ramp.
//!
//! Gradients never produce in-between colours: the band count is explicit and
//! every band is a palette entry, so the result is a hard-edged ramp band, the
//! way a hand-drawn pixel gradient looks.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use pxl_core::{Oklab, Palette, PxlError, PxlResult, Rgba};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Strict,
    Nearest,
    Extend,
}

impl Default for ColorMode {
    fn default() -> Self {
        Self::Strict
    }
}

impl ColorMode {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "strict" => Self::Strict,
            "nearest" => Self::Nearest,
            "extend" => Self::Extend,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Nearest => "nearest",
            Self::Extend => "extend",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ColorReport {
    pub exact: usize,
    pub snapped: usize,
    pub appended: usize,
    /// Colours that were not in the palette, with their round-trip.
    pub entries: Vec<String>,
}

pub struct ColorResolver {
    pub palette: Palette,
    pub mode: ColorMode,
    pub max_colors: usize,
    pub report: ColorReport,
}

fn nearest(palette: &Palette, c: Rgba) -> Option<(u8, i64)> {
    let lab = Oklab::from_rgba(c);
    palette
        .colors
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, p)| p.a != 0)
        .map(|(i, p)| (i as u8, Oklab::from_rgba(*p).dist2(lab)))
        .min_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)))
}

fn nearest_three(palette: &Palette, c: Rgba) -> String {
    let lab = Oklab::from_rgba(c);
    let mut all: Vec<(u8, i64)> = palette
        .colors
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, p)| (i as u8, Oklab::from_rgba(*p).dist2(lab)))
        .collect();
    all.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    all.iter()
        .take(3)
        .map(|(i, _)| {
            format!(
                "#{} ({i})",
                palette.colors[*i as usize].to_hex().trim_start_matches('#')
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl ColorResolver {
    pub fn new(palette: &Palette, mode: ColorMode, max_colors: usize) -> Self {
        Self {
            palette: palette.clone(),
            mode,
            max_colors: max_colors.clamp(2, 256),
            report: ColorReport::default(),
        }
    }

    /// Palette index for a paint colour. 0 (transparent) paints nothing.
    pub fn index_of(&mut self, c: Rgba) -> PxlResult<u8> {
        if c.a == 0 {
            return Ok(0);
        }
        if let Some(i) = self.palette.colors.iter().position(|p| *p == c) {
            self.report.exact += 1;
            return Ok(i as u8);
        }
        match self.mode {
            ColorMode::Strict => Err(PxlError::schema(format!(
                "svg.color_not_in_palette: {} is not in palette '{}'",
                c.to_hex(),
                self.palette.id
            ))
            .with_hint(format!(
                "nearest entries are {}; use one of them, add data-ramp, or pass --color-mode nearest",
                nearest_three(&self.palette, c)
            ))),
            ColorMode::Nearest => {
                let (i, d) = nearest(&self.palette, c).ok_or_else(|| {
                    PxlError::semantic("svg.empty_palette: the palette has no visible colours")
                })?;
                self.report.snapped += 1;
                self.report.entries.push(format!(
                    "{} -> {} (d {})",
                    c.to_hex(),
                    self.palette.colors[i as usize].to_hex(),
                    (d as f64).sqrt() as i64
                ));
                Ok(i)
            }
            ColorMode::Extend => {
                if self.palette.colors.len() >= self.max_colors {
                    let (i, _) = nearest(&self.palette, c).ok_or_else(|| {
                        PxlError::semantic("svg.empty_palette: the palette has no visible colours")
                    })?;
                    self.report.snapped += 1;
                    self.report.entries.push(format!(
                        "{} -> {} (palette cap {})",
                        c.to_hex(),
                        self.palette.colors[i as usize].to_hex(),
                        self.max_colors
                    ));
                    return Ok(i);
                }
                self.palette.colors.push(c);
                self.report.appended += 1;
                self.report.entries.push(format!("{} appended as {}", c.to_hex(), self.palette.colors.len() - 1));
                Ok((self.palette.colors.len() - 1) as u8)
            }
        }
    }

    /// `data-ramp="skin" data-step="-1"`: an entry on a declared ramp.
    pub fn ramp_index(&mut self, ramp: &str, step: i32) -> PxlResult<u8> {
        if !self.palette.ramps.contains_key(ramp) {
            return Err(PxlError::schema(format!(
                "svg.unknown_ramp: the palette declares no ramp '{ramp}'"
            ))
            .with_hint(format!(
                "declared ramps are {}",
                self.palette
                    .ramps
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        // Start from the middle entry so a negative step has room on both sides.
        let list = self.palette.ramps[ramp].clone();
        let start = (list.len() / 2) as i32;
        let at = (start + step).clamp(0, list.len() as i32 - 1) as usize;
        Ok(list[at])
    }

    /// `bands` hard steps between two palette entries, both ends included.
    pub fn bands(&mut self, a: u8, b: u8, bands: usize) -> PxlResult<Vec<u8>> {
        let bands = bands.max(2);
        let (la, lb) = (
            Oklab::from_rgba(self.palette.colors[a as usize]),
            Oklab::from_rgba(self.palette.colors[b as usize]),
        );
        let mut out = Vec::with_capacity(bands);
        for i in 0..bands {
            let t = i as i64 * 65536 / (bands as i64 - 1);
            let mix = Oklab {
                l: la.l + ((lb.l - la.l) as i64 * t / 65536) as i32,
                a: la.a + ((lb.a - la.a) as i64 * t / 65536) as i32,
                b: la.b + ((lb.b - la.b) as i64 * t / 65536) as i32,
            };
            // A gradient is an approximation by nature: interior bands snap to
            // the closest entry instead of demanding an exact colour.
            let idx = if i == 0 {
                a
            } else if i + 1 == bands {
                b
            } else {
                let (idx, _) = nearest(&self.palette, mix.to_rgba()).ok_or_else(|| {
                    PxlError::semantic("svg.empty_palette: the palette has no visible colours")
                })?;
                self.report.snapped += 1;
                idx
            };
            out.push(idx);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pal() -> Palette {
        Palette {
            id: "p".into(),
            colors: alloc::vec![
                Rgba::TRANSPARENT,
                Rgba::rgb(0, 0, 0),
                Rgba::rgb(128, 128, 128),
                Rgba::rgb(255, 255, 255),
            ],
            ramps: {
                let mut m = alloc::collections::BTreeMap::new();
                m.insert("grey".to_string(), alloc::vec![1u8, 2, 3]);
                m
            },
            cycles: Default::default(),
            special: pxl_core::Special::default(),
        }
    }

    #[test]
    fn strict_names_the_nearest_entries() {
        let mut r = ColorResolver::new(&pal(), ColorMode::Strict, 16);
        assert_eq!(r.index_of(Rgba::rgb(255, 255, 255)).unwrap(), 3);
        let e = r.index_of(Rgba::rgb(250, 10, 10)).unwrap_err();
        assert!(e.message.contains("svg.color_not_in_palette"));
        assert!(e.hint.unwrap().contains('#'));
    }

    #[test]
    fn nearest_and_extend() {
        let mut r = ColorResolver::new(&pal(), ColorMode::Nearest, 16);
        // The grey sits at similar lightness, so it is the closer entry.
        assert_eq!(r.index_of(Rgba::rgb(250, 10, 10)).unwrap(), 2);
        assert_eq!(r.report.snapped, 1);
        let mut r = ColorResolver::new(&pal(), ColorMode::Extend, 5);
        assert_eq!(r.index_of(Rgba::rgb(250, 10, 10)).unwrap(), 4);
        assert_eq!(r.palette.colors.len(), 5);
        // At the cap it snaps instead of growing.
        assert_eq!(r.index_of(Rgba::rgb(200, 10, 10)).unwrap(), 4);
        assert_eq!(r.report.appended, 1);
    }

    #[test]
    fn ramps_and_banded_gradients_stay_in_the_palette() {
        let mut r = ColorResolver::new(&pal(), ColorMode::Strict, 16);
        assert_eq!(r.ramp_index("grey", 0).unwrap(), 2);
        assert_eq!(r.ramp_index("grey", 5).unwrap(), 3);
        assert!(r.ramp_index("skin", 0).is_err());
        let bands = r.bands(1, 3, 5).unwrap();
        assert_eq!(bands, alloc::vec![1, 1, 2, 2, 3]);
        for b in bands {
            assert!((b as usize) < r.palette.colors.len());
        }
    }
}
