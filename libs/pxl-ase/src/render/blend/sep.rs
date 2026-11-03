//! Separable blend channel ops (clean-room; W3C compositing-1 / PDF-style).

/// MUL_UN8-style multiply.
pub fn mul_u8(b: u8, s: u8) -> u8 {
    let t = b as u32 * s as u32;
    ((t + (t >> 8)) >> 8) as u8
}

pub fn div_u8(b: u8, s: u8) -> u8 {
    if s == 0 {
        return 255;
    }
    ((b as u32 * 255) / s as u32).min(255) as u8
}

pub fn screen_u8(b: u8, s: u8) -> u8 {
    let m = mul_u8(b, s);
    (b as u16 + s as u16 - m as u16).min(255) as u8
}

pub fn hard_light_u8(b: u8, s: u8) -> u8 {
    if s < 128 {
        mul_u8(b, s.saturating_mul(2))
    } else {
        screen_u8(b, ((s as u16) * 2 - 255) as u8)
    }
}

pub fn overlay_u8(b: u8, s: u8) -> u8 {
    hard_light_u8(s, b)
}

pub fn soft_light_u8(b: u8, s: u8) -> u8 {
    let bf = b as f64 / 255.0;
    let sf = s as f64 / 255.0;
    let d = if bf <= 0.25 {
        ((16.0 * bf - 12.0) * bf + 4.0) * bf
    } else {
        bf.sqrt()
    };
    let r = if sf <= 0.5 {
        bf - (1.0 - 2.0 * sf) * bf * (1.0 - bf)
    } else {
        bf + (2.0 * sf - 1.0) * (d - bf)
    };
    (r.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn color_dodge_u8(b: u8, s: u8) -> u8 {
    if b == 0 {
        return 0;
    }
    let inv = 255u32.saturating_sub(s as u32);
    if b as u32 >= inv {
        255
    } else if inv == 0 {
        255
    } else {
        ((b as u32 * 255) / inv).min(255) as u8
    }
}

pub fn color_burn_u8(b: u8, s: u8) -> u8 {
    if b == 255 {
        return 255;
    }
    let nb = 255u32.saturating_sub(b as u32);
    if nb >= s as u32 {
        0
    } else if s == 0 {
        0
    } else {
        255 - ((nb * 255) / s as u32).min(255) as u8
    }
}

pub fn divide_u8(b: u8, s: u8) -> u8 {
    if b == 0 {
        0
    } else if b >= s {
        255
    } else if s == 0 {
        255
    } else {
        div_u8(b, s)
    }
}

pub fn exclusion_u8(b: u8, s: u8) -> u8 {
    let t = mul_u8(b, s);
    (b as i16 + s as i16 - 2 * t as i16).clamp(0, 255) as u8
}
