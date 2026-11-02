//! Wang-mask helpers shared by the scene compiler and tests.
use pxl_contract::tileset::{
    Tileset, MASK_E, MASK_N, MASK_NE, MASK_NW, MASK_S, MASK_SE, MASK_SW, MASK_W,
};

/// How one mask was resolved for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskResolution {
    /// Drawn exactly by this tile.
    Exact(String),
    /// Approximated by this tile; the report must say so.
    Approximate(String),
}

/// Resolve the 47-mask for one cell of a terrain grid. `same(x, y)` reports
/// whether the cell belongs to the terrain being drawn; out-of-bounds cells
/// are not the same terrain. Shared fixtures keep this in step with
/// `scene-select.js neighbourMask`.
pub fn cell_mask<F>(width: i32, height: i32, x: i32, y: i32, same: F) -> u8
where
    F: Fn(i32, i32) -> bool,
{
    let look = |dx: i32, dy: i32| -> bool {
        let (nx, ny) = (x + dx, y + dy);
        nx >= 0 && ny >= 0 && nx < width && ny < height && same(nx, ny)
    };
    let (n, e, s, w) = (look(0, -1), look(1, 0), look(0, 1), look(-1, 0));
    let mut raw = 0u8;
    if n {
        raw |= MASK_N;
    }
    if e {
        raw |= MASK_E;
    }
    if s {
        raw |= MASK_S;
    }
    if w {
        raw |= MASK_W;
    }
    if look(1, -1) {
        raw |= MASK_NE;
    }
    if look(1, 1) {
        raw |= MASK_SE;
    }
    if look(-1, 1) {
        raw |= MASK_SW;
    }
    if look(-1, -1) {
        raw |= MASK_NW;
    }
    pxl_contract::tileset::reduce_mask(raw)
}

/// Resolve which tile draws one mask: exact first, approximation second, and
/// `None` only when the tileset has neither — the caller must report that gap.
pub fn tile_for_mask(set: &Tileset, mask: u8) -> Option<MaskResolution> {
    if let Some(tile) = set.tiles.iter().find(|t| t.exact_masks.contains(&mask)) {
        return Some(MaskResolution::Exact(tile.id.clone()));
    }
    if let Some(tile) = set.tiles.iter().find(|t| t.approximate_masks.contains(&mask)) {
        return Some(MaskResolution::Approximate(tile.id.clone()));
    }
    None
}

