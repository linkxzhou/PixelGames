//! Individual ASE chunk readers.

use std::io::{Cursor, Read};

use flate2::read::ZlibDecoder;
use pxl_ase_doc::{
    BlendMode, Cel, ColorMode, GrayImage, ImageBuffer, IndexedImage, Layer, LayerFlags, LayerKind,
    RgbaImage, Slice, SliceKey, Sprite, Tag, TagDirection, UserData,
};
use pxl_core::Rgba;

use crate::error::{IoError, IoResult};

pub(crate) fn read_layer_chunk(c: &mut Cursor<&[u8]>, sprite: &mut Sprite) -> IoResult<()> {
    let flags = read_u16(c)?;
    let kind = LayerKind::from_u16(read_u16(c)?);
    let child_level = read_u16(c)?;
    let _ = read_u16(c)?;
    let _ = read_u16(c)?;
    let blend = BlendMode::from_u16(read_u16(c)?);
    let opacity = read_u8(c)?;
    let mut pad = [0u8; 3];
    c.read_exact(&mut pad)?;
    let name = read_string(c)?;
    let tileset_index = if kind == LayerKind::Tilemap {
        Some(read_u32(c)?)
    } else {
        None
    };
    let id = pxl_ase_doc::LayerId(sprite.layers.len() as u16);
    sprite.layers.push(Layer {
        id,
        name,
        kind,
        flags: LayerFlags::from_bits(flags),
        child_level,
        blend,
        opacity,
        cels: Default::default(),
        tileset_index,
        uuid: None,
        user_data: UserData::default(),
    });
    Ok(())
}

pub(crate) fn read_cel_chunk(
    c: &mut Cursor<&[u8]>,
    sprite: &mut Sprite,
    frame: u16,
    color_mode: ColorMode,
    data_end: u64,
) -> IoResult<Option<usize>> {
    let layer_index = read_u16(c)?;
    let x = read_i16(c)?;
    let y = read_i16(c)?;
    let opacity = read_u8(c)?;
    let cel_type = read_u16(c)?;
    let z_index = read_i16(c)?;
    let mut pad = [0u8; 5];
    c.read_exact(&mut pad)?;

    let layer_id = pxl_ase_doc::LayerId(layer_index);
    let Some(layer_pos) = sprite.layers.iter().position(|l| l.id == layer_id) else {
        c.set_position(data_end);
        return Ok(None);
    };

    match cel_type {
        1 => {
            let link = read_u16(c)?;
            sprite.layers[layer_pos].cels.insert(
                frame,
                Cel::Linked {
                    frame: link,
                    z_index,
                    user_data: UserData::default(),
            extra: None,
        },
            );
        }
        0 | 2 => {
            let w = read_u16(c)?;
            let h = read_u16(c)?;
            let raw_len = (w as usize)
                .saturating_mul(h as usize)
                .saturating_mul(color_mode.bytes_per_pixel());
            let pixels = if cel_type == 0 {
                let mut buf = vec![0u8; raw_len];
                c.read_exact(&mut buf)?;
                buf
            } else {
                let compressed_len = (data_end.saturating_sub(c.position())) as usize;
                let mut compressed = vec![0u8; compressed_len];
                c.read_exact(&mut compressed)?;
                inflate(&compressed, raw_len)?
            };
            let image = match color_mode {
                ColorMode::Rgba => ImageBuffer::Rgba(RgbaImage::from_rgba(w, h, pixels)),
                ColorMode::Grayscale => ImageBuffer::Grayscale(GrayImage::from_va(w, h, pixels)),
                ColorMode::Indexed => ImageBuffer::Indexed(IndexedImage::from_indices(w, h, pixels)),
            };
            let image_id = sprite.insert_image(image);
            sprite.layers[layer_pos].cels.insert(
                frame,
                Cel::Image {
                    image: image_id,
                    x,
                    y,
                    opacity,
                    z_index,
                    user_data: UserData::default(),
            extra: None,
        },
            );
        }
        3 => {
            let w = read_u16(c)?;
            let h = read_u16(c)?;
            let compressed_len = read_u32(c)? as usize;
            let mut compressed = vec![0u8; compressed_len.min((data_end.saturating_sub(c.position())) as usize)];
            c.read_exact(&mut compressed)?;
            let expect = (w as usize).saturating_mul(h as usize).saturating_mul(4);
            let raw = inflate(&compressed, expect)?;
            if raw.len() < expect {
                return Err(IoError::new("tilemap cel truncated"));
            }
            let mut tiles = Vec::with_capacity(w as usize * h as usize);
            for chunk in raw[..expect].chunks_exact(4) {
                tiles.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
            sprite.layers[layer_pos].cels.insert(
                frame,
                Cel::Tilemap {
                    x,
                    y,
                    opacity,
                    z_index,
                    w,
                    h,
                    tiles,
                    user_data: UserData::default(),
                    extra: None,
                },
            );
        }
        other => {
            return Err(IoError::new(format!("unsupported cel type {other}")));
        }
    }
    Ok(Some(layer_pos))
}

pub(crate) fn read_tags_chunk(c: &mut Cursor<&[u8]>, sprite: &mut Sprite) -> IoResult<()> {
    let n = read_u16(c)? as usize;
    let mut pad = [0u8; 8];
    c.read_exact(&mut pad)?;
    for _ in 0..n {
        let from = read_u16(c)?;
        let to = read_u16(c)?;
        let direction = TagDirection::from_u8(read_u8(c)?);
        let repeat = read_u16(c)?;
        let mut future = [0u8; 6];
        c.read_exact(&mut future)?;
        let mut rgb = [0u8; 3];
        c.read_exact(&mut rgb)?;
        let _ = read_u8(c)?;
        let name = read_string(c)?;
        let user_data = if rgb != [0, 0, 0] {
            UserData::from_rgb_compat(rgb[0], rgb[1], rgb[2])
        } else {
            UserData::default()
        };
        sprite.tags.push(Tag {
            name,
            from,
            to,
            direction,
            repeat,
            user_data,
        });
    }
    Ok(())
}

pub(crate) fn read_palette_chunk(
    c: &mut Cursor<&[u8]>,
    sprite: &mut Sprite,
    frame: u16,
) -> IoResult<()> {
    let size = read_u32(c)? as usize;
    let first = read_u32(c)? as usize;
    let last = read_u32(c)? as usize;
    let mut pad = [0u8; 8];
    c.read_exact(&mut pad)?;
    let mut entries: Vec<(usize, Rgba, Option<String>)> = Vec::new();
    for i in first..=last {
        let flags = read_u16(c)?;
        let r = read_u8(c)?;
        let g = read_u8(c)?;
        let b = read_u8(c)?;
        let a = read_u8(c)?;
        let name = if flags & 1 != 0 {
            Some(read_string(c)?)
        } else {
            None
        };
        entries.push((i, Rgba { r, g, b, a }, name));
    }
    let apply = |pal: &mut Vec<Rgba>, names: &mut Vec<Option<String>>| {
        if pal.len() < size {
            pal.resize(size, Rgba::TRANSPARENT);
        }
        if names.len() < pal.len() {
            names.resize(pal.len(), None);
        }
        for (i, rgba, name) in &entries {
            if *i < pal.len() {
                pal[*i] = *rgba;
            }
            if *i < names.len() {
                names[*i] = name.clone();
            }
        }
    };
    // Clear obsolete loss note once names are preserved.
    sprite
        .read_loss_notes
        .retain(|n| n != "dropped palette entry name(s); write-back will not restore names");
    if frame == 0 {
        apply(&mut sprite.palette, &mut sprite.palette_names);
    } else {
        let _ = sprite.ensure_frame_palette(frame);
        let pal = sprite.frame_palettes.get_mut(&frame).unwrap();
        let names = sprite
            .frame_palette_names
            .entry(frame)
            .or_insert_with(|| vec![None; pal.len()]);
        if names.len() < pal.len() {
            names.resize(pal.len(), None);
        }
        apply(pal, names);
    }
    Ok(())
}

pub(crate) fn read_old_palette_chunk(
    c: &mut Cursor<&[u8]>,
    sprite: &mut Sprite,
    six_bit: bool,
) -> IoResult<()> {
    let packets = read_u16(c)? as usize;
    let mut index = 0usize;
    if sprite.palette.is_empty() {
        sprite.palette = vec![Rgba::TRANSPARENT; 256];
    }
    for _ in 0..packets {
        let skip = read_u8(c)? as usize;
        index += skip;
        let mut n = read_u8(c)? as usize;
        if n == 0 {
            n = 256;
        }
        for _ in 0..n {
            let mut r = read_u8(c)?;
            let mut g = read_u8(c)?;
            let mut b = read_u8(c)?;
            if six_bit {
                r = (r as u16 * 255 / 63) as u8;
                g = (g as u16 * 255 / 63) as u8;
                b = (b as u16 * 255 / 63) as u8;
            }
            if index < sprite.palette.len() {
                sprite.palette[index] = Rgba::rgb(r, g, b);
            }
            index += 1;
        }
    }
    Ok(())
}

pub(crate) fn read_slice_chunk(c: &mut Cursor<&[u8]>, sprite: &mut Sprite) -> IoResult<()> {
    let nkeys = read_u32(c)? as usize;
    let flags = read_u32(c)?;
    let _ = read_u32(c)?;
    let name = read_string(c)?;
    let mut keys = Vec::with_capacity(nkeys);
    for _ in 0..nkeys {
        let frame = read_u32(c)?;
        let x = read_i32(c)?;
        let y = read_i32(c)?;
        let w = read_u32(c)?;
        let h = read_u32(c)?;
        let center = if flags & 1 != 0 {
            Some((read_i32(c)?, read_i32(c)?, read_u32(c)?, read_u32(c)?))
        } else {
            None
        };
        let pivot = if flags & 2 != 0 {
            Some((read_i32(c)?, read_i32(c)?))
        } else {
            None
        };
        keys.push(SliceKey {
            frame,
            x,
            y,
            w,
            h,
            center,
            pivot,
        });
    }
    sprite.slices.push(Slice {
        name,
        keys,
        user_data: UserData::default(),
    });
    Ok(())
}


pub(crate) fn read_tileset_chunk(c: &mut Cursor<&[u8]>, sprite: &mut Sprite, data_end: u64) -> IoResult<()> {
    let start = c.position();
    let id = read_u32(c)?;
    let flags = read_u32(c)?;
    let ntiles = read_u32(c)?;
    let tile_width = read_u16(c)?;
    let tile_height = read_u16(c)?;
    let base_index = {
        let mut b = [0u8; 2];
        c.read_exact(&mut b)?;
        i16::from_le_bytes(b)
    };
    let mut reserved = [0u8; 14];
    c.read_exact(&mut reserved)?;
    let name = read_string(c)?;
    let mut pixels = Vec::new();
    let mut raw_passthrough = None;
    if flags & 1 != 0 {
        // external link — keep remainder as raw for write
        let _ext_file = read_u32(c)?;
        let _ext_tileset = read_u32(c)?;
    }
    if flags & 2 != 0 {
        let data_len = read_u32(c)? as usize;
        let mut compressed = vec![0u8; data_len.min((data_end.saturating_sub(c.position())) as usize)];
        c.read_exact(&mut compressed)?;
        let bpp = sprite.color_mode.bytes_per_pixel();
        let expect = (tile_width as usize)
            .saturating_mul(tile_height as usize)
            .saturating_mul(ntiles as usize)
            .saturating_mul(bpp);
        match inflate(&compressed, expect) {
            Ok(px) => pixels = px,
            Err(_) => {
                // fall back: store whole chunk body
                c.set_position(start);
                raw_passthrough = Some(read_raw_remaining(c, data_end)?);
            }
        }
    }
    if pixels.is_empty() && raw_passthrough.is_none() && c.position() < data_end {
        c.set_position(start);
        raw_passthrough = Some(read_raw_remaining(c, data_end)?);
    }
    sprite.tilesets.push(pxl_ase_doc::Tileset {
        id,
        flags,
        ntiles,
        tile_width,
        tile_height,
        base_index,
        name,
        pixels,
        raw: raw_passthrough,
    });
    Ok(())
}

fn read_raw_remaining(c: &mut Cursor<&[u8]>, data_end: u64) -> IoResult<Vec<u8>> {
    let n = data_end.saturating_sub(c.position()) as usize;
    let mut buf = vec![0u8; n];
    c.read_exact(&mut buf)?;
    Ok(buf)
}

fn inflate(data: &[u8], expect: usize) -> IoResult<Vec<u8>> {
    let mut d = ZlibDecoder::new(data);
    let mut out = Vec::with_capacity(expect);
    d.read_to_end(&mut out)
        .map_err(|e| IoError::new(format!("zlib: {e}")))?;
    if out.len() != expect {
        return Err(IoError::new(format!(
            "zlib size {} != expected {expect}",
            out.len()
        )));
    }
    Ok(out)
}

fn read_u8(c: &mut Cursor<&[u8]>) -> IoResult<u8> {
    let mut b = [0u8; 1];
    c.read_exact(&mut b)?;
    Ok(b[0])
}
fn read_u16(c: &mut Cursor<&[u8]>) -> IoResult<u16> {
    let mut b = [0u8; 2];
    c.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}
fn read_i16(c: &mut Cursor<&[u8]>) -> IoResult<i16> {
    let mut b = [0u8; 2];
    c.read_exact(&mut b)?;
    Ok(i16::from_le_bytes(b))
}
fn read_u32(c: &mut Cursor<&[u8]>) -> IoResult<u32> {
    let mut b = [0u8; 4];
    c.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}
fn read_i32(c: &mut Cursor<&[u8]>) -> IoResult<i32> {
    let mut b = [0u8; 4];
    c.read_exact(&mut b)?;
    Ok(i32::from_le_bytes(b))
}
fn read_string(c: &mut Cursor<&[u8]>) -> IoResult<String> {
    let n = read_u16(c)? as usize;
    let mut buf = vec![0u8; n];
    c.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| IoError::new(format!("utf8: {e}")))
}
