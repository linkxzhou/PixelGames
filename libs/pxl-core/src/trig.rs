//! Integer trigonometry for whole-degree angles. A lookup table instead of
//! `f64::sin` keeps rotation bit-identical on every platform and in wasm.

/// `sin(d)` for d in 0..=90 degrees, Q16 (65536 = 1.0).
const SIN_Q16: [i32; 91] = [
    0, 1144, 2287, 3430, 4572, 5712, 6850, 7987, 9121, 10252, 11380, 12505, 13626, 14742, 15855,
    16962, 18064, 19161, 20252, 21336, 22415, 23486, 24550, 25607, 26656, 27697, 28729, 29753,
    30767, 31772, 32768, 33754, 34729, 35693, 36647, 37590, 38521, 39441, 40348, 41243, 42126,
    42995, 43852, 44695, 45525, 46341, 47143, 47930, 48703, 49461, 50203, 50931, 51643, 52339,
    53020, 53684, 54332, 54963, 55578, 56175, 56756, 57319, 57865, 58393, 58903, 59396, 59870,
    60326, 60764, 61183, 61584, 61966, 62328, 62672, 62997, 63303, 63589, 63856, 64104, 64332,
    64540, 64729, 64898, 65048, 65177, 65287, 65376, 65446, 65496, 65526, 65536,
];

/// `sin(deg)` in Q16 for any whole-degree angle.
pub fn sin_q16(deg: i32) -> i32 {
    let d = deg.rem_euclid(360);
    match d {
        0..=90 => SIN_Q16[d as usize],
        91..=180 => SIN_Q16[(180 - d) as usize],
        181..=270 => -SIN_Q16[(d - 180) as usize],
        _ => -SIN_Q16[(360 - d) as usize],
    }
}

/// `cos(deg)` in Q16.
pub fn cos_q16(deg: i32) -> i32 {
    sin_q16(deg + 90)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadrants_and_identities() {
        assert_eq!(sin_q16(0), 0);
        assert_eq!(sin_q16(90), 65536);
        assert_eq!(sin_q16(180), 0);
        assert_eq!(sin_q16(270), -65536);
        assert_eq!(cos_q16(0), 65536);
        assert_eq!(cos_q16(-90), 0);
        assert_eq!(sin_q16(30), 32768);
        for d in -400..400 {
            assert_eq!(sin_q16(d), -sin_q16(-d));
        }
    }
}
