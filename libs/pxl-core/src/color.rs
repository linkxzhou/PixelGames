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

/// OKLab in Q16 fixed point (`l` in 0..=65536, `a`/`b` roughly ±26000). Every
/// step is integer arithmetic, so the same colour converts to the same bits on
/// every platform; that keeps palette generation and colour distances
/// deterministic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Oklab {
    pub l: i32,
    pub a: i32,
    pub b: i32,
}

/// sRGB byte to linear light, Q16.
const SRGB_TO_LINEAR: [u32; 256] = [
    0, 20, 40, 60, 80, 99, 119, 139, 159, 179, 199, 219, 241, 264, 288, 313, 340, 367, 396, 427,
    458, 491, 526, 562, 599, 637, 677, 718, 761, 805, 851, 898, 947, 997, 1048, 1101, 1156, 1212,
    1270, 1330, 1391, 1453, 1517, 1583, 1651, 1720, 1790, 1863, 1937, 2013, 2090, 2170, 2250, 2333,
    2418, 2504, 2592, 2681, 2773, 2866, 2961, 3058, 3157, 3258, 3360, 3464, 3570, 3678, 3788, 3900,
    4014, 4129, 4247, 4366, 4488, 4611, 4736, 4864, 4993, 5124, 5257, 5392, 5530, 5669, 5810, 5953,
    6099, 6246, 6395, 6547, 6700, 6856, 7014, 7174, 7335, 7500, 7666, 7834, 8004, 8177, 8352, 8528,
    8708, 8889, 9072, 9258, 9445, 9635, 9828, 10022, 10219, 10417, 10619, 10822, 11028, 11235,
    11446, 11658, 11873, 12090, 12309, 12530, 12754, 12980, 13209, 13440, 13673, 13909, 14146,
    14387, 14629, 14874, 15122, 15371, 15623, 15878, 16135, 16394, 16656, 16920, 17187, 17456,
    17727, 18001, 18277, 18556, 18837, 19121, 19407, 19696, 19987, 20281, 20577, 20876, 21177,
    21481, 21787, 22096, 22407, 22721, 23038, 23357, 23678, 24002, 24329, 24658, 24990, 25325,
    25662, 26001, 26344, 26688, 27036, 27386, 27739, 28094, 28452, 28813, 29176, 29542, 29911,
    30282, 30656, 31033, 31412, 31794, 32179, 32567, 32957, 33350, 33745, 34143, 34544, 34948,
    35355, 35764, 36176, 36591, 37008, 37429, 37852, 38278, 38706, 39138, 39572, 40009, 40449,
    40891, 41337, 41785, 42236, 42690, 43147, 43606, 44069, 44534, 45002, 45473, 45947, 46423,
    46903, 47385, 47871, 48359, 48850, 49344, 49841, 50341, 50844, 51349, 51858, 52369, 52884,
    53401, 53921, 54445, 54971, 55500, 56032, 56567, 57105, 57646, 58190, 58737, 59287, 59840,
    60396, 60955, 61517, 62082, 62650, 63221, 63795, 64372, 64952, 65535,
];

fn icbrt(v: u64) -> u64 {
    // Largest r with r^3 <= v. v < 2^50 here, so r < 2^17 and r^3 fits.
    let (mut lo, mut hi) = (0u64, 1u64 << 17);
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if mid * mid * mid <= v {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

fn mix(m: [[i64; 3]; 3], v: [i64; 3]) -> [i64; 3] {
    let row = |r: [i64; 3]| (r[0] * v[0] + r[1] * v[1] + r[2] * v[2]) >> 16;
    [row(m[0]), row(m[1]), row(m[2])]
}

const M1: [[i64; 3]; 3] = [
    [27015, 35149, 3372],
    [13887, 44610, 7038],
    [5787, 18463, 41286],
];
const M2: [[i64; 3]; 3] = [
    [13792, 52011, -267],
    [129630, -159160, 29530],
    [1698, 51300, -52997],
];
const I1: [[i64; 3]; 3] = [
    [267173, -216774, 15137],
    [-83128, 171033, -22369],
    [-275, -46099, 111910],
];
const I2: [[i64; 3]; 3] = [
    [65536, 25974, 14143],
    [65536, -6918, -4185],
    [65536, -5864, -84639],
];

impl Oklab {
    pub fn from_rgba(c: Rgba) -> Self {
        let lin = [
            SRGB_TO_LINEAR[c.r as usize] as i64,
            SRGB_TO_LINEAR[c.g as usize] as i64,
            SRGB_TO_LINEAR[c.b as usize] as i64,
        ];
        let lms = mix(M1, lin);
        // cbrt(x / 2^16) * 2^16 == cbrt(x * 2^32).
        let root = |v: i64| icbrt((v.max(0) as u64) << 32) as i64;
        let lab = mix(M2, [root(lms[0]), root(lms[1]), root(lms[2])]);
        Self {
            l: lab[0] as i32,
            a: lab[1] as i32,
            b: lab[2] as i32,
        }
    }

    /// Back to an opaque sRGB colour, clamping out-of-gamut values.
    pub fn to_rgba(self) -> Rgba {
        let lms_ = mix(I2, [self.l as i64, self.a as i64, self.b as i64]);
        let cube = |v: i64| (v * v / 65536) * v / 65536;
        let lin = mix(I1, [cube(lms_[0]), cube(lms_[1]), cube(lms_[2])]);
        let to_byte = |v: i64| -> u8 {
            let v = v.clamp(0, 65535) as u32;
            // Nearest table entry: the table is increasing.
            let (mut lo, mut hi) = (0usize, 255usize);
            while lo < hi {
                let mid = (lo + hi) / 2;
                if SRGB_TO_LINEAR[mid] < v {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
            if lo > 0 && v - SRGB_TO_LINEAR[lo - 1] < SRGB_TO_LINEAR[lo] - v {
                (lo - 1) as u8
            } else {
                lo as u8
            }
        };
        Rgba::rgb(to_byte(lin[0]), to_byte(lin[1]), to_byte(lin[2]))
    }

    /// Squared Euclidean distance, in Q16 units squared.
    pub fn dist2(self, other: Oklab) -> i64 {
        let d = |a: i32, b: i32| (a as i64 - b as i64) * (a as i64 - b as i64);
        d(self.l, other.l) + d(self.a, other.a) + d(self.b, other.b)
    }
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

    #[test]
    fn oklab_matches_the_reference_and_round_trips() {
        let white = Oklab::from_rgba(Rgba::rgb(255, 255, 255));
        assert!((white.l - 65536).abs() < 80, "{white:?}");
        assert!(white.a.abs() < 80 && white.b.abs() < 80);
        let black = Oklab::from_rgba(Rgba::rgb(0, 0, 0));
        assert_eq!(black.l, 0);
        // Reference red: L 0.6280, a 0.2249, b 0.1258.
        let red = Oklab::from_rgba(Rgba::rgb(255, 0, 0));
        assert!((red.l - 41157).abs() < 200, "{red:?}");
        assert!((red.a - 14739).abs() < 200, "{red:?}");
        assert!((red.b - 8244).abs() < 200, "{red:?}");
        for c in [
            Rgba::rgb(255, 0, 0),
            Rgba::rgb(32, 32, 40),
            Rgba::rgb(0x88, 0x50, 0x30),
            Rgba::rgb(0xf0, 0xb8, 0x88),
            Rgba::rgb(12, 200, 90),
        ] {
            let back = Oklab::from_rgba(c).to_rgba();
            let close = |a: u8, b: u8| (a as i32 - b as i32).abs() <= 2;
            assert!(
                close(back.r, c.r) && close(back.g, c.g) && close(back.b, c.b),
                "{c:?} -> {back:?}"
            );
        }
    }
}
