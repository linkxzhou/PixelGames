//! A 3x5 bitmap font and a contact sheet, so a comparison image can name each
//! panel and print its numbers without any font file or dependency.

use pxl_core::{PxlError, PxlResult};

use crate::raster::scaled_dims;

const GLYPH_W: u32 = 3;
const GLYPH_H: u32 = 5;
const BACKDROP: [u8; 4] = [34, 34, 40, 255];
const TITLE: [u8; 4] = [255, 255, 255, 255];
const BODY: [u8; 4] = [190, 190, 200, 255];
const MARGIN: u32 = 8;

/// Rows of a glyph, top to bottom; bit 2 is the left column.
fn glyph(c: char) -> [u8; 5] {
    match c.to_ascii_lowercase() {
        '0' => [7, 5, 5, 5, 7],
        '1' => [2, 6, 2, 2, 7],
        '2' => [7, 1, 7, 4, 7],
        '3' => [7, 1, 7, 1, 7],
        '4' => [5, 5, 7, 1, 1],
        '5' => [7, 4, 7, 1, 7],
        '6' => [7, 4, 7, 5, 7],
        '7' => [7, 1, 1, 2, 2],
        '8' => [7, 5, 7, 5, 7],
        '9' => [7, 5, 7, 1, 7],
        'a' => [2, 5, 7, 5, 5],
        'b' => [6, 5, 6, 5, 6],
        'c' => [3, 4, 4, 4, 3],
        'd' => [6, 5, 5, 5, 6],
        'e' => [7, 4, 6, 4, 7],
        'f' => [7, 4, 6, 4, 4],
        'g' => [3, 4, 5, 5, 3],
        'h' => [5, 5, 7, 5, 5],
        'i' => [7, 2, 2, 2, 7],
        'j' => [1, 1, 1, 5, 2],
        'k' => [5, 5, 6, 5, 5],
        'l' => [4, 4, 4, 4, 7],
        'm' => [5, 7, 7, 5, 5],
        'n' => [6, 5, 5, 5, 5],
        'o' => [2, 5, 5, 5, 2],
        'p' => [6, 5, 6, 4, 4],
        'q' => [2, 5, 5, 6, 3],
        'r' => [6, 5, 6, 5, 5],
        's' => [3, 4, 2, 1, 6],
        't' => [7, 2, 2, 2, 2],
        'u' => [5, 5, 5, 5, 7],
        'v' => [5, 5, 5, 5, 2],
        'w' => [5, 5, 7, 7, 5],
        'x' => [5, 5, 2, 5, 5],
        'y' => [5, 5, 2, 2, 2],
        'z' => [7, 1, 2, 4, 7],
        '.' => [0, 0, 0, 0, 2],
        '-' => [0, 0, 7, 0, 0],
        '_' => [0, 0, 0, 0, 7],
        ':' => [0, 2, 0, 2, 0],
        '=' => [0, 7, 0, 7, 0],
        '%' => [5, 1, 2, 4, 5],
        '/' => [1, 1, 2, 4, 4],
        '+' => [0, 2, 7, 2, 0],
        ' ' => [0; 5],
        _ => [7, 5, 5, 5, 7],
    }
}

/// Width in pixels of `text` drawn at `scale`.
pub fn text_width(text: &str, scale: u32) -> u32 {
    let n = text.chars().count() as u32;
    if n == 0 {
        0
    } else {
        (n * (GLYPH_W + 1) - 1) * scale.max(1)
    }
}

/// Height in pixels of one line of text at `scale`.
pub fn text_height(scale: u32) -> u32 {
    GLYPH_H * scale.max(1)
}

fn draw_text(
    rgba: &mut [u8],
    width: u32,
    x0: u32,
    y0: u32,
    text: &str,
    color: [u8; 4],
    scale: u32,
) {
    let scale = scale.max(1);
    let height = rgba.len() as u32 / 4 / width.max(1);
    for (n, ch) in text.chars().enumerate() {
        let rows = glyph(ch);
        let gx = x0 + n as u32 * (GLYPH_W + 1) * scale;
        for (ry, row) in rows.iter().enumerate() {
            for rx in 0..GLYPH_W {
                if row >> (GLYPH_W - 1 - rx) & 1 == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = gx + rx * scale + dx;
                        let y = y0 + ry as u32 * scale + dy;
                        if x < width && y < height {
                            let i = ((y * width + x) * 4) as usize;
                            rgba[i..i + 4].copy_from_slice(&color);
                        }
                    }
                }
            }
        }
    }
}

/// One image with a title above it and lines of text below it.
pub struct Panel {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
    pub title: String,
    pub lines: Vec<String>,
}

/// Lay `panels` out left to right on a dark sheet. Panels may differ in size;
/// each column is as wide as its image or its widest line.
pub fn contact_sheet(panels: &[Panel], text_scale: u32) -> PxlResult<(u32, u32, Vec<u8>)> {
    if panels.is_empty() {
        return Err(PxlError::usage(
            "a comparison sheet needs at least one panel",
        ));
    }
    let ts = text_scale.max(1);
    let line_h = (GLYPH_H + 2) * ts;
    let mut columns = Vec::with_capacity(panels.len());
    for p in panels {
        if p.rgba.len() as u64 != p.w as u64 * p.h as u64 * 4 {
            return Err(PxlError::semantic(format!(
                "panel '{}' has {} bytes for {}x{}",
                p.title,
                p.rgba.len(),
                p.w,
                p.h
            )));
        }
        let text = p
            .lines
            .iter()
            .map(|l| text_width(l, ts))
            .chain([text_width(&p.title, ts)])
            .max()
            .unwrap_or(0);
        columns.push(p.w.max(text));
    }
    let image_h = panels.iter().map(|p| p.h).max().unwrap_or(0);
    let lines = panels
        .iter()
        .map(|p| p.lines.len() as u32)
        .max()
        .unwrap_or(0);
    let total_w = MARGIN + columns.iter().map(|c| c + MARGIN).sum::<u32>();
    let total_h = MARGIN + line_h + image_h + MARGIN / 2 + lines * line_h + MARGIN;
    scaled_dims(total_w, total_h, 1)?;
    let mut rgba = vec![0u8; (total_w as usize) * (total_h as usize) * 4];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&BACKDROP);
    }
    let mut x = MARGIN;
    for (p, col) in panels.iter().zip(&columns) {
        draw_text(&mut rgba, total_w, x, MARGIN, &p.title, TITLE, ts);
        let top = MARGIN + line_h;
        for row in 0..p.h {
            let s = (row * p.w * 4) as usize;
            let d = (((top + row) * total_w + x) * 4) as usize;
            rgba[d..d + (p.w * 4) as usize].copy_from_slice(&p.rgba[s..s + (p.w * 4) as usize]);
        }
        let mut y = top + image_h + MARGIN / 2;
        for line in &p.lines {
            draw_text(&mut rgba, total_w, x, y, line, BODY, ts);
            y += line_h;
        }
        x += col + MARGIN;
    }
    Ok((total_w, total_h, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_documented_character_has_its_own_glyph() {
        let set: Vec<char> = "0123456789abcdefghijklmnopqrstuvwxyz.-_:=%/+"
            .chars()
            .collect();
        for (i, a) in set.iter().enumerate() {
            assert_ne!(glyph(*a), [0; 5], "'{a}' draws nothing");
            for b in &set[i + 1..] {
                assert_ne!(glyph(*a), glyph(*b), "'{a}' and '{b}' look the same");
            }
        }
        assert_eq!(glyph('A'), glyph('a'));
    }

    #[test]
    fn text_width_counts_the_gap_between_glyphs_but_not_after_the_last() {
        assert_eq!(text_width("", 2), 0);
        assert_eq!(text_width("a", 1), 3);
        assert_eq!(text_width("ab", 2), 14);
    }

    fn panel(title: &str, w: u32, h: u32, shade: u8) -> Panel {
        Panel {
            w,
            h,
            rgba: vec![shade; (w * h * 4) as usize],
            title: title.into(),
            lines: vec!["jag 3".into(), "ramp 7000".into()],
        }
    }

    #[test]
    fn a_sheet_holds_every_panel_image_untouched() {
        let panels = [panel("copy", 12, 6, 200), panel("hybrid", 12, 6, 100)];
        let (w, h, rgba) = contact_sheet(&panels, 1).unwrap();
        assert_eq!(rgba.len() as u32, w * h * 4);
        let at = |x: u32, y: u32| rgba[((y * w + x) * 4) as usize];
        let top = MARGIN + (GLYPH_H + 2);
        assert_eq!(at(MARGIN + 3, top + 2), 200);
        let second = MARGIN + text_width("ramp 7000", 1).max(12) + MARGIN;
        assert_eq!(at(second + 3, top + 2), 100);
        assert_eq!(&rgba[0..4], &BACKDROP);
    }

    #[test]
    fn labels_leave_ink_on_the_sheet() {
        let (w, h, rgba) = contact_sheet(&[panel("copy", 40, 8, 10)], 2).unwrap();
        let ink = rgba
            .chunks_exact(4)
            .filter(|p| p == &TITLE || p == &BODY)
            .count();
        assert!(ink > 0, "no label pixels on {w}x{h}");
    }

    #[test]
    fn a_bad_panel_buffer_or_an_empty_list_is_refused() {
        assert!(contact_sheet(&[], 1).is_err());
        let mut bad = panel("x", 4, 4, 0);
        bad.rgba.pop();
        assert!(contact_sheet(&[bad], 1).is_err());
    }
}
