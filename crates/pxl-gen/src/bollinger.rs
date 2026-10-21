//! Clean-room Bollinger mask generator.
//!
//! Pipeline, matching the published description of zfedoran/pixel-sprite-generator
//! (MIT) and Dave Bollinger's mask write-up:
//! 1. Turn mask values into body / border / empty.
//! 2. Mirror (the center column or row is duplicated, so the size doubles).
//! 3. Paint a 4-neighbour outline onto empty cells that touch a body.
//! 4. Color from a palette ramp with an integer sine lookup. The original
//!    vertical/horizontal gradient branches are kept distinct (the unlicensed
//!    Python port made them identical; that bug is not reproduced).
//!
//! Mask extensions used by this project: `3` always body, `4` highlight,
//! `5` anchor (not drawn).

use pxl_core::{
    fraction_to_bp, fraction_to_q8, IndexFrame, Mirror, Palette, Pcg32, PxlError, PxlResult,
};
use pxl_format::TemplateFile;

/// sin(i * π / 63) * 255 for i in 0..64. Peak at the middle of the half-wave.
const SIN_HALF_Q8: [u8; 64] = [
    0, 13, 25, 38, 51, 63, 75, 87, 99, 111, 122, 133, 144, 154, 164, 173, 183, 191, 199, 207, 214,
    221, 227, 232, 237, 242, 245, 249, 251, 253, 254, 255, 255, 254, 253, 251, 249, 245, 242, 237,
    232, 227, 221, 214, 207, 199, 191, 183, 173, 164, 154, 144, 133, 122, 111, 99, 87, 75, 63, 51,
    38, 25, 13, 0,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cell {
    Empty,
    Body,
    Border,
    Highlight,
}

#[derive(Clone, Debug)]
pub struct RuntimeMask {
    pub id: String,
    pub mirror: Mirror,
    pub width: u16,
    pub height: u16,
    pub cells: Vec<u8>,
    pub row_hue_bp: u16,
    pub noise_q8: u8,
    pub vertical: bool,
    pub edge_outline: bool,
    pub ramp_pool: Vec<String>,
}

pub type MaskTemplate = RuntimeMask;

impl RuntimeMask {
    pub fn from_file(file: &TemplateFile, mirror_override: Option<Mirror>) -> PxlResult<Self> {
        if file.cells.is_empty() {
            return Err(PxlError::schema("template has no cells"));
        }
        let width = file.cells[0].chars().count();
        let mut cells = Vec::with_capacity(width * file.cells.len());
        for (y, row) in file.cells.iter().enumerate() {
            let chars: Vec<char> = row.chars().collect();
            if chars.len() != width {
                return Err(PxlError::schema(format!(
                    "template row {y} width {}, expected {width}",
                    chars.len()
                )));
            }
            for c in chars {
                let v = c.to_digit(10).ok_or_else(|| {
                    PxlError::schema(format!("template cell '{c}' is not 0-5")).with_hint(
                        "0 empty, 1 maybe, 2 body-or-border, 3 body, 4 highlight, 5 anchor",
                    )
                })?;
                if v > 5 {
                    return Err(PxlError::schema(format!(
                        "template cell {v} is out of range"
                    )));
                }
                cells.push(v as u8);
            }
        }
        let mirror = match mirror_override {
            Some(m) => m,
            None => Mirror::parse(&file.mirror)?,
        };
        let gradient = if file.color.gradient.is_empty() {
            "vertical"
        } else {
            file.color.gradient.as_str()
        };
        let vertical = match gradient {
            "vertical" | "y" => true,
            "horizontal" | "x" => false,
            other => {
                return Err(PxlError::schema(format!("unknown gradient '{other}'"))
                    .with_hint("gradient is vertical or horizontal"));
            }
        };
        Ok(Self {
            id: file.id.clone(),
            mirror,
            width: width as u16,
            height: file.cells.len() as u16,
            cells,
            row_hue_bp: fraction_to_bp(file.color.row_hue_change),
            noise_q8: fraction_to_q8(file.color.noise),
            vertical,
            edge_outline: file.color.edge.is_empty() || file.color.edge == "outline",
            ramp_pool: file.color.ramp_pool.clone(),
        })
    }
}

pub fn generate_bollinger(
    mask: &RuntimeMask,
    seed: u64,
    palette: &Palette,
    ramp_name: Option<&str>,
) -> PxlResult<IndexFrame> {
    let mut rng = Pcg32::from_seed(seed);
    let mut grid = sample_body(mask, &mut rng)?;
    grid = mirror_grid(grid, mask.mirror);
    add_edges(&mut grid);
    colorize(&grid, mask, palette, ramp_name, &mut rng)
}

fn sample_body(mask: &RuntimeMask, rng: &mut Pcg32) -> PxlResult<Vec<Vec<Cell>>> {
    let w = mask.width as usize;
    let h = mask.height as usize;
    let mut rows = Vec::with_capacity(h);
    for y in 0..h {
        let mut row = Vec::with_capacity(w);
        for x in 0..w {
            let v = mask.cells[y * w + x];
            let cell = match v {
                0 => Cell::Empty,
                1 => {
                    if rng.chance_half() {
                        Cell::Body
                    } else {
                        Cell::Empty
                    }
                }
                2 => {
                    if rng.chance_half() {
                        Cell::Body
                    } else {
                        Cell::Border
                    }
                }
                3 => Cell::Body,
                4 => Cell::Highlight,
                5 => Cell::Empty,
                _ => {
                    return Err(PxlError::semantic(format!("bad mask value {v}")));
                }
            };
            row.push(cell);
        }
        rows.push(row);
    }
    Ok(rows)
}

fn mirror_grid(rows: Vec<Vec<Cell>>, mirror: Mirror) -> Vec<Vec<Cell>> {
    let rows = match mirror {
        Mirror::None => rows,
        Mirror::X | Mirror::Xy => rows
            .into_iter()
            .map(|row| {
                let mut out = row.clone();
                out.extend(row.into_iter().rev());
                out
            })
            .collect(),
        Mirror::Y => rows,
    };
    match mirror {
        Mirror::Y | Mirror::Xy => {
            let mut out = rows.clone();
            for row in rows.into_iter().rev() {
                out.push(row);
            }
            out
        }
        _ => rows,
    }
}

fn add_edges(rows: &mut [Vec<Cell>]) {
    if rows.is_empty() {
        return;
    }
    let h = rows.len();
    let w = rows[0].len();
    let mut mark = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            if !is_solid(rows[y][x]) {
                continue;
            }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let (nx, ny) = (nx as usize, ny as usize);
                if rows[ny][nx] == Cell::Empty {
                    mark[ny * w + nx] = true;
                }
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            if mark[y * w + x] {
                rows[y][x] = Cell::Border;
            }
        }
    }
}

fn is_solid(c: Cell) -> bool {
    matches!(c, Cell::Body | Cell::Highlight)
}

fn colorize(
    rows: &[Vec<Cell>],
    mask: &RuntimeMask,
    palette: &Palette,
    ramp_name: Option<&str>,
    rng: &mut Pcg32,
) -> PxlResult<IndexFrame> {
    let h = rows.len();
    let w = rows.first().map(|r| r.len()).unwrap_or(0);
    if w == 0 || h == 0 || w > 64 || h > 64 {
        return Err(
            PxlError::semantic(format!("bollinger output {w}x{h} is outside 1..=64"))
                .with_hint("mirror doubles a template; start from a smaller mask"),
        );
    }
    let ramp = resolve_ramp(palette, ramp_name, &mask.ramp_pool, rng)?;
    let mut frame = IndexFrame::new(w as u16, h as u16)?;
    let mut ramp_cursor = 0usize;
    for y in 0..h {
        // Biased-small draw: two uniform samples, keep the lesser one.
        let a = rng.gen_range(1000);
        let b = rng.gen_range(1000);
        let biased = a.min(b);
        if (biased as u16) < mask.row_hue_bp && ramp.len() > 1 {
            ramp_cursor = (ramp_cursor + 1) % ramp.len();
        }
        for x in 0..w {
            let cell = rows[y][x];
            let color = match cell {
                Cell::Empty => 0,
                Cell::Highlight => {
                    if palette.special.flash != 0 {
                        palette.special.flash
                    } else {
                        *ramp.last().unwrap_or(&1)
                    }
                }
                Cell::Border => {
                    if mask.edge_outline {
                        palette.special.outline
                    } else {
                        ramp[0]
                    }
                }
                Cell::Body => {
                    let t = if mask.vertical { y } else { x };
                    let len = if mask.vertical { h } else { w };
                    let idx = if len <= 1 { 32 } else { t * 63 / (len - 1) };
                    let base = SIN_HALF_Q8[idx.min(63)] as u32;
                    let noise = rng.next_u8() as u32;
                    let n = mask.noise_q8 as u32;
                    let lum = (base * (255 - n) + noise * n) / 255;
                    let steps = ramp.len() as u32;
                    let mut slot = (lum * (steps - 1)) / 255;
                    // Row hue walks the ramp by one slot, then wraps.
                    slot = (slot + ramp_cursor as u32) % steps;
                    ramp[slot as usize]
                }
            };
            frame.set(x as i32, y as i32, color);
        }
    }
    Ok(frame)
}

fn resolve_ramp<'a>(
    palette: &'a Palette,
    requested: Option<&str>,
    pool: &[String],
    rng: &mut Pcg32,
) -> PxlResult<&'a [u8]> {
    if let Some(name) = requested {
        return palette
            .ramps
            .get(name)
            .map(|v| v.as_slice())
            .ok_or_else(|| {
                PxlError::semantic(format!("unknown ramp '{name}'"))
                    .with_hint("ramp names come from the palette file")
            });
    }
    let pool: Vec<&str> = if pool.is_empty() {
        palette.ramps.keys().map(|s| s.as_str()).collect()
    } else {
        pool.iter().map(|s| s.as_str()).collect()
    };
    if pool.is_empty() {
        return Err(
            PxlError::semantic("palette has no ramps for bollinger coloring")
                .with_hint("add a ramp or pass --ramp"),
        );
    }
    let pick = pool[rng.gen_range(pool.len() as u32) as usize];
    palette
        .ramps
        .get(pick)
        .map(|v| v.as_slice())
        .ok_or_else(|| {
            PxlError::semantic(format!(
                "template ramp_pool entry '{pick}' is not on the palette"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_format::parse_template;

    fn palette() -> Palette {
        crate::builtin_palette("p16").unwrap()
    }

    #[test]
    fn same_seed_same_pixels() {
        let file = parse_template(include_str!(
            "../../../assets/templates/slime_8x8.template.json"
        ))
        .unwrap();
        let mask = RuntimeMask::from_file(&file, None).unwrap();
        let pal = palette();
        let a = generate_bollinger(&mask, 42, &pal, Some("slime")).unwrap();
        let b = generate_bollinger(&mask, 42, &pal, Some("slime")).unwrap();
        assert_eq!(a.pixels, b.pixels);
        let c = generate_bollinger(&mask, 43, &pal, Some("slime")).unwrap();
        assert_ne!(a.pixels, c.pixels);
        assert_eq!(a.w, 8);
    }

    #[test]
    fn mirror_doubles_width() {
        let file = parse_template(include_str!(
            "../../../assets/templates/gem_6x6.template.json"
        ))
        .unwrap();
        let mask = RuntimeMask::from_file(&file, None).unwrap();
        assert_eq!(mask.width, 3);
        let frame = generate_bollinger(&mask, 1, &palette(), Some("heat")).unwrap();
        assert_eq!((frame.w, frame.h), (6, 6));
    }

    #[test]
    fn horizontal_gradient_differs_from_vertical() {
        let file = parse_template(include_str!(
            "../../../assets/templates/ship_8x12.template.json"
        ))
        .unwrap();
        let mut vertical = RuntimeMask::from_file(&file, None).unwrap();
        vertical.vertical = true;
        let mut horizontal = vertical.clone();
        horizontal.vertical = false;
        let pal = palette();
        let a = generate_bollinger(&vertical, 9, &pal, Some("metal")).unwrap();
        let b = generate_bollinger(&horizontal, 9, &pal, Some("metal")).unwrap();
        assert_ne!(a.pixels, b.pixels);
    }
}
