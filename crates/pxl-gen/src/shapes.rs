//! Shape programs for objects that need a readable silhouette (tanks, props).

use pxl_core::{
    paint_outline, Brush, IndexFrame, Mirror, OutlineMode, Palette, PxlError, PxlResult,
};
use pxl_format::{DrawCmd, ShapesFile};

pub fn generate_shapes(spec: &ShapesFile, palette: &Palette) -> PxlResult<IndexFrame> {
    let mut frame = IndexFrame::new(spec.w, spec.h)?;
    let global = Mirror::parse(spec.mirror.as_deref().unwrap_or("none"))?;
    for cmd in &spec.draw {
        if let DrawCmd::Outline { outline } = cmd {
            let mode = OutlineMode::parse(&outline.mode)?;
            let brush = Brush::parse(&outline.brush)?;
            ensure_color(palette, outline.c)?;
            paint_outline(&mut frame, outline.c, mode, brush, outline.width as i32)?;
            continue;
        }
        let mut stamp = IndexFrame::new(spec.w, spec.h)?;
        let local_mirror = draw_stamp(&mut stamp, cmd)?;
        let axis = mirror_axis(global, local_mirror);
        blit(&mut frame, &stamp);
        match axis {
            Mirror::None => {}
            Mirror::X => blit(&mut frame, &flip_x(&stamp)),
            Mirror::Y => blit(&mut frame, &flip_y(&stamp)),
            Mirror::Xy => {
                let fx = flip_x(&stamp);
                blit(&mut frame, &fx);
                blit(&mut frame, &flip_y(&stamp));
                blit(&mut frame, &flip_y(&fx));
            }
        }
    }
    for (i, px) in frame.pixels.iter().enumerate() {
        if *px as usize >= palette.len() {
            return Err(PxlError::semantic(format!(
                "shape color {px} is outside the palette (pixel {i})"
            )));
        }
    }
    Ok(frame)
}

fn mirror_axis(global: Mirror, local: Option<bool>) -> Mirror {
    if local == Some(false) {
        return Mirror::None;
    }
    match global {
        Mirror::None if local == Some(true) => Mirror::X,
        other => other,
    }
}

fn ensure_color(palette: &Palette, c: u8) -> PxlResult<()> {
    palette.get(c).map(|_| ())
}

fn draw_stamp(stamp: &mut IndexFrame, cmd: &DrawCmd) -> PxlResult<Option<bool>> {
    match cmd {
        DrawCmd::Rect {
            rect,
            c,
            pattern,
            mirror,
        } => {
            let [x, y, w, h] = *rect;
            if w <= 0 || h <= 0 {
                return Err(PxlError::schema("rect width and height must be positive"));
            }
            for py in y..y + h {
                for px in x..x + w {
                    if pattern_on(pattern, px, py)? {
                        stamp.set(px, py, *c);
                    }
                }
            }
            Ok(*mirror)
        }
        DrawCmd::Circle { circle, c, mirror } => {
            let [cx, cy, r] = *circle;
            if r < 0 {
                return Err(PxlError::schema("circle radius must be >= 0"));
            }
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy <= r * r {
                        stamp.set(cx + dx, cy + dy, *c);
                    }
                }
            }
            Ok(*mirror)
        }
        DrawCmd::Line { line, c, mirror } => {
            let [x0, y0, x1, y1] = *line;
            bresenham(stamp, x0, y0, x1, y1, *c);
            Ok(*mirror)
        }
        DrawCmd::Pixels {
            pixels,
            x,
            y,
            mirror,
        } => {
            for (row_i, row) in pixels.iter().enumerate() {
                for (col_i, ch) in row.chars().enumerate() {
                    let idx = pxl_core::decode_index(ch)?;
                    if idx != 0 {
                        stamp.set(*x + col_i as i32, *y + row_i as i32, idx);
                    }
                }
            }
            Ok(*mirror)
        }
        DrawCmd::Outline { .. } => Ok(Some(false)),
    }
}

fn pattern_on(pattern: &Option<String>, x: i32, y: i32) -> PxlResult<bool> {
    let Some(p) = pattern else {
        return Ok(true);
    };
    if p == "checker" {
        return Ok((x + y).rem_euclid(2) == 0);
    }
    if let Some(rest) = p.strip_prefix("stripe_y:") {
        let n = parse_period(rest)?;
        return Ok(y.rem_euclid(n) < n / 2);
    }
    if let Some(rest) = p.strip_prefix("stripe_x:") {
        let n = parse_period(rest)?;
        return Ok(x.rem_euclid(n) < n / 2);
    }
    Err(PxlError::schema(format!("unknown shape pattern '{p}'"))
        .with_hint("patterns: stripe_y:N, stripe_x:N, checker"))
}

fn parse_period(rest: &str) -> PxlResult<i32> {
    let n: i32 = rest.parse().map_err(|_| {
        PxlError::schema(format!("bad stripe period '{rest}'"))
            .with_hint("use a positive integer, e.g. stripe_y:4")
    })?;
    if n <= 1 {
        return Err(PxlError::schema("stripe period must be >= 2"));
    }
    Ok(n)
}

fn bresenham(stamp: &mut IndexFrame, mut x0: i32, mut y0: i32, x1: i32, y1: i32, c: u8) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        stamp.set(x0, y0, c);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = err * 2;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn blit(dst: &mut IndexFrame, src: &IndexFrame) {
    for (i, px) in src.pixels.iter().enumerate() {
        if *px != 0 {
            dst.pixels[i] = *px;
        }
    }
}

fn flip_x(src: &IndexFrame) -> IndexFrame {
    let mut out = src.blank_like();
    for y in 0..src.h as i32 {
        for x in 0..src.w as i32 {
            out.set(src.w as i32 - 1 - x, y, src.get(x, y));
        }
    }
    out
}

fn flip_y(src: &IndexFrame) -> IndexFrame {
    let mut out = src.blank_like();
    for y in 0..src.h as i32 {
        for x in 0..src.w as i32 {
            out.set(x, src.h as i32 - 1 - y, src.get(x, y));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_format::parse_shapes;

    #[test]
    fn mirrored_rect_is_symmetric() {
        let spec = parse_shapes(r#"{"w":8,"h":4,"mirror":"x","draw":[{"rect":[1,1,2,2],"c":3}]}"#)
            .unwrap();
        let pal = crate::builtin_palette("tank").unwrap();
        let frame = generate_shapes(&spec, &pal).unwrap();
        assert_eq!(frame.get(1, 1), 3);
        assert_eq!(frame.get(5, 1), 3);
        assert_eq!(frame.get(0, 0), 0);
    }

    #[test]
    fn circle_mirror_false_stays_put() {
        let spec = parse_shapes(
            r#"{"w":8,"h":8,"mirror":"x","draw":[{"circle":[2,3,1],"c":4,"mirror":false}]}"#,
        )
        .unwrap();
        let pal = crate::builtin_palette("tank").unwrap();
        let frame = generate_shapes(&spec, &pal).unwrap();
        assert_eq!(frame.get(2, 3), 4);
        assert!(frame.get(5, 3) == 0);
    }

    #[test]
    fn local_mirror_true_defaults_to_x_when_global_is_none() {
        let spec = parse_shapes(
            r#"{"w":8,"h":4,"mirror":"none","draw":[{"rect":[1,1,2,2],"c":3,"mirror":true}]}"#,
        )
        .unwrap();
        let pal = crate::builtin_palette("tank").unwrap();
        let frame = generate_shapes(&spec, &pal).unwrap();
        assert_eq!(frame.get(1, 1), 3);
        assert_eq!(frame.get(5, 1), 3);
    }
}
