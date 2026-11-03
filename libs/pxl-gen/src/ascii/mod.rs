//! ASCII layout compiler.
//!
//! The text grid only carries layout. The legend turns each character into a
//! palette index, a block pattern, or a cell-sized stamp; the compiler expands
//! cells, places the result on a 32/64/128/256 canvas, and returns plain frames. Pixel
//! ops (`post`, `mirror`, delta `ops`) stay with the caller, which owns
//! `pxl-ops`.

use std::collections::{BTreeMap, BTreeSet};

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult};
use pxl_format::{AsciiSpec, Charset, LegendValue, Op};

mod frames;
mod legend;

use frames::resolve_frames;

#[derive(Debug, Clone)]
pub struct AsciiWarning {
    pub id: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct AsciiCompiled {
    /// Every frame is `spec.size × spec.size`, sorted by name.
    pub frames: Vec<(String, IndexFrame)>,
    /// Palette the frames were resolved against, including appended colours.
    pub palette: Palette,
    /// Palette file to write when colours had to be generated or appended.
    pub palette_file: Option<pxl_format::PaletteFile>,
    pub warnings: Vec<AsciiWarning>,
    pub post: Vec<Op>,
    pub mirror: Option<String>,
    /// Per-frame ops that run after the patch, before `mirror` and `post`.
    pub delta_ops: BTreeMap<String, Vec<Op>>,
}

/// One character's paint inside a cell.
#[derive(Debug, Clone)]
pub(crate) enum CellPaint {
    Solid(u8),
    Pattern {
        base: u8,
        alt: u8,
        kind: PatternKind,
        period: i32,
    },
    /// Row-major `cell.w × cell.h` pixels.
    Stamp(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatternKind {
    Checker,
    StripeX,
    StripeY,
    Bevel,
}

/// A layout grid: one entry per cell, `None` meaning transparent.
#[derive(Debug, Clone)]
pub(crate) struct Grid {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<Option<CellPaint>>,
}

impl Grid {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            cells: vec![None; w * h],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, paint: Option<CellPaint>) {
        if x < self.w && y < self.h {
            self.cells[y * self.w + x] = paint;
        }
    }

    /// Grow to at least `w × h`, keeping existing cells at their coordinates.
    pub fn grow(&mut self, w: usize, h: usize) {
        if w <= self.w && h <= self.h {
            return;
        }
        let (w, h) = (w.max(self.w), h.max(self.h));
        let mut cells = vec![None; w * h];
        for y in 0..self.h {
            for x in 0..self.w {
                cells[y * w + x] = self.cells[y * self.w + x].clone();
            }
        }
        self.w = w;
        self.h = h;
        self.cells = cells;
    }
}

pub fn compile_ascii(
    spec: &AsciiSpec,
    palette: Option<&Palette>,
    shared_legend: &BTreeMap<String, LegendValue>,
) -> PxlResult<AsciiCompiled> {
    spec.validate()?;
    let plan = legend::resolve_legend(spec, palette, shared_legend)?;
    let mut used = BTreeSet::new();
    let (grids, delta_ops) = resolve_frames(spec, &plan.paints, &plan.palette, &mut used)?;
    let mut frames = Vec::with_capacity(grids.len());
    for (name, grid) in grids {
        let layout = expand(&grid, spec, &name)?;
        frames.push((name.clone(), place(&layout, spec, &name)?));
    }
    let mut warnings = Vec::new();
    for ch in &plan.auto_chars {
        warnings.push(AsciiWarning {
            id: "ascii.auto_legend".into(),
            message: format!(
                "character '{ch}' still uses an auto colour; set an explicit index or color"
            ),
        });
    }
    for key in spec.legend.keys() {
        let Some(c) = key.chars().next() else {
            continue;
        };
        if !used.contains(&c) {
            warnings.push(AsciiWarning {
                id: "ascii.unused_legend".into(),
                message: format!("legend entry '{c}' is not used by any frame"),
            });
        }
    }
    Ok(AsciiCompiled {
        frames,
        palette: plan.palette,
        palette_file: plan.palette_file,
        warnings,
        post: spec.post.clone(),
        mirror: spec.mirror.clone(),
        delta_ops,
    })
}

/// Resolve one layout character through the legend, falling back to the
/// charset's default mapping.
pub(crate) fn paint_char(
    c: char,
    paints: &BTreeMap<char, CellPaint>,
    charset: Charset,
    palette_len: usize,
    used: &mut BTreeSet<char>,
    path: &str,
) -> PxlResult<Option<CellPaint>> {
    if let Some(paint) = paints.get(&c) {
        used.insert(c);
        return Ok(match paint {
            CellPaint::Solid(0) => None,
            other => Some(other.clone()),
        });
    }
    match charset {
        Charset::Digits => match c {
            '0' => Ok(None),
            '1'..='9' => {
                let index = c as u8 - b'0';
                check_index(index, palette_len, c, path)?;
                Ok(Some(CellPaint::Solid(index)))
            }
            other => Err(PxlError::schema(format!(
                "ascii.bad_char: '{other}' at {path} is not a digit"
            ))
            .with_path(path)
            .with_hint("charset digits allows 0-9; 0 is transparent")),
        },
        Charset::Printable => match c {
            '.' | ' ' => Ok(None),
            other => Err(PxlError::schema(format!(
                "ascii.undefined_char: '{other}' at {path} has no legend entry"
            ))
            .with_path(path)
            .with_hint("add a legend entry for every symbol, or use '.' and space for empty")),
        },
    }
}

pub(crate) fn check_index(index: u8, palette_len: usize, c: char, path: &str) -> PxlResult<()> {
    if index as usize >= palette_len {
        return Err(PxlError::semantic(format!(
            "ascii.palette_overflow: '{c}' at {path} uses index {index}, but the palette has {palette_len} colours"
        ))
        .with_path(path)
        .with_hint("use an index that exists in the palette, or add colours with palette_mode extend"));
    }
    Ok(())
}

/// Name the largest square cell that still fits, so the caller can act on it.
fn overflow_hint(grid: &Grid, spec: &AsciiSpec) -> String {
    let span = grid.w.max(grid.h).max(1);
    let fit = spec.size as usize / span;
    let next = pxl_core::SIZE_TIERS
        .iter()
        .copied()
        .find(|s| *s > spec.size && *s as usize >= span);
    match (fit >= 1, next) {
        (true, Some(n)) => format!(
            "the largest cell that fits is {fit}x{fit} (set cell to [{fit}, {fit}]), or use size {n}"
        ),
        (true, None) => format!("the largest cell that fits is {fit}x{fit} (set cell to [{fit}, {fit}])"),
        (false, Some(n)) => format!(
            "the layout has more cells than the canvas has pixels; use size {n} or fewer columns"
        ),
        (false, None) => "the layout has more cells than a 256 canvas has pixels; use fewer columns".into(),
    }
}

fn expand(grid: &Grid, spec: &AsciiSpec, name: &str) -> PxlResult<IndexFrame> {
    let cw = spec.cell_w() as usize;
    let chh = spec.cell_h() as usize;
    let w = grid.w * cw;
    let h = grid.h * chh;
    let size = spec.size as usize;
    if w > size || h > size {
        return Err(PxlError::semantic(format!(
            "ascii.size_overflow: frame '{name}' expands to {w}x{h}, over the {}x{} canvas",
            spec.size, spec.size
        ))
        .with_path(format!("/frames/{name}"))
        .with_hint(overflow_hint(grid, spec)));
    }
    let mut frame = IndexFrame::new(w as u16, h as u16)?;
    for (i, cell) in grid.cells.iter().enumerate() {
        let Some(paint) = cell else {
            continue;
        };
        let cx = (i % grid.w * cw) as i32;
        let cy = (i / grid.w * chh) as i32;
        match paint {
            CellPaint::Solid(color) => {
                for py in 0..chh as i32 {
                    for px in 0..cw as i32 {
                        frame.set(cx + px, cy + py, *color);
                    }
                }
            }
            CellPaint::Pattern {
                base,
                alt,
                kind,
                period,
            } => {
                for py in 0..chh as i32 {
                    for px in 0..cw as i32 {
                        let light = match kind {
                            PatternKind::Checker => (px + py) % 2 == 0,
                            PatternKind::StripeX => px.rem_euclid(*period) < *period / 2,
                            PatternKind::StripeY => py.rem_euclid(*period) < *period / 2,
                            PatternKind::Bevel => px == 0 || py == 0,
                        };
                        let color = if light { *base } else { *alt };
                        if color != 0 {
                            frame.set(cx + px, cy + py, color);
                        }
                    }
                }
            }
            CellPaint::Stamp(pixels) => {
                for py in 0..chh {
                    for px in 0..cw {
                        let color = pixels[py * cw + px];
                        if color != 0 {
                            frame.set(cx + px as i32, cy + py as i32, color);
                        }
                    }
                }
            }
        }
    }
    Ok(frame)
}

fn place(layout: &IndexFrame, spec: &AsciiSpec, name: &str) -> PxlResult<IndexFrame> {
    let size = spec.size as i32;
    let (lw, lh) = (layout.w as i32, layout.h as i32);
    let (mut x, mut y) = match spec.anchor {
        pxl_format::Anchor::Center => ((size - lw) / 2, (size - lh) / 2),
        pxl_format::Anchor::BottomCenter => ((size - lw) / 2, size - lh),
        pxl_format::Anchor::TopLeft => (0, 0),
    };
    x += spec.offset[0] as i32;
    y += spec.offset[1] as i32;
    if x < 0 || y < 0 || x + lw > size || y + lh > size {
        return Err(PxlError::semantic(format!(
            "ascii.offset_out_of_bounds: frame '{name}' lands at ({x},{y}) and leaves the {size}x{size} canvas"
        ))
        .with_path(format!("/frames/{name}"))
        .with_hint("reduce cell/columns or adjust anchor and offset"));
    }
    let mut out = IndexFrame::new(spec.size, spec.size)?;
    for py in 0..lh {
        for px in 0..lw {
            let color = layout.get(px, py);
            if color != 0 {
                out.set(x + px, y + py, color);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
