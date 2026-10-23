use pxl_core::{IndexFrame, PxlError, PxlResult};

use super::region::full_rect;

pub(super) fn translate(
    frame: &IndexFrame,
    dx: i32,
    dy: i32,
    wrap: bool,
    region: Option<[i32; 4]>,
) -> IndexFrame {
    let rect = region.unwrap_or_else(|| full_rect(frame));
    let [rx, ry, rw, rh] = rect;
    let mut out = frame.clone();
    for ly in 0..rh {
        for lx in 0..rw {
            let x = rx + lx;
            let y = ry + ly;
            let (sx, sy) = if wrap {
                let sx = rx + (lx - dx).rem_euclid(rw);
                let sy = ry + (ly - dy).rem_euclid(rh);
                (sx, sy)
            } else {
                (x - dx, y - dy)
            };
            let c = if wrap || (sx >= rx && sy >= ry && sx < rx + rw && sy < ry + rh) {
                frame.get(sx, sy)
            } else {
                0
            };
            if frame.in_bounds(x, y) {
                out.set(x, y, c);
            }
        }
    }
    out
}

pub(super) fn scroll_rect(
    frame: &IndexFrame,
    rect: [i32; 4],
    axis: &str,
    amount: i32,
    wrap: bool,
) -> PxlResult<IndexFrame> {
    let vertical = match axis {
        "y" | "vertical" => true,
        "x" | "horizontal" => false,
        other => {
            return Err(
                PxlError::schema(format!("unknown row_shift axis '{other}'"))
                    .with_hint("axis is x or y"),
            );
        }
    };
    let dx = if vertical { 0 } else { amount };
    let dy = if vertical { amount } else { 0 };
    Ok(translate(frame, dx, dy, wrap, Some(rect)))
}

pub(super) fn shift_rows(
    frame: &IndexFrame,
    y0: i32,
    y1: i32,
    amp: i32,
    pattern: Option<&str>,
    phase: i32,
    wrap: bool,
) -> PxlResult<IndexFrame> {
    const SINE4: [i32; 4] = [0, 1, 0, -1];
    let mut out = frame.clone();
    let y0 = y0.max(0);
    let y1 = y1.min(frame.h as i32 - 1);
    for y in y0..=y1 {
        let dx = match pattern {
            None | Some("none") | Some("") => amp,
            Some("sine4") => amp * SINE4[(y + phase).rem_euclid(4) as usize],
            Some(other) => {
                return Err(
                    PxlError::schema(format!("unknown row_shift pattern '{other}'"))
                        .with_hint("pattern is sine4 or omitted"),
                );
            }
        };
        let w = frame.w as i32;
        for x in 0..w {
            let sx = if wrap { (x - dx).rem_euclid(w) } else { x - dx };
            let c = if sx >= 0 && sx < w {
                frame.get(sx, y)
            } else {
                0
            };
            out.set(x, y, c);
        }
    }
    Ok(out)
}

pub(super) fn shear(
    frame: &IndexFrame,
    axis: &str,
    k: i32,
    pivot: i32,
    rect: [i32; 4],
) -> IndexFrame {
    let [rx, ry, rw, rh] = rect;
    let mut out = frame.clone();
    for ly in 0..rh {
        for lx in 0..rw {
            let x = rx + lx;
            let y = ry + ly;
            if !frame.in_bounds(x, y) {
                continue;
            }
            out.set(x, y, 0);
        }
    }
    for ly in 0..rh {
        for lx in 0..rw {
            let x = rx + lx;
            let y = ry + ly;
            let c = frame.get(x, y);
            if c == 0 || !frame.in_bounds(x, y) {
                continue;
            }
            let (nx, ny) = match axis {
                "y" | "vertical" => {
                    let shift = (x - pivot) / k;
                    (x, y + shift)
                }
                _ => {
                    let shift = (y - pivot) / k;
                    (x + shift, y)
                }
            };
            if nx >= rx && ny >= ry && nx < rx + rw && ny < ry + rh {
                out.set(nx, ny, c);
            }
        }
    }
    out
}

pub(super) fn squash(
    frame: &IndexFrame,
    sx: i32,
    sy: i32,
    anchor: &str,
    rect: [i32; 4],
) -> PxlResult<IndexFrame> {
    let [rx, ry, rw, rh] = rect;
    let dw = (rw + sx).max(1);
    let dh = (rh + sy).max(1);
    let (hx, hy) = match anchor {
        "bottom" => ("center", "end"),
        "top" => ("center", "start"),
        "left" => ("start", "center"),
        "right" => ("end", "center"),
        "center" => ("center", "center"),
        other => {
            return Err(PxlError::schema(format!("unknown squash anchor '{other}'"))
                .with_hint("anchor is bottom, top, left, right, or center"));
        }
    };
    let ox = place(rw, dw, hx);
    let oy = place(rh, dh, hy);
    let mut out = frame.clone();
    for ly in 0..rh {
        for lx in 0..rw {
            let x = rx + lx;
            let y = ry + ly;
            if frame.in_bounds(x, y) {
                out.set(x, y, 0);
            }
        }
    }
    for dy in 0..dh {
        for dx in 0..dw {
            let sx_i = sample_index(dx, rw, dw);
            let sy_i = sample_index(dy, rh, dh);
            let c = frame.get(rx + sx_i, ry + sy_i);
            let x = rx + ox + dx;
            let y = ry + oy + dy;
            if frame.in_bounds(x, y) {
                out.set(x, y, c);
            }
        }
    }
    Ok(out)
}

fn sample_index(i: i32, src_len: i32, dst_len: i32) -> i32 {
    if dst_len <= 1 || src_len <= 1 {
        return 0;
    }
    i * (src_len - 1) / (dst_len - 1)
}

fn place(src_len: i32, dst_len: i32, how: &str) -> i32 {
    match how {
        "start" => 0,
        "end" => src_len - dst_len,
        _ => (src_len - dst_len) / 2,
    }
}

/// Left-right when `axis` is x (Piskel's VERTICAL). Up-down when `axis` is y
/// (Piskel's HORIZONTAL).
pub(super) fn flip(frame: &IndexFrame, axis: &str) -> PxlResult<IndexFrame> {
    let mut out = frame.blank_like();
    let horizontal = match axis {
        "x" | "h" | "lr" | "horizontal" => true,
        "y" | "v" | "tb" | "vertical" => false,
        other => {
            return Err(PxlError::schema(format!("unknown flip axis '{other}'"))
                .with_hint("axis x mirrors left-right, axis y mirrors up-down"));
        }
    };
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let (nx, ny) = if horizontal {
                (frame.w as i32 - 1 - x, y)
            } else {
                (x, frame.h as i32 - 1 - y)
            };
            out.set(nx, ny, frame.get(x, y));
        }
    }
    Ok(out)
}

/// Piskel `TransformUtils.rotate`: pad to a square with `ceil`, rotate, crop
/// back to the original width and height. Square sprites are a plain 90° turn.
pub(crate) fn rotate90(frame: &IndexFrame, dir: &str) -> PxlResult<IndexFrame> {
    match dir {
        "180" | "half" => {
            let once = flip(frame, "x")?;
            flip(&once, "y")
        }
        "cw" | "clockwise" | "ccw" | "counterclockwise" => {
            let cw = dir == "cw" || dir == "clockwise";
            let w = frame.w as i32;
            let h = frame.h as i32;
            let max = w.max(h);
            let x_delta = (max - w + 1) / 2;
            let y_delta = (max - h + 1) / 2;
            let mut out = frame.blank_like();
            for dy in 0..h {
                for dx in 0..w {
                    let x = dx + x_delta;
                    let y = dy + y_delta;
                    let (sx, sy) = if cw {
                        (y, max - 1 - x)
                    } else {
                        (max - 1 - y, x)
                    };
                    let sx = sx - x_delta;
                    let sy = sy - y_delta;
                    let c = if frame.in_bounds(sx, sy) {
                        frame.get(sx, sy)
                    } else {
                        0
                    };
                    out.set(dx, dy, c);
                }
            }
            Ok(out)
        }
        other => Err(PxlError::schema(format!("unknown rotate90 dir '{other}'"))
            .with_hint("dir is cw, ccw, or 180")),
    }
}
