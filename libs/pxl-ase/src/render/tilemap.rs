//! Tilemap cel compositing (GE-5 follow-up).

use pxl_ase_doc::{BlendMode, Cel, ColorMode, Layer, Sprite, Tileset};
use pxl_core::Rgba;

use super::blend;

/// ASE tile id flags (ase-file-specs).
const FLIP_X: u32 = 0x2000_0000;
const FLIP_Y: u32 = 0x4000_0000;
const FLIP_D: u32 = 0x8000_0000;
const TILE_ID_MASK: u32 = 0x1FFF_FFFF;

pub(super) fn blit_tilemap_layer(
    out: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    layer: &Layer,
    frame: u16,
    sprite: &Sprite,
) {
    let Some(cel) = layer.cels.get(&frame) else {
        return;
    };
    let Cel::Tilemap {
        x,
        y,
        opacity,
        w,
        h,
        tiles,
        ..
    } = cel
    else {
        return;
    };
    let ts = match layer.tileset_index {
        Some(id) => sprite.tilesets.iter().find(|t| t.id == id),
        None => sprite.tilesets.first(),
    };
    let Some(ts) = ts else {
        return;
    };
    if ts.pixels.is_empty() || ts.tile_width == 0 || ts.tile_height == 0 {
        return;
    }
    let tw = ts.tile_width as i32;
    let th = ts.tile_height as i32;
    let map_w = *w as i32;
    let map_h = *h as i32;
    for ty in 0..map_h {
        for tx in 0..map_w {
            let ti = (ty * map_w + tx) as usize;
            if ti >= tiles.len() {
                continue;
            }
            let raw = tiles[ti];
            let id = raw & TILE_ID_MASK;
            if id == 0 && ts.base_index != 0 {
                // empty tile when base_index is 1 (common)
                continue;
            }
            let tile_index = if ts.base_index > 0 {
                id.saturating_sub(ts.base_index as u32)
            } else {
                id
            };
            if tile_index >= ts.ntiles {
                continue;
            }
            let flip_x = raw & FLIP_X != 0;
            let flip_y = raw & FLIP_Y != 0;
            let flip_d = raw & FLIP_D != 0;
            blit_one_tile(
                out,
                canvas_w,
                canvas_h,
                sprite,
                ts,
                tile_index as usize,
                *x as i32 + tx * tw,
                *y as i32 + ty * th,
                *opacity,
                layer.opacity,
                layer.blend,
                flip_x,
                flip_y,
                flip_d,
            );
        }
    }
}

fn blit_one_tile(
    out: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    sprite: &Sprite,
    ts: &Tileset,
    tile_index: usize,
    dest_x: i32,
    dest_y: i32,
    cel_opacity: u8,
    layer_opacity: u8,
    blend_mode: BlendMode,
    flip_x: bool,
    flip_y: bool,
    flip_d: bool,
) {
    let tw = ts.tile_width as i32;
    let th = ts.tile_height as i32;
    let bpp = match sprite.color_mode {
        ColorMode::Rgba => 4,
        ColorMode::Grayscale => 2,
        ColorMode::Indexed => 1,
    };
    let row_stride = (ts.tile_width as usize) * bpp;
    let tile_bytes = row_stride * (ts.tile_height as usize);
    let src_off = tile_index.saturating_mul(tile_bytes);
    if src_off + tile_bytes > ts.pixels.len() {
        return;
    }
    let tile = &ts.pixels[src_off..src_off + tile_bytes];
    let out_w = if flip_d { th } else { tw };
    let out_h = if flip_d { tw } else { th };
    for py in 0..out_h {
        for px in 0..out_w {
            // Map output pixel back to source tile coords.
            let (mut sx, mut sy) = if flip_d { (py, px) } else { (px, py) };
            if flip_x {
                sx = tw - 1 - sx;
            }
            if flip_y {
                sy = th - 1 - sy;
            }
            if sx < 0 || sy < 0 || sx >= tw || sy >= th {
                continue;
            }
            let si = (sy as usize) * row_stride + (sx as usize) * bpp;
            let (r, g, b, a) = sample_tile_pixel(sprite, tile, si, bpp);
            if a == 0 {
                continue;
            }
            let dx = dest_x + px;
            let dy = dest_y + py;
            if dx < 0 || dy < 0 || dx >= canvas_w as i32 || dy >= canvas_h as i32 {
                continue;
            }
            let di = ((dy as usize) * canvas_w + dx as usize) * 4;
            let a = scale_alpha(a, cel_opacity, layer_opacity);
            if a == 0 {
                continue;
            }
            blend::blend_pixel(blend_mode, out, di, r, g, b, a);
        }
    }
}

fn sample_tile_pixel(sprite: &Sprite, tile: &[u8], si: usize, bpp: usize) -> (u8, u8, u8, u8) {
    match bpp {
        4 => {
            if si + 3 >= tile.len() {
                return (0, 0, 0, 0);
            }
            (tile[si], tile[si + 1], tile[si + 2], tile[si + 3])
        }
        2 => {
            if si + 1 >= tile.len() {
                return (0, 0, 0, 0);
            }
            let v = tile[si];
            (v, v, v, tile[si + 1])
        }
        _ => {
            if si >= tile.len() {
                return (0, 0, 0, 0);
            }
            let idx = tile[si];
            if idx == sprite.transparent_index {
                return (0, 0, 0, 0);
            }
            let c = sprite
                .palette
                .get(idx as usize)
                .copied()
                .unwrap_or(Rgba::TRANSPARENT);
            (c.r, c.g, c.b, c.a)
        }
    }
}

fn scale_alpha(a: u8, cel: u8, layer: u8) -> u8 {
    (((a as u32) * (cel as u32) * (layer as u32)) / 65025) as u8
}
