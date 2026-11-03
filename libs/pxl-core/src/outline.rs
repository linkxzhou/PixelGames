//! Integer outline rasterizer.
//!
//! The three brushes (square / diamond / circle) follow the idea of
//! Pixelorama's MIT `OutlineInline` shader: outer paints transparent texels
//! next to opaque ones, inner paints opaque texels next to transparency.
//! This is an independent CPU implementation, not a shader translation.

use crate::{IndexFrame, PxlError, PxlResult};
use alloc::format;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutlineMode {
    Outer,
    Inner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Brush {
    Square,
    Diamond,
    Circle,
}

impl OutlineMode {
    pub fn parse(s: &str) -> PxlResult<Self> {
        match s {
            "outer" => Ok(OutlineMode::Outer),
            "inner" => Ok(OutlineMode::Inner),
            other => Err(PxlError::schema(format!("unknown outline mode '{other}'"))
                .with_hint("mode is outer or inner")),
        }
    }
}

impl Brush {
    pub fn parse(s: &str) -> PxlResult<Self> {
        match s {
            "square" | "box" => Ok(Brush::Square),
            "diamond" | "plus" => Ok(Brush::Diamond),
            "circle" | "round" => Ok(Brush::Circle),
            other => Err(PxlError::schema(format!("unknown outline brush '{other}'"))
                .with_hint("brush is square, diamond, or circle")),
        }
    }
}

pub fn paint_outline(
    frame: &mut IndexFrame,
    color: u8,
    mode: OutlineMode,
    brush: Brush,
    width: i32,
) -> PxlResult<()> {
    if width < 1 || width > 4 {
        return Err(
            PxlError::schema(format!("outline width {width} is outside 1..=4"))
                .with_hint("keep outline width small on a 16–32px sprite"),
        );
    }
    let src = frame.clone();
    let w = frame.w as i32;
    let h = frame.h as i32;
    for y in 0..h {
        for x in 0..w {
            let opaque = src.opaque_at(x, y);
            let hit = match mode {
                OutlineMode::Outer => !opaque && touches(&src, x, y, brush, width, true),
                OutlineMode::Inner => opaque && touches(&src, x, y, brush, width, false),
            };
            if hit {
                frame.set(x, y, color);
            }
        }
    }
    Ok(())
}

fn touches(src: &IndexFrame, x: i32, y: i32, brush: Brush, width: i32, want_opaque: bool) -> bool {
    for dy in -width..=width {
        for dx in -width..=width {
            if dx == 0 && dy == 0 {
                continue;
            }
            if !in_brush(dx, dy, brush, width) {
                continue;
            }
            let opaque = src.opaque_at(x + dx, y + dy);
            if opaque == want_opaque {
                return true;
            }
        }
    }
    false
}

fn in_brush(dx: i32, dy: i32, brush: Brush, width: i32) -> bool {
    match brush {
        Brush::Square => dx.abs().max(dy.abs()) <= width,
        Brush::Diamond => dx.abs() + dy.abs() <= width,
        Brush::Circle => dx * dx + dy * dy <= width * width,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_square_rings_a_pixel() {
        let mut f = IndexFrame::new(3, 3).unwrap();
        f.set(1, 1, 2);
        paint_outline(&mut f, 1, OutlineMode::Outer, Brush::Square, 1).unwrap();
        assert_eq!(f.get(1, 1), 2);
        assert_eq!(f.get(0, 0), 1);
        assert_eq!(f.get(1, 0), 1);
        assert_eq!(f.get(2, 2), 1);
    }

    #[test]
    fn outer_diamond_skips_corners() {
        let mut f = IndexFrame::new(3, 3).unwrap();
        f.set(1, 1, 2);
        paint_outline(&mut f, 1, OutlineMode::Outer, Brush::Diamond, 1).unwrap();
        assert_eq!(f.get(0, 1), 1);
        assert_eq!(f.get(0, 0), 0);
    }
}
