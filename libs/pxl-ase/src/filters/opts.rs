//! Filter channel mask + boundary sampling (GE-4 implementation).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelMask {
    pub r: bool,
    pub g: bool,
    pub b: bool,
    pub a: bool,
}

impl Default for ChannelMask {
    fn default() -> Self {
        Self {
            r: true,
            g: true,
            b: true,
            a: false,
        }
    }
}

impl ChannelMask {
    pub fn rgba() -> Self {
        Self {
            r: true,
            g: true,
            b: true,
            a: true,
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.to_ascii_lowercase();
        if s.is_empty() || s == "rgb" {
            return Ok(Self::default());
        }
        if s == "rgba" || s == "all" {
            return Ok(Self::rgba());
        }
        let mut m = Self {
            r: false,
            g: false,
            b: false,
            a: false,
        };
        for ch in s.chars() {
            match ch {
                'r' => m.r = true,
                'g' => m.g = true,
                'b' => m.b = true,
                'a' => m.a = true,
                _ => return Err(format!("bad channel '{ch}' in '{s}' (use rgb|rgba|r|g|b|a…)")),
            }
        }
        Ok(m)
    }

    pub fn write_into(self, dst: &mut [u8], src: &[u8]) {
        debug_assert_eq!(dst.len(), 4);
        debug_assert_eq!(src.len(), 4);
        if self.r {
            dst[0] = src[0];
        }
        if self.g {
            dst[1] = src[1];
        }
        if self.b {
            dst[2] = src[2];
        }
        if self.a {
            dst[3] = src[3];
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BoundaryMode {
    #[default]
    Clamp,
    WrapX,
    WrapY,
    Wrap,
}

impl BoundaryMode {
    pub fn parse(s: &str) -> Result<Self, String> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "clamp" | "none" | "" => BoundaryMode::Clamp,
            "x" | "wrap-x" | "wrapx" => BoundaryMode::WrapX,
            "y" | "wrap-y" | "wrapy" => BoundaryMode::WrapY,
            "both" | "wrap" | "xy" | "tiled" => BoundaryMode::Wrap,
            other => {
                return Err(format!(
                    "unknown boundary '{other}' (clamp|x|y|both)"
                ));
            }
        })
    }

    pub fn sample(self, x: i32, y: i32, w: i32, h: i32) -> (i32, i32) {
        let sx = match self {
            BoundaryMode::Clamp | BoundaryMode::WrapY => x.clamp(0, w - 1),
            BoundaryMode::WrapX | BoundaryMode::Wrap => {
                if w <= 0 {
                    0
                } else {
                    ((x % w) + w) % w
                }
            }
        };
        let sy = match self {
            BoundaryMode::Clamp | BoundaryMode::WrapX => y.clamp(0, h - 1),
            BoundaryMode::WrapY | BoundaryMode::Wrap => {
                if h <= 0 {
                    0
                } else {
                    ((y % h) + h) % h
                }
            }
        };
        (sx, sy)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FilterOpts {
    pub channels: ChannelMask,
    pub boundary: BoundaryMode,
}
