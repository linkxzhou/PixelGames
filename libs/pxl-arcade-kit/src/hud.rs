//! Pixel HUD: text drawn from the `hud_font` sheet plus bar/icon helpers.
//!
//! The font is a 16×8 grid of 8×8 glyphs covering ASCII 32..127. Glyph pixels
//! come from step-300 like every other sprite; this module only slices cells.

use macroquad::prelude::*;

use crate::gfx::{font_sheet, Gfx, DrawOpts};
use crate::package::Package;

pub const FONT_ASSET: &str = "hud_font";
/// Horizontal advance of one glyph, in logical pixels.
pub const ADVANCE: f32 = 6.0;
pub const CELL: f32 = 8.0;

pub fn text(gfx: &Gfx, package: &Package, value: &str, x: f32, y: f32, scale: f32, color: Color) -> f32 {
    let Some(font) = font_sheet(package, FONT_ASSET) else {
        return 0.0;
    };
    let mut cursor = x;
    for ch in value.chars() {
        let code = ch as u32;
        if (32..128).contains(&code) {
            let index = code - 32;
            let col = index % font.columns;
            let row = index / font.columns;
            let source = Rect::new(
                (col * font.cell) as f32,
                (row * font.cell) as f32,
                font.cell as f32,
                font.cell as f32,
            );
            let dest = Rect::new(
                cursor,
                y,
                CELL * scale,
                CELL * scale,
            );
            gfx.blit_rect(&font.sheet, source, dest, color);
        }
        cursor += ADVANCE * scale;
    }
    cursor - x
}

pub fn text_width(value: &str, scale: f32) -> f32 {
    value.chars().count() as f32 * ADVANCE * scale
}

pub fn text_centered(
    gfx: &Gfx,
    package: &Package,
    value: &str,
    center_x: f32,
    y: f32,
    scale: f32,
    color: Color,
) -> f32 {
    let width = text_width(value, scale);
    text(gfx, package, value, center_x - width / 2.0, y, scale, color)
}

pub fn number(gfx: &Gfx, package: &Package, value: i64, x: f32, y: f32, scale: f32, color: Color, digits: usize) -> f32 {
    let text_value = format!("{:0width$}", value.max(0), width = digits);
    text(gfx, package, &text_value, x, y, scale, color)
}

/// Left-aligned label/value pair used by every score bar.
pub fn label(
    gfx: &Gfx,
    package: &Package,
    label: &str,
    value: &str,
    x: f32,
    y: f32,
    scale: f32,
    label_color: Color,
    value_color: Color,
) {
    let used = text(gfx, package, label, x, y, scale, label_color);
    text(gfx, package, value, x + used + 4.0 * scale, y, scale, value_color);
}

pub fn bar(gfx: &Gfx, package: &Package, x: f32, y: f32, w: f32, h: f32, ratio: f32, fill: Color, back: Color) {
    draw_rectangle(x, y, w, h, back);
    let clamped = ratio.clamp(0.0, 1.0);
    if clamped > 0.0 {
        draw_rectangle(x, y, w * clamped, h, fill);
    }
    let _ = (gfx, package);
}

/// Draw a life/level icon from an asset, bottom-centre anchored.
pub fn icon(gfx: &Gfx, package: &Package, asset: &str, x: f32, y: f32, scale: f32) {
    gfx.sprite(
        package,
        asset,
        "idle",
        0,
        x,
        y,
        DrawOpts::default().scaled(scale),
    );
}

pub fn dim_screen(alpha: f32) {
    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        screen_height(),
        Color::new(0.0, 0.0, 0.0, alpha),
    );
}

pub fn color(hex: u32) -> Color {
    Color::new(
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
        1.0,
    )
}
