//! Palette loading. Sheets are palette-indexed and every sprite is decoded
//! through this table, so a wrong palette shows up as a wrong colour rather
//! than a decoding error.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PaletteFile {
    #[serde(default)]
    pub id: String,
    pub colors: Vec<String>,
}

/// RGBA colours, indexed exactly like `palette.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub id: String,
    pub colors: Vec<[u8; 4]>,
}

impl Palette {
    pub fn from_json(text: &str) -> Result<Self, String> {
        let file: PaletteFile =
            serde_json::from_str(text).map_err(|e| format!("palette.json: {e}"))?;
        if file.colors.is_empty() {
            return Err("palette.json carries no colours".to_string());
        }
        let mut colors = Vec::with_capacity(file.colors.len());
        for (index, raw) in file.colors.iter().enumerate() {
            colors
                .push(parse_hex(raw).map_err(|e| format!("palette colour {index} '{raw}': {e}"))?);
        }
        Ok(Self {
            id: file.id,
            colors,
        })
    }

    pub fn get(&self, index: u8) -> [u8; 4] {
        self.colors
            .get(index as usize)
            .copied()
            .unwrap_or([0, 0, 0, 0])
    }

    pub fn len(&self) -> usize {
        self.colors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }
}

fn parse_hex(raw: &str) -> Result<[u8; 4], String> {
    let hex = raw.strip_prefix('#').unwrap_or(raw);
    if hex.len() != 6 && hex.len() != 8 {
        return Err("expected #RRGGBB or #RRGGBBAA".to_string());
    }
    let byte = |at: usize| -> Result<u8, String> {
        u8::from_str_radix(&hex[at..at + 2], 16).map_err(|e| format!("bad hex byte: {e}"))
    };
    let alpha = if hex.len() == 8 { byte(6)? } else { 255 };
    Ok([byte(0)?, byte(2)?, byte(4)?, alpha])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rgb_and_rgba() {
        let palette =
            Palette::from_json(r##"{"id":"p","colors":["#00000000","#0d0f0a","#6f8a3aff"]}"##)
                .unwrap();
        assert_eq!(palette.get(0), [0, 0, 0, 0]);
        assert_eq!(palette.get(1), [0x0d, 0x0f, 0x0a, 255]);
        assert_eq!(palette.get(2), [0x6f, 0x8a, 0x3a, 255]);
        assert_eq!(palette.get(9), [0, 0, 0, 0]);
    }
}
