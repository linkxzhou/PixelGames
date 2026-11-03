//! Non-separable HSL blend helpers (clean-room W3C non-separable blend modes).

fn lum(r: f64, g: f64, b: f64) -> f64 {
    0.3 * r + 0.59 * g + 0.11 * b
}

fn clip_color(mut r: f64, mut g: f64, mut b: f64) -> (f64, f64, f64) {
    let l = lum(r, g, b);
    let n = r.min(g).min(b);
    let x = r.max(g).max(b);
    if n < 0.0 {
        r = l + (((r - l) * l) / (l - n));
        g = l + (((g - l) * l) / (l - n));
        b = l + (((b - l) * l) / (l - n));
    }
    if x > 1.0 {
        r = l + (((r - l) * (1.0 - l)) / (x - l));
        g = l + (((g - l) * (1.0 - l)) / (x - l));
        b = l + (((b - l) * (1.0 - l)) / (x - l));
    }
    (r, g, b)
}

fn set_lum(r: f64, g: f64, b: f64, l: f64) -> (f64, f64, f64) {
    let d = l - lum(r, g, b);
    clip_color(r + d, g + d, b + d)
}

fn sat(r: f64, g: f64, b: f64) -> f64 {
    r.max(g).max(b) - r.min(g).min(b)
}

fn set_sat(r: f64, g: f64, b: f64, s: f64) -> (f64, f64, f64) {
    let mut ch = [(r, 0u8), (g, 1u8), (b, 2u8)];
    ch.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));
    let min_i = ch[0].1 as usize;
    let mid_i = ch[1].1 as usize;
    let max_i = ch[2].1 as usize;
    let mut v = [r, g, b];
    if v[max_i] > v[min_i] {
        v[mid_i] = ((v[mid_i] - v[min_i]) * s) / (v[max_i] - v[min_i]);
        v[max_i] = s;
    } else {
        v[mid_i] = 0.0;
        v[max_i] = 0.0;
    }
    v[min_i] = 0.0;
    (v[0], v[1], v[2])
}

fn to_f(c: u8) -> f64 {
    c as f64 / 255.0
}
fn to_u8(c: f64) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn blend_hue(br: u8, bg: u8, bb: u8, sr: u8, sg: u8, sb: u8) -> (u8, u8, u8) {
    let (mut r, mut g, mut b) = (to_f(sr), to_f(sg), to_f(sb));
    let (cr, cg, cb) = (to_f(br), to_f(bg), to_f(bb));
    (r, g, b) = set_sat(r, g, b, sat(cr, cg, cb));
    (r, g, b) = set_lum(r, g, b, lum(cr, cg, cb));
    (to_u8(r), to_u8(g), to_u8(b))
}

pub fn blend_saturation(br: u8, bg: u8, bb: u8, sr: u8, sg: u8, sb: u8) -> (u8, u8, u8) {
    let (cr, cg, cb) = (to_f(br), to_f(bg), to_f(bb));
    let (sr, sg, sb) = (to_f(sr), to_f(sg), to_f(sb));
    let (mut r, mut g, mut b) = set_sat(cr, cg, cb, sat(sr, sg, sb));
    (r, g, b) = set_lum(r, g, b, lum(cr, cg, cb));
    (to_u8(r), to_u8(g), to_u8(b))
}

pub fn blend_color(br: u8, bg: u8, bb: u8, sr: u8, sg: u8, sb: u8) -> (u8, u8, u8) {
    let (sr, sg, sb) = (to_f(sr), to_f(sg), to_f(sb));
    let (cr, cg, cb) = (to_f(br), to_f(bg), to_f(bb));
    let (r, g, b) = set_lum(sr, sg, sb, lum(cr, cg, cb));
    (to_u8(r), to_u8(g), to_u8(b))
}

pub fn blend_luminosity(br: u8, bg: u8, bb: u8, sr: u8, sg: u8, sb: u8) -> (u8, u8, u8) {
    let (cr, cg, cb) = (to_f(br), to_f(bg), to_f(bb));
    let (sr, sg, sb) = (to_f(sr), to_f(sg), to_f(sb));
    let (r, g, b) = set_lum(cr, cg, cb, lum(sr, sg, sb));
    (to_u8(r), to_u8(g), to_u8(b))
}
