//! Palette document + the colour helpers the renderer uses.

use std::path::Path;

#[derive(Debug, Clone)]
pub struct Palette {
    pub id: String,
    /// RGBA bytes, index 0 transparent.
    pub colors: Vec<[u8; 4]>,
    pub special: Special,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Special {
    pub transparent: u8,
    pub outline: u8,
    pub flash: u8,
}

impl Palette {
    pub fn load(path: &Path) -> Result<Palette, String> {
        let doc = crate::package::read_json(path)?;
        let colors = doc["colors"]
            .as_array()
            .ok_or_else(|| format!("{}: no colors", path.display()))?
            .iter()
            .filter_map(|value| value.as_str())
            .map(parse_hex)
            .collect::<Result<Vec<_>, _>>()?;
        if colors.is_empty() {
            return Err(format!("{}: palette is empty", path.display()));
        }
        let special = Special {
            transparent: doc["special"]["transparent"].as_u64().unwrap_or(0) as u8,
            outline: doc["special"]["outline"].as_u64().unwrap_or(1) as u8,
            flash: doc["special"]["flash"]
                .as_u64()
                .unwrap_or(colors.len().saturating_sub(1) as u64) as u8,
        };
        Ok(Palette {
            id: doc["id"].as_str().unwrap_or("palette").to_string(),
            colors,
            special,
        })
    }

    pub fn rgba(&self, index: u8) -> [u8; 4] {
        self.colors
            .get(index as usize)
            .copied()
            .unwrap_or([0, 0, 0, 0])
    }

    pub fn outline(&self) -> [u8; 4] {
        self.rgba(self.special.outline)
    }

    pub fn flash(&self) -> [u8; 4] {
        self.rgba(self.special.flash)
    }
}

pub fn parse_hex(text: &str) -> Result<[u8; 4], String> {
    let hex = text.trim_start_matches('#');
    let value = |at: usize| -> Result<u8, String> {
        u8::from_str_radix(&hex[at..at + 2], 16).map_err(|e| format!("colour '{text}': {e}"))
    };
    match hex.len() {
        6 => Ok([value(0)?, value(2)?, value(4)?, 255]),
        8 => Ok([value(0)?, value(2)?, value(4)?, value(6)?]),
        _ => Err(format!("colour '{text}' is not #rgb/#rrggbb/#rrggbbaa")),
    }
}
