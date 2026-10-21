use crate::{parse_hex, PxlError, PxlResult, Rgba};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Cycle {
    pub indices: Vec<u8>,
    pub period: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Special {
    pub transparent: u8,
    pub outline: u8,
    pub flash: u8,
}

impl Default for Special {
    fn default() -> Self {
        Self {
            transparent: 0,
            outline: 1,
            flash: 15,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Palette {
    pub id: String,
    pub colors: Vec<Rgba>,
    pub ramps: BTreeMap<String, Vec<u8>>,
    pub cycles: BTreeMap<String, Cycle>,
    pub special: Special,
}

impl Palette {
    pub fn len(&self) -> usize {
        self.colors.len()
    }

    pub fn get(&self, index: u8) -> PxlResult<Rgba> {
        self.colors.get(index as usize).copied().ok_or_else(|| {
            PxlError::semantic(alloc::format!(
                "color index {index} is outside the palette ({} colors)",
                self.colors.len()
            ))
            .with_hint("index 0 is transparent; other indices must exist in colors")
        })
    }

    pub fn validate(&self) -> PxlResult<()> {
        if self.colors.is_empty() {
            return Err(PxlError::schema("palette has no colors"));
        }
        if self.colors.len() > 256 {
            return Err(PxlError::schema("palette has more than 256 colors"));
        }
        for (i, c) in self.colors.iter().enumerate() {
            if c.a != 0 && c.a != 255 {
                return Err(PxlError::schema(alloc::format!(
                    "color {i} is semi-transparent (alpha {})",
                    c.a
                ))
                .with_hint("indexed output only allows alpha 0 or 255"));
            }
        }
        if !self.colors[0].is_transparent() {
            return Err(PxlError::schema("palette index 0 must be transparent")
                .with_hint("set colors[0] to #00000000"));
        }
        for (name, ramp) in &self.ramps {
            if ramp.is_empty() {
                return Err(PxlError::schema(alloc::format!("ramp '{name}' is empty")));
            }
            for &idx in ramp {
                self.get(idx)
                    .map_err(|e| e.with_path(alloc::format!("/ramps/{name}")))?;
            }
        }
        for (name, cycle) in &self.cycles {
            if cycle.indices.is_empty() {
                return Err(PxlError::schema(alloc::format!("cycle '{name}' is empty")));
            }
            for &idx in &cycle.indices {
                self.get(idx)?;
            }
        }
        self.get(self.special.outline)?;
        self.get(self.special.flash)?;
        Ok(())
    }

    pub fn resolve_color_name(&self, name: &str) -> PxlResult<u8> {
        if let Ok(n) = name.parse::<u8>() {
            self.get(n)?;
            return Ok(n);
        }
        match name {
            "outline" => Ok(self.special.outline),
            "flash" => Ok(self.special.flash),
            "transparent" | "none" => Ok(self.special.transparent),
            other => Err(
                PxlError::semantic(alloc::format!("unknown color '{other}'"))
                    .with_hint("use a palette index, or outline / flash / transparent"),
            ),
        }
    }

    pub fn ramp_pos(&self, ramp: &str, index: u8) -> Option<usize> {
        self.ramps
            .get(ramp)
            .and_then(|r| r.iter().position(|&c| c == index))
    }

    /// Move `index` by `delta` steps inside whichever ramp contains it.
    /// Indices that are not on a ramp are left unchanged.
    pub fn shift_on_ramp(&self, index: u8, delta: i32) -> u8 {
        if index == 0 {
            return 0;
        }
        for ramp in self.ramps.values() {
            if let Some(pos) = ramp.iter().position(|&c| c == index) {
                let len = ramp.len() as i32;
                let mut next = pos as i32 + delta;
                if next < 0 {
                    next = 0;
                }
                if next >= len {
                    next = len - 1;
                }
                return ramp[next as usize];
            }
        }
        index
    }

    pub fn in_ramp(&self, ramp: &str, index: u8) -> bool {
        self.ramps
            .get(ramp)
            .map(|r| r.contains(&index))
            .unwrap_or(false)
    }

    /// Absolute distance along a named ramp. `None` if either index is absent.
    pub fn ramp_distance(&self, ramp: &str, a: u8, b: u8) -> Option<usize> {
        let ra = self.ramp_pos(ramp, a)?;
        let rb = self.ramp_pos(ramp, b)?;
        Some(ra.abs_diff(rb))
    }

    pub fn cycle_map(&self, name: &str, step: i32) -> PxlResult<BTreeMap<u8, u8>> {
        let cycle = self.cycles.get(name).ok_or_else(|| {
            PxlError::semantic(alloc::format!("unknown palette cycle '{name}'"))
                .with_hint("cycles are declared on the palette; try `pxl palette show`")
        })?;
        let n = cycle.indices.len() as i32;
        if n == 0 {
            return Err(PxlError::semantic(alloc::format!(
                "cycle '{name}' has no indices"
            )));
        }
        let step = step.rem_euclid(n);
        let mut map = BTreeMap::new();
        for (i, &idx) in cycle.indices.iter().enumerate() {
            let j = (i as i32 + step).rem_euclid(n) as usize;
            map.insert(idx, cycle.indices[j]);
        }
        Ok(map)
    }
}

/// Parse a fraction written in JSON into basis points (0..=1000) without
/// leaving a float in the generator. `0.2` becomes 200.
pub fn fraction_to_bp(value: f64) -> u16 {
    let v = if value.is_nan() { 0.0 } else { value };
    let scaled = if v > 1.0 { v } else { v * 1000.0 };
    let rounded = if scaled >= 0.0 {
        (scaled + 0.5) as u32
    } else {
        0
    };
    rounded.min(1000) as u16
}

pub fn fraction_to_q8(value: f64) -> u8 {
    let v = if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    };
    let scaled = v * 255.0;
    (scaled + 0.5) as u8
}

pub fn colors_from_hex(list: &[String]) -> PxlResult<Vec<Rgba>> {
    let mut out = Vec::with_capacity(list.len());
    for (i, h) in list.iter().enumerate() {
        out.push(parse_hex(h).map_err(|e| e.with_path(alloc::format!("/colors/{i}")))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_is_stable() {
        assert_eq!(fraction_to_bp(0.2), 200);
        assert_eq!(fraction_to_bp(0.0), 0);
        assert_eq!(fraction_to_bp(1.0), 1000);
        assert_eq!(fraction_to_q8(0.0), 0);
        assert_eq!(fraction_to_q8(1.0), 255);
    }

    #[test]
    fn cycle_rotates() {
        let mut p = Palette {
            id: "t".into(),
            colors: alloc::vec![
                Rgba::TRANSPARENT,
                Rgba::rgb(1, 1, 1),
                Rgba::rgb(2, 2, 2),
                Rgba::rgb(3, 3, 3),
            ],
            ramps: BTreeMap::new(),
            cycles: BTreeMap::new(),
            special: Special {
                transparent: 0,
                outline: 1,
                flash: 3,
            },
        };
        p.cycles.insert(
            "tread".into(),
            Cycle {
                indices: alloc::vec![1, 2, 3],
                period: 3,
            },
        );
        let m = p.cycle_map("tread", 1).unwrap();
        assert_eq!(m[&1], 2);
        assert_eq!(m[&2], 3);
        assert_eq!(m[&3], 1);
        let id = p.cycle_map("tread", 0).unwrap();
        assert_eq!(id[&1], 1);
    }
}
