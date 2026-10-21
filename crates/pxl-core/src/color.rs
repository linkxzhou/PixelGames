use crate::{PxlError, PxlResult};
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn is_transparent(self) -> bool {
        self.a == 0
    }

    pub fn is_opaque(self) -> bool {
        self.a == 255
    }

    pub fn to_hex(self) -> String {
        if self.a == 255 {
            alloc::format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            alloc::format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

pub fn parse_hex(input: &str) -> PxlResult<Rgba> {
    let s = input.trim();
    let hex = s.strip_prefix('#').unwrap_or(s);
    let digits = hex.as_bytes();
    let nibble = |b: u8| -> PxlResult<u8> {
        match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            b'A'..=b'F' => Ok(b - b'A' + 10),
            _ => Err(
                PxlError::schema(alloc::format!("invalid hex color '{input}'"))
                    .with_hint("use #RRGGBB or #RRGGBBAA"),
            ),
        }
    };
    let byte =
        |i: usize| -> PxlResult<u8> { Ok((nibble(digits[i])? << 4) | nibble(digits[i + 1])?) };
    match digits.len() {
        3 => Ok(Rgba::rgb(
            nibble(digits[0])? * 17,
            nibble(digits[1])? * 17,
            nibble(digits[2])? * 17,
        )),
        4 => Ok(Rgba {
            r: nibble(digits[0])? * 17,
            g: nibble(digits[1])? * 17,
            b: nibble(digits[2])? * 17,
            a: nibble(digits[3])? * 17,
        }),
        6 => Ok(Rgba::rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Ok(Rgba {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: byte(6)?,
        }),
        _ => Err(
            PxlError::schema(alloc::format!("invalid hex color '{input}'"))
                .with_hint("use #RGB, #RRGGBB, or #RRGGBBAA"),
        ),
    }
}

pub fn palette_rgb_bytes(colors: &[Rgba]) -> Vec<u8> {
    let mut out = Vec::with_capacity(colors.len() * 3);
    for c in colors {
        out.push(c.r);
        out.push(c.g);
        out.push(c.b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex() {
        assert_eq!(parse_hex("#00000000").unwrap(), Rgba::TRANSPARENT);
        assert_eq!(parse_hex("#e0533d").unwrap(), Rgba::rgb(0xe0, 0x53, 0x3d));
        assert_eq!(parse_hex("fff").unwrap(), Rgba::rgb(255, 255, 255));
    }
}
