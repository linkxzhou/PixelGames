//! Battle City palette. Index 0 is clear. Every other index is opaque.
//! The bytes match `data/palette.json` (the same ramp set the asset stage uses).

use serde_json::Value;

pub const COLOR_COUNT: usize = 16;

#[derive(Clone)]
pub struct Palette {
    pub rgba: [[u8; 4]; COLOR_COUNT],
}

impl Palette {
    pub fn battle_city() -> Self {
        let raw = include_str!("../data/palette.json");
        Self::from_json(raw).expect("bundled palette.json")
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let colors = value
            .get("colors")
            .and_then(|c| c.as_array())
            .ok_or_else(|| "palette.colors missing".to_string())?;
        if colors.len() != COLOR_COUNT {
            return Err(format!("expected {COLOR_COUNT} colors, got {}", colors.len()));
        }
        let mut rgba = [[0u8; 4]; COLOR_COUNT];
        for (i, color) in colors.iter().enumerate() {
            let hex = color.as_str().ok_or_else(|| format!("color {i} not a string"))?;
            rgba[i] = parse_hex(hex).ok_or_else(|| format!("bad color {hex}"))?;
        }
        if rgba[0][3] != 0 {
            return Err("index 0 must be transparent".into());
        }
        Ok(Self { rgba })
    }

    pub fn contains_rgba(&self, px: [u8; 4]) -> bool {
        if px[3] == 0 {
            return true;
        }
        self.rgba.iter().any(|c| c[0] == px[0] && c[1] == px[1] && c[2] == px[2] && c[3] == 255 && px[3] == 255)
    }
}

fn parse_hex(hex: &str) -> Option<[u8; 4]> {
    let h = hex.strip_prefix('#')?;
    match h.len() {
        8 => {
            let n = u32::from_str_radix(h, 16).ok()?;
            Some([
                ((n >> 24) & 0xff) as u8,
                ((n >> 16) & 0xff) as u8,
                ((n >> 8) & 0xff) as u8,
                (n & 0xff) as u8,
            ])
        }
        6 => {
            let n = u32::from_str_radix(h, 16).ok()?;
            Some([((n >> 16) & 0xff) as u8, ((n >> 8) & 0xff) as u8, (n & 0xff) as u8, 255])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_palette_is_opaque_except_clear() {
        let p = Palette::battle_city();
        assert_eq!(p.rgba[0][3], 0);
        assert!(p.rgba[1][3] == 255);
        assert!(p.contains_rgba([0, 0, 0, 0]));
        assert!(p.contains_rgba(p.rgba[4]));
        assert!(!p.contains_rgba([1, 2, 3, 255]));
    }
}
