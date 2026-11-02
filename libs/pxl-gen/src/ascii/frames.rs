//! Frame grids: inline rows, layer composites, and delta patches.

use std::collections::{BTreeMap, BTreeSet};

use pxl_core::{Palette, PxlError, PxlResult};
use pxl_format::{AsciiFrame, AsciiPatch, AsciiSpec, AsciiTile, Op};

use super::{paint_char, CellPaint, Grid};

type FrameGrids = (BTreeMap<String, Grid>, BTreeMap<String, Vec<Op>>);

pub(super) fn resolve_frames(
    spec: &AsciiSpec,
    paints: &BTreeMap<char, CellPaint>,
    palette: &Palette,
    used: &mut BTreeSet<char>,
) -> PxlResult<FrameGrids> {
    let mut memo: BTreeMap<String, (Grid, Vec<Op>)> = BTreeMap::new();
    let mut visiting = BTreeSet::new();
    if spec.frames.is_empty() {
        let order: Vec<String> = spec
            .layers
            .iter()
            .filter(|layer| layer.visible)
            .map(|layer| layer.name.clone())
            .collect();
        let grid = composite_named(spec, &order, paints, palette, used, "base")?;
        memo.insert("base".into(), (grid, Vec::new()));
    } else {
        for name in spec.frames.keys() {
            resolve_one(name, spec, paints, palette, used, &mut memo, &mut visiting)?;
        }
    }
    let delta_ops = memo
        .iter()
        .filter(|(_, (_, ops))| !ops.is_empty())
        .map(|(name, (_, ops))| (name.clone(), ops.clone()))
        .collect();
    let grids = memo
        .into_iter()
        .map(|(name, (grid, _))| (name, grid))
        .collect();
    Ok((grids, delta_ops))
}

fn resolve_one(
    name: &str,
    spec: &AsciiSpec,
    paints: &BTreeMap<char, CellPaint>,
    palette: &Palette,
    used: &mut BTreeSet<char>,
    memo: &mut BTreeMap<String, (Grid, Vec<Op>)>,
    visiting: &mut BTreeSet<String>,
) -> PxlResult<()> {
    if memo.contains_key(name) {
        return Ok(());
    }
    if !visiting.insert(name.to_string()) {
        return Err(PxlError::schema(format!(
            "ascii.frame_cycle: frame '{name}' refers back to itself"
        ))
        .with_path(format!("/frames/{name}")));
    }
    let frame = spec.frames.get(name).ok_or_else(|| {
        PxlError::schema(format!("ascii.frame_source: frame '{name}' is not defined"))
            .with_hint("define the frame under /frames, or fix the reference")
    })?;
    let entry = match frame {
        AsciiFrame::Rows { rows } => (
            grid_from_rows(
                rows,
                paints,
                spec,
                palette,
                used,
                &format!("/frames/{name}/rows"),
            )?,
            Vec::new(),
        ),
        AsciiFrame::Tiles { tiles } => (
            grid_from_tiles(
                tiles,
                paints,
                spec,
                palette,
                used,
                &format!("/frames/{name}/tiles"),
            )?,
            Vec::new(),
        ),
        AsciiFrame::Layers { layers } => {
            let order: Vec<String> = if layers.is_empty() {
                spec.layers
                    .iter()
                    .filter(|layer| layer.visible)
                    .map(|layer| layer.name.clone())
                    .collect()
            } else {
                layers.clone()
            };
            (
                composite_named(spec, &order, paints, palette, used, name)?,
                Vec::new(),
            )
        }
        AsciiFrame::Delta { from, patch, ops } => {
            resolve_one(from, spec, paints, palette, used, memo, visiting)?;
            let (mut grid, mut inherited) = memo.get(from).cloned().ok_or_else(|| {
                PxlError::semantic(format!(
                    "ascii.frame_source: frame '{name}' reads '{from}', which has no layout"
                ))
            })?;
            for (i, patch) in patch.iter().enumerate() {
                apply_patch(
                    &mut grid,
                    patch,
                    paints,
                    spec,
                    palette,
                    used,
                    &format!("/frames/{name}/patch/{i}"),
                )?;
            }
            inherited.extend(ops.iter().cloned());
            (grid, inherited)
        }
    };
    visiting.remove(name);
    memo.insert(name.to_string(), entry);
    Ok(())
}

fn composite_named(
    spec: &AsciiSpec,
    order: &[String],
    paints: &BTreeMap<char, CellPaint>,
    palette: &Palette,
    used: &mut BTreeSet<char>,
    frame_name: &str,
) -> PxlResult<Grid> {
    if order.is_empty() {
        return Err(PxlError::schema(format!(
            "ascii.no_layout: frame '{frame_name}' has no layers to draw"
        ))
        .with_hint("add layers, or write the frame as inline rows"));
    }
    let mut parts = Vec::new();
    for layer_name in order {
        let layer = spec
            .layers
            .iter()
            .find(|layer| &layer.name == layer_name)
            .ok_or_else(|| {
                PxlError::schema(format!(
                    "ascii.undefined_layer: frame '{frame_name}' uses layer '{layer_name}'"
                ))
                .with_path("/layers")
                .with_hint("declare the layer under /layers before referencing it")
            })?;
        if !layer.visible {
            continue;
        }
        let grid = grid_from_rows(
            &layer.rows,
            paints,
            spec,
            palette,
            used,
            &format!("/layers/{layer_name}/rows"),
        )?;
        parts.push((layer.x, layer.y, grid));
    }
    if parts.is_empty() {
        return Err(PxlError::schema(format!(
            "ascii.no_layout: frame '{frame_name}' has no visible layers"
        )));
    }
    Ok(composite(parts))
}

fn composite(parts: Vec<(i16, i16, Grid)>) -> Grid {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (0i32, 0i32, 0i32, 0i32);
    for (x, y, grid) in &parts {
        let (x, y) = (*x as i32, *y as i32);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + grid.w as i32);
        max_y = max_y.max(y + grid.h as i32);
    }
    let w = (max_x - min_x).max(1) as usize;
    let h = (max_y - min_y).max(1) as usize;
    let mut out = Grid::new(w, h);
    for (x, y, grid) in parts {
        let (x, y) = (x as i32 - min_x, y as i32 - min_y);
        for gy in 0..grid.h {
            for gx in 0..grid.w {
                if let Some(paint) = &grid.cells[gy * grid.w + gx] {
                    out.set(
                        (x + gx as i32) as usize,
                        (y + gy as i32) as usize,
                        Some(paint.clone()),
                    );
                }
            }
        }
    }
    out
}

fn grid_from_rows(
    rows: &[String],
    paints: &BTreeMap<char, CellPaint>,
    spec: &AsciiSpec,
    palette: &Palette,
    used: &mut BTreeSet<char>,
    path: &str,
) -> PxlResult<Grid> {
    let Some(first) = rows.first() else {
        return Err(
            PxlError::schema(format!("ascii.empty_layout: {path} has no rows")).with_path(path),
        );
    };
    let width = spec.row_cells(first, &format!("{path}/0"))?.len();
    if width == 0 {
        return Err(
            PxlError::schema(format!("ascii.empty_layout: {path} row 0 is empty")).with_path(path),
        );
    }
    let mut grid = Grid::new(width, rows.len());
    for (y, row) in rows.iter().enumerate() {
        let chars = spec.row_cells(row, &format!("{path}/{y}"))?;
        if chars.len() != width {
            let head: String = chars.iter().take(8).collect();
            return Err(PxlError::schema(format!(
                "ascii.row_width: {path} row {y} is {} cells, expected {width} (starts '{head}')",
                chars.len()
            ))
            .with_path(format!("{path}/{y}"))
            .with_hint("every row of a block has the same width; count the cells of this row against row 0"));
        }
        for (x, c) in chars.iter().enumerate() {
            let paint = paint_char(
                *c,
                paints,
                spec.charset,
                palette.len(),
                used,
                &format!("{path}/{y}/{x}"),
            )?;
            grid.set(x, y, paint);
        }
    }
    Ok(grid)
}

/// Tiles are placed on an empty `size x size` cell grid (cells, so a 2x2
/// cell layout on a 128 canvas is a 64 grid). Overlap and overflow are errors:
/// a tiled frame is meant to be edited one block at a time.
fn grid_from_tiles(
    tiles: &[AsciiTile],
    paints: &BTreeMap<char, CellPaint>,
    spec: &AsciiSpec,
    palette: &Palette,
    used: &mut BTreeSet<char>,
    path: &str,
) -> PxlResult<Grid> {
    if tiles.is_empty() {
        return Err(
            PxlError::schema(format!("ascii.empty_layout: {path} has no tiles")).with_path(path),
        );
    }
    let gw = (spec.size / spec.cell_w()) as usize;
    let gh = (spec.size / spec.cell_h()) as usize;
    let mut grid = Grid::new(gw, gh);
    let mut owner: Vec<Option<usize>> = vec![None; gw * gh];
    for (i, tile) in tiles.iter().enumerate() {
        let tpath = format!("{path}/{i}");
        let block = grid_from_rows(
            &tile.rows,
            paints,
            spec,
            palette,
            used,
            &format!("{tpath}/rows"),
        )?;
        let (ox, oy) = (tile.at[0] as usize, tile.at[1] as usize);
        if ox + block.w > gw || oy + block.h > gh {
            return Err(PxlError::semantic(format!(
                "ascii.tile_bounds: tile {i} at ({ox},{oy}) is {}x{} and leaves the {gw}x{gh} cell grid",
                block.w, block.h
            ))
            .with_path(tpath)
            .with_hint("move the tile or make it smaller"));
        }
        for y in 0..block.h {
            for x in 0..block.w {
                let at = (oy + y) * gw + ox + x;
                if let Some(prev) = owner[at] {
                    return Err(PxlError::semantic(format!(
                        "ascii.tile_overlap: tiles {prev} and {i} both cover cell ({},{})",
                        ox + x,
                        oy + y
                    ))
                    .with_path(tpath));
                }
                owner[at] = Some(i);
                grid.cells[at] = block.cells[y * block.w + x].clone();
            }
        }
    }
    Ok(grid)
}

fn apply_patch(
    grid: &mut Grid,
    patch: &AsciiPatch,
    paints: &BTreeMap<char, CellPaint>,
    spec: &AsciiSpec,
    palette: &Palette,
    used: &mut BTreeSet<char>,
    path: &str,
) -> PxlResult<()> {
    let Some(first) = patch.rows.first() else {
        return Err(
            PxlError::schema(format!("ascii.empty_layout: {path} has no rows")).with_path(path),
        );
    };
    let width = spec.row_cells(first, &format!("{path}/0"))?.len();
    let keep = spec.keep_char();
    let need_w = patch.x as usize + width;
    let need_h = patch.y as usize + patch.rows.len();
    grid.grow(need_w, need_h);
    for (y, row) in patch.rows.iter().enumerate() {
        let chars = spec.row_cells(row, &format!("{path}/{y}"))?;
        if chars.len() != width {
            return Err(PxlError::schema(format!(
                "ascii.row_width: {path} row {y} is {} cells, expected {width}",
                chars.len()
            ))
            .with_path(format!("{path}/{y}")));
        }
        for (x, c) in chars.iter().enumerate() {
            if *c == keep {
                continue;
            }
            if !spec.charset.accepts(*c) {
                return Err(PxlError::schema(format!(
                    "ascii.bad_char: '{c}' in a patch row is not allowed by charset {}",
                    spec.charset.name()
                ))
                .with_path(format!("{path}/{y}/{x}"))
                .with_hint(format!(
                    "'{keep}' keeps a cell unchanged; the transparent character erases it"
                )));
            }
            let paint = paint_char(
                *c,
                paints,
                spec.charset,
                palette.len(),
                used,
                &format!("{path}/{y}/{x}"),
            )?;
            grid.set(patch.x as usize + x, patch.y as usize + y, paint);
        }
    }
    Ok(())
}

/// Stamps carry palette indices, so they need a palette to be meaningful.
pub(super) fn stamp_pixels(
    rows: &[String],
    cell: [u16; 2],
    palette_len: usize,
    path: &str,
) -> PxlResult<Vec<u8>> {
    let frame = pxl_core::decode_matrix(rows).map_err(|e| {
        e.with_path(path).with_hint(
            "a stamp uses the standard text matrix: '.' transparent, 0-9 and a-z indices",
        )
    })?;
    if frame.w != cell[0] || frame.h != cell[1] {
        return Err(PxlError::schema(format!(
            "ascii.stamp_size: stamp is {}x{}, cell is {}x{}",
            frame.w, frame.h, cell[0], cell[1]
        ))
        .with_path(path)
        .with_hint("a stamp fills exactly one cell"));
    }
    for &pixel in &frame.pixels {
        if pixel as usize >= palette_len {
            return Err(PxlError::semantic(format!(
                "ascii.palette_overflow: stamp at {path} uses index {pixel}, but the palette has {palette_len} colours"
            ))
            .with_path(path));
        }
    }
    Ok(frame.pixels)
}
