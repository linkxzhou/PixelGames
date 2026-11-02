//! Shape programs for objects that need a readable silhouette (tanks, props).

use pxl_core::{
    paint_outline, Brush, IndexFrame, Mirror, OutlineMode, Palette, PxlError, PxlResult,
};
use pxl_format::{DrawCmd, ShapesFile};

pub fn generate_shapes(spec: &ShapesFile, palette: &Palette) -> PxlResult<IndexFrame> {
    let mut frame = IndexFrame::new(spec.w, spec.h)?;
    let global = Mirror::parse(spec.mirror.as_deref().unwrap_or("none"))?;
    for (n, cmd) in spec.draw.iter().enumerate() {
        // Every command's colour is checked where it is written, so a command
        // that is clipped away or painted over cannot hide an invalid index.
        if let Some(c) = cmd_color(cmd) {
            if c as usize >= palette.len() {
                return Err(PxlError::semantic(format!(
                    "draw[{n}]: color {c} is outside the palette ({} colors)",
                    palette.len()
                ))
                .with_path(format!("/draw/{n}/c"))
                .with_hint("use an index that exists in the palette"));
            }
        }
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

fn cmd_color(cmd: &DrawCmd) -> Option<u8> {
    match cmd {
        DrawCmd::Rect { c, .. }
        | DrawCmd::Circle { c, .. }
        | DrawCmd::Ellipse { c, .. }
        | DrawCmd::Polygon { c, .. }
        | DrawCmd::ThickLine { c, .. }
        | DrawCmd::Line { c, .. } => Some(*c),
        DrawCmd::Pixels { .. } | DrawCmd::Outline { .. } => None,
    }
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
            // i64 end points and a canvas intersection: the loop never visits a
            // cell outside the canvas, whatever the declared size.
            let (cw, ch) = (stamp.w as i64, stamp.h as i64);
            let x0 = (x as i64).max(0);
            let y0 = (y as i64).max(0);
            let x1 = (x as i64 + w as i64).min(cw);
            let y1 = (y as i64 + h as i64).min(ch);
            for py in y0..y1 {
                for px in x0..x1 {
                    if pattern_on(pattern, px as i32, py as i32)? {
                        stamp.set(px as i32, py as i32, *c);
                    }
                }
            }
            if pattern.is_some() {
                pattern_on(pattern, 0, 0)?;
            }
            Ok(*mirror)
        }
        DrawCmd::Circle { circle, c, mirror } => {
            let [cx, cy, r] = *circle;
            if r < 0 {
                return Err(PxlError::schema("circle radius must be >= 0"));
            }
            let (cw, ch) = (stamp.w as i64, stamp.h as i64);
            let (cx, cy, r) = (cx as i64, cy as i64, r as i64);
            let r2 = r * r;
            for py in (cy - r).max(0)..(cy + r + 1).min(ch) {
                for px in (cx - r).max(0)..(cx + r + 1).min(cw) {
                    let (dx, dy) = (px - cx, py - cy);
                    if dx * dx + dy * dy <= r2 {
                        stamp.set(px as i32, py as i32, *c);
                    }
                }
            }
            Ok(*mirror)
        }
        DrawCmd::Ellipse {
            ellipse,
            c,
            fill,
            mirror,
        } => {
            draw_ellipse(stamp, *ellipse, *c, *fill)?;
            Ok(*mirror)
        }
        DrawCmd::Polygon { polygon, c, mirror } => {
            draw_polygon(stamp, polygon, *c)?;
            Ok(*mirror)
        }
        DrawCmd::ThickLine {
            line,
            width,
            c,
            mirror,
        } => {
            draw_thick_line(stamp, *line, *width, *c)?;
            Ok(*mirror)
        }
        DrawCmd::Line { line, c, mirror } => {
            let [x0, y0, x1, y1] = *line;
            bresenham(stamp, x0, y0, x1, y1, *c)?;
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
                        let px = *x as i64 + col_i as i64;
                        let py = *y as i64 + row_i as i64;
                        if let (Ok(px), Ok(py)) = (i32::try_from(px), i32::try_from(py)) {
                            stamp.set(px, py, idx);
                        }
                    }
                }
            }
            Ok(*mirror)
        }
        DrawCmd::Outline { .. } => Ok(Some(false)),
    }
}

/// Ellipse inscribed in the box `[x, y, w, h]`. Pixel centres are tested in
/// doubled coordinates so the maths stays in integers: with the box centre at
/// `(2x + w, 2y + h)` and semi-axes `w`, `h` (all doubled), pixel `(px, py)`
/// centre `(2px + 1, 2py + 1)` is inside when
/// `((2px+1-cx)/w)^2 + ((2py+1-cy)/h)^2 <= 1`, evaluated as
/// `dx^2 * h^2 + dy^2 * w^2 <= w^2 * h^2`.
fn draw_ellipse(stamp: &mut IndexFrame, e: [i32; 4], c: u8, fill: bool) -> PxlResult<()> {
    let [x, y, w, h] = e;
    if w <= 0 || h <= 0 {
        return Err(PxlError::schema(
            "ellipse width and height must be positive",
        ));
    }
    let (x, y, w, h) = (x as i64, y as i64, w as i64, h as i64);
    let inside = |px: i64, py: i64| -> bool {
        let dx = 2 * px + 1 - (2 * x + w);
        let dy = 2 * py + 1 - (2 * y + h);
        dx * dx * h * h + dy * dy * w * w <= w * w * h * h
    };
    let (cw, ch) = (stamp.w as i64, stamp.h as i64);
    for py in y.max(0)..(y + h).min(ch) {
        for px in x.max(0)..(x + w).min(cw) {
            if !inside(px, py) {
                continue;
            }
            // A ring keeps only pixels with a 4-neighbour outside the ellipse.
            let edge = !inside(px - 1, py)
                || !inside(px + 1, py)
                || !inside(px, py - 1)
                || !inside(px, py + 1);
            if fill || edge {
                stamp.set(px as i32, py as i32, c);
            }
        }
    }
    Ok(())
}

/// Even-odd polygon fill. A pixel is inside when a ray from its centre toward +x
/// crosses an odd number of edges. Crossings use the half-open rule
/// `min(y0,y1) <= cy < max(y0,y1)`, so a vertex shared by two edges is counted
/// once, and the x position is compared exactly with integer cross products.
fn draw_polygon(stamp: &mut IndexFrame, pts: &[[i32; 2]], c: u8) -> PxlResult<()> {
    if pts.len() < 3 {
        return Err(PxlError::schema("polygon needs at least 3 points"));
    }
    if pts.len() > 256 {
        return Err(PxlError::schema("polygon has more than 256 points"));
    }
    let p: Vec<(i64, i64)> = pts
        .iter()
        .map(|[x, y]| (2 * *x as i64, 2 * *y as i64))
        .collect();
    let (min_y, max_y) = p.iter().fold((i64::MAX, i64::MIN), |(lo, hi), &(_, y)| {
        (lo.min(y), hi.max(y))
    });
    let (min_x, max_x) = p.iter().fold((i64::MAX, i64::MIN), |(lo, hi), &(x, _)| {
        (lo.min(x), hi.max(x))
    });
    let (cw, ch) = (stamp.w as i64, stamp.h as i64);
    // Pixel (px, py) covers [px, px+1) so its centre is (2px+1, 2py+1) in the
    // doubled space; clip the scan to the polygon bounds and the canvas.
    let y0 = (min_y.div_euclid(2) - 1).max(0);
    let y1 = (max_y.div_euclid(2) + 1).min(ch - 1);
    let x0 = (min_x.div_euclid(2) - 1).max(0);
    let x1 = (max_x.div_euclid(2) + 1).min(cw - 1);
    for py in y0..=y1 {
        let cy = 2 * py + 1;
        for px in x0..=x1 {
            let cx = 2 * px + 1;
            let mut inside = false;
            for i in 0..p.len() {
                let (ax, ay) = p[i];
                let (bx, by) = p[(i + 1) % p.len()];
                if (ay <= cy) == (by <= cy) {
                    continue;
                }
                // Is the crossing to the right of the pixel centre?
                //   x_cross = ax + (cy - ay) * (bx - ax) / (by - ay)  >=  cx
                // Compared as a cross product in i128: coordinates reach 2^32,
                // so a product of two of them overflows i64.
                let num = (cy - ay) as i128 * (bx - ax) as i128;
                let den = (by - ay) as i128;
                let lhs = (cx - ax) as i128 * den;
                let right = if den > 0 { num >= lhs } else { num <= lhs };
                if right {
                    inside = !inside;
                }
            }
            if inside {
                stamp.set(px as i32, py as i32, c);
            }
        }
    }
    Ok(())
}

/// A `width`-pixel line: the single-pixel Bresenham path with a square brush of
/// `width` pixels centred on each step. Width 1 equals a plain line.
fn draw_thick_line(stamp: &mut IndexFrame, l: [i32; 4], width: u8, c: u8) -> PxlResult<()> {
    if width == 0 || width > 16 {
        return Err(PxlError::schema("line width must be in 1..=16"));
    }
    let mut path = IndexFrame::canvas(stamp.w, stamp.h)?;
    let [x0, y0, x1, y1] = l;
    bresenham(&mut path, x0, y0, x1, y1, 1)?;
    let lo = -((width as i32 - 1) / 2);
    let hi = width as i32 / 2;
    for y in 0..stamp.h as i32 {
        for x in 0..stamp.w as i32 {
            if path.get(x, y) == 0 {
                continue;
            }
            for dy in lo..=hi {
                for dx in lo..=hi {
                    stamp.set(x + dx, y + dy, c);
                }
            }
        }
    }
    // A brush centred off-canvas can still reach in; replay the ends unclipped.
    for (ex, ey) in [(x0, y0), (x1, y1)] {
        for dy in lo..=hi {
            for dx in lo..=hi {
                if let (Some(px), Some(py)) = (ex.checked_add(dx), ey.checked_add(dy)) {
                    stamp.set(px, py, c);
                }
            }
        }
    }
    Ok(())
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

/// Longest line, in steps, a shape program may declare. Canvases are at most
/// 128 px, so anything far beyond that is a mistake, not a drawing.
const MAX_LINE_STEPS: i64 = 4096;

fn bresenham(stamp: &mut IndexFrame, x0: i32, y0: i32, x1: i32, y1: i32, c: u8) -> PxlResult<()> {
    let (mut x0, mut y0, x1, y1) = (x0 as i64, y0 as i64, x1 as i64, y1 as i64);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    if dx.max(-dy) > MAX_LINE_STEPS {
        return Err(PxlError::schema(format!(
            "line is {} px long, over the {MAX_LINE_STEPS} px limit",
            dx.max(-dy)
        ))
        .with_hint("lines are drawn on a canvas of at most 128x128"));
    }
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if let (Ok(px), Ok(py)) = (i32::try_from(x0), i32::try_from(y0)) {
            stamp.set(px, py, c);
        }
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
    Ok(())
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

    fn run(draw: &str) -> PxlResult<IndexFrame> {
        let spec = parse_shapes(&format!(r#"{{"w":8,"h":8,"draw":[{draw}]}}"#)).unwrap();
        let pal = crate::builtin_palette("tank").unwrap();
        generate_shapes(&spec, &pal)
    }

    #[test]
    fn extreme_rect_coordinates_neither_panic_nor_wrap() {
        // x + w overflows i32; the rect is entirely off the canvas.
        let frame = run(r#"{"rect":[2147483647,0,1,1],"c":2}"#).unwrap();
        assert_eq!(frame.count_opaque(), 0);
        // A huge rect that does cover the canvas is clipped, not iterated.
        let frame =
            run(r#"{"rect":[-2147483648,-2147483648,2147483647,2147483647],"c":2}"#).unwrap();
        assert_eq!(frame.count_opaque(), 0, "the far corner stays off-canvas");
        let frame =
            run(r#"{"rect":[-1000000000,-1000000000,1000000008,1000000008],"c":2}"#).unwrap();
        assert_eq!(frame.count_opaque(), 64);
    }

    #[test]
    fn huge_circle_is_clipped_and_cheap() {
        let started = std::time::Instant::now();
        let frame = run(r#"{"circle":[4,4,1000000000],"c":2}"#).unwrap();
        assert_eq!(frame.count_opaque(), 64);
        assert!(
            started.elapsed().as_millis() < 500,
            "circle was not clipped"
        );
        let frame = run(r#"{"circle":[2147483647,2147483647,2],"c":2}"#).unwrap();
        assert_eq!(frame.count_opaque(), 0);
    }

    #[test]
    fn extreme_lines_are_rejected_or_clipped_without_overflow() {
        assert!(run(r#"{"line":[-2147483648,0,2147483647,0],"c":2}"#).is_err());
        let frame = run(r#"{"line":[-3,4,20,4],"c":2}"#).unwrap();
        assert_eq!(frame.count_opaque(), 8);
    }

    #[test]
    fn an_invalid_colour_fails_even_when_fully_clipped_or_overdrawn() {
        let bad = 200;
        assert!(run(&format!(r#"{{"rect":[100,100,1,1],"c":{bad}}}"#)).is_err());
        assert!(run(&format!(
            r#"{{"rect":[0,0,8,8],"c":{bad}}},{{"rect":[0,0,8,8],"c":2}}"#
        ))
        .is_err());
        assert!(run(&format!(r#"{{"circle":[0,0,1],"c":{bad}}}"#)).is_err());
        assert!(run(&format!(r#"{{"line":[0,0,3,3],"c":{bad}}}"#)).is_err());
    }

    #[test]
    fn pixels_with_extreme_origin_do_not_overflow() {
        let frame = run(r#"{"pixels":["22"],"x":2147483647,"y":0}"#).unwrap();
        assert_eq!(frame.count_opaque(), 0);
    }

    fn grid(frame: &IndexFrame) -> Vec<String> {
        (0..frame.h as i32)
            .map(|y| {
                (0..frame.w as i32)
                    .map(|x| if frame.get(x, y) == 0 { '.' } else { '#' })
                    .collect()
            })
            .collect()
    }

    fn run_n(w: u16, h: u16, draw: &str) -> PxlResult<IndexFrame> {
        let spec = parse_shapes(&format!(r#"{{"w":{w},"h":{h},"draw":[{draw}]}}"#)).unwrap();
        let pal = crate::builtin_palette("tank").unwrap();
        generate_shapes(&spec, &pal)
    }

    #[test]
    fn a_plain_line_still_parses_as_a_line_and_width_makes_it_thick() {
        let thin = run_n(8, 8, r#"{"line":[0,2,7,2],"c":2}"#).unwrap();
        assert_eq!(thin.count_opaque(), 8);
        let one = run_n(8, 8, r#"{"line":[0,2,7,2],"width":1,"c":2}"#).unwrap();
        assert_eq!(thin.pixels, one.pixels, "width 1 equals a plain line");
        let thick = run_n(8, 8, r#"{"line":[0,3,7,3],"width":3,"c":2}"#).unwrap();
        assert_eq!(thick.count_opaque(), 24);
        assert_eq!(thick.get(4, 2), 2);
        assert_eq!(thick.get(4, 4), 2);
        assert_eq!(thick.get(4, 5), 0);
        assert!(run_n(8, 8, r#"{"line":[0,0,7,7],"width":0,"c":2}"#).is_err());
        assert!(run_n(8, 8, r#"{"line":[0,0,7,7],"width":99,"c":2}"#).is_err());
    }

    #[test]
    fn ellipse_fills_a_symmetric_round_shape_inside_its_box() {
        let f = run_n(10, 6, r#"{"ellipse":[0,0,10,6],"c":2}"#).unwrap();
        let g = grid(&f);
        for row in &g {
            let r: String = row.chars().rev().collect();
            assert_eq!(*row, r, "mirror symmetric in x: {g:?}");
        }
        assert_eq!(g[0], g[5], "mirror symmetric in y");
        assert_eq!(g[2].matches('#').count(), 10, "widest rows span the box");
        assert!(g[0].matches('#').count() < g[2].matches('#').count());
        assert_eq!(f.get(0, 0), 0, "corner of the box is outside the ellipse");
        let ring = run_n(10, 6, r#"{"ellipse":[0,0,10,6],"c":2,"fill":false}"#).unwrap();
        assert!(ring.count_opaque() < f.count_opaque());
        assert_eq!(ring.get(5, 3), 0, "ring is hollow");
        assert!(run_n(8, 8, r#"{"ellipse":[0,0,0,4],"c":2}"#).is_err());
    }

    #[test]
    fn a_square_ellipse_matches_a_filled_disc_footprint() {
        let f = run_n(8, 8, r#"{"ellipse":[0,0,8,8],"c":2}"#).unwrap();
        assert_eq!(f.count_opaque(), 52);
    }

    #[test]
    fn polygon_fills_a_triangle_with_even_odd_edges() {
        let f = run_n(8, 8, r#"{"polygon":[[0,0],[8,0],[0,8]],"c":2}"#).unwrap();
        let g = grid(&f);
        assert_eq!(g[0], "########");
        assert_eq!(g[7], "#.......");
        let n = f.count_opaque();
        assert!((28..=36).contains(&n), "triangle area ~32, got {n}");
        // The two halves of the diagonal split are complementary, with no
        // pixel dropped or doubled where the edge passes through a centre.
        let other = run_n(8, 8, r#"{"polygon":[[8,0],[8,8],[0,8]],"c":2}"#).unwrap();
        assert_eq!(n + other.count_opaque(), 64);
        assert!(run_n(8, 8, r#"{"polygon":[[0,0],[1,1]],"c":2}"#).is_err());
    }

    #[test]
    fn polygon_is_even_odd_and_handles_extreme_points() {
        let square = run_n(8, 8, r#"{"polygon":[[1,1],[7,1],[7,7],[1,7]],"c":2}"#).unwrap();
        assert_eq!(square.count_opaque(), 36);
        let frame = run_n(
            8,
            8,
            r#"{"polygon":[[-2147483648,-2147483648],[2147483647,0],[0,2147483647]],"c":2}"#,
        )
        .unwrap();
        assert!(frame.count_opaque() <= 64);
        let too_many = format!(r#"{{"polygon":[{}],"c":2}}"#, vec!["[0,0]"; 300].join(","));
        assert!(run_n(8, 8, &too_many).is_err());
    }

    #[test]
    fn new_primitives_validate_colour_and_mirror() {
        let bad = 200;
        assert!(run_n(8, 8, &format!(r#"{{"ellipse":[0,0,4,4],"c":{bad}}}"#)).is_err());
        assert!(run_n(
            8,
            8,
            &format!(r#"{{"polygon":[[0,0],[4,0],[0,4]],"c":{bad}}}"#)
        )
        .is_err());
        let spec =
            parse_shapes(r#"{"w":8,"h":4,"mirror":"x","draw":[{"ellipse":[0,0,3,3],"c":3}]}"#)
                .unwrap();
        let pal = crate::builtin_palette("tank").unwrap();
        let f = generate_shapes(&spec, &pal).unwrap();
        assert_eq!(f.get(1, 1), 3);
        assert_eq!(f.get(6, 1), 3, "mirrored copy");
    }
}
