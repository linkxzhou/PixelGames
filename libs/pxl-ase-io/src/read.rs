use std::io::{Cursor, Read};
use std::path::Path;

use flate2::read::ZlibDecoder;
use pxl_ase_doc::{
    AseDoc, BlendMode, Cel, ColorMode, GrayImage, Grid, ImageBuffer, IndexedImage, Layer,
    LayerFlags, LayerKind, RgbaImage, Slice, SliceKey, Sprite, Tag, TagDirection,
};
use pxl_core::Rgba;

use crate::error::{IoError, IoResult};

#[derive(Clone, Debug, Default)]
pub struct ReadOptions {
    /// Load only the first frame (aligned with `--oneframe`).
    pub one_frame: bool,
}

pub fn read_aseprite(path: impl AsRef<Path>, opts: &ReadOptions) -> IoResult<AseDoc> {
    let bytes = std::fs::read(path.as_ref())?;
    let mut doc = read_aseprite_bytes(&bytes, opts)?;
    doc.path = Some(path.as_ref().display().to_string());
    Ok(doc)
}

pub fn read_aseprite_bytes(bytes: &[u8], opts: &ReadOptions) -> IoResult<AseDoc> {
    if bytes.len() < 128 {
        return Err(IoError::new("file too small for ASE header"));
    }
    let mut c = Cursor::new(bytes);
    let _file_size = read_u32(&mut c)?;
    let magic = read_u16(&mut c)?;
    if magic != 0xA5E0 {
        return Err(IoError::new(format!("bad ASE magic {magic:#x}")).with_hint("expected 0xA5E0"));
    }
    let frames = read_u16(&mut c)? as usize;
    let width = read_u16(&mut c)?;
    let height = read_u16(&mut c)?;
    let bpp = read_u16(&mut c)?;
    let color_mode = ColorMode::from_bpp(bpp).ok_or_else(|| {
        IoError::new(format!("unsupported color depth {bpp}")).with_hint("need 8/16/32")
    })?;
    let _flags = read_u32(&mut c)?;
    let speed = read_u16(&mut c)?;
    let _ = read_u32(&mut c)?;
    let _ = read_u32(&mut c)?;
    let transparent_index = read_u8(&mut c)?;
    let mut ignore = [0u8; 3];
    c.read_exact(&mut ignore)?;
    let num_colors = read_u16(&mut c)?;
    let _px_w = read_u8(&mut c)?;
    let _px_h = read_u8(&mut c)?;
    let grid_x = read_i16(&mut c)?;
    let grid_y = read_i16(&mut c)?;
    let grid_w = read_u16(&mut c)?;
    let grid_h = read_u16(&mut c)?;
    let mut pad = [0u8; 84];
    c.read_exact(&mut pad)?;

    let mut sprite = Sprite::new(width, height, color_mode);
    sprite.transparent_index = transparent_index;
    sprite.grid = Grid {
        x: grid_x,
        y: grid_y,
        w: grid_w,
        h: grid_h,
    };
    if num_colors > 0 {
        sprite.palette = vec![Rgba::TRANSPARENT; num_colors as usize];
    }
    sprite.frame_durations_ms.clear();

    let frame_limit = if opts.one_frame { frames.min(1) } else { frames };
    let mut saw_new_palette = false;

    for frame_index in 0..frame_limit {
        let frame_start = c.position();
        let frame_bytes = read_u32(&mut c)? as u64;
        let frame_magic = read_u16(&mut c)?;
        if frame_magic != 0xF1FA {
            return Err(IoError::new(format!(
                "bad frame magic {frame_magic:#x} at frame {frame_index}"
            )));
        }
        let old_chunks = read_u16(&mut c)?;
        let duration = read_u16(&mut c)?;
        let _ = read_u16(&mut c)?;
        let new_chunks = read_u32(&mut c)?;
        let n_chunks = if new_chunks != 0 {
            new_chunks as usize
        } else if old_chunks == 0xFFFF {
            return Err(IoError::new("chunk count overflow without new field"));
        } else {
            old_chunks as usize
        };
        sprite.frame_durations_ms.push(if duration == 0 { speed } else { duration });

        for _ in 0..n_chunks {
            let chunk_start = c.position();
            let chunk_size = read_u32(&mut c)? as u64;
            if chunk_size < 6 {
                return Err(IoError::new("chunk size < 6"));
            }
            let chunk_type = read_u16(&mut c)?;
            let data_end = chunk_start + chunk_size;
            match chunk_type {
                0x2004 => read_layer_chunk(&mut c, &mut sprite)?,
                0x2005 => read_cel_chunk(&mut c, &mut sprite, frame_index as u16, color_mode, data_end)?,
                0x2018 => read_tags_chunk(&mut c, &mut sprite)?,
                0x2019 => {
                    read_palette_chunk(&mut c, &mut sprite)?;
                    saw_new_palette = true;
                }
                0x0004 | 0x0011 => {
                    if !saw_new_palette {
                        read_old_palette_chunk(&mut c, &mut sprite, chunk_type == 0x0011)?;
                    } else {
                        c.set_position(data_end);
                    }
                }
                0x2022 => read_slice_chunk(&mut c, &mut sprite)?,
                // skip unknowns / extras
                _ => {
                    c.set_position(data_end);
                }
            }
            if c.position() != data_end {
                // resync if a chunk reader left us short/long
                c.set_position(data_end);
            }
        }
        let expected = frame_start + frame_bytes;
        if c.position() != expected && frame_bytes > 0 {
            c.set_position(expected);
        }
    }

    if sprite.frame_durations_ms.is_empty() {
        sprite.frame_durations_ms.push(100);
    }
    Ok(AseDoc::new(sprite))
}

fn read_layer_chunk(c: &mut Cursor<&[u8]>, sprite: &mut Sprite) -> IoResult<()> {
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
    });
    Ok(())
}

fn read_cel_chunk(
    c: &mut Cursor<&[u8]>,
    sprite: &mut Sprite,
    frame: u16,
    color_mode: ColorMode,
    data_end: u64,
) -> IoResult<()> {
    let layer_index = read_u16(c)?;
    let x = read_i16(c)?;
    let y = read_i16(c)?;
    let opacity = read_u8(c)?;
    let cel_type = read_u16(c)?;
    let z_index = read_i16(c)?;
    let mut pad = [0u8; 5];
    c.read_exact(&mut pad)?;

    let layer_id = pxl_ase_doc::LayerId(layer_index);
    if !sprite.layers.iter().any(|l| l.id == layer_id) {
        c.set_position(data_end);
        return Ok(());
    }

    match cel_type {
        1 => {
            let link = read_u16(c)?;
            let layer = sprite.layers.iter_mut().find(|l| l.id == layer_id).unwrap();
            layer.cels.insert(frame, Cel::linked(link));
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
            let layer = sprite.layers.iter_mut().find(|l| l.id == layer_id).unwrap();
            layer.cels.insert(
                frame,
                Cel::Image {
                    image: image_id,
                    x,
                    y,
                    opacity,
                    z_index,
                },
            );
        }
        3 => {
            c.set_position(data_end);
        }
        other => {
            return Err(IoError::new(format!("unsupported cel type {other}")));
        }
    }
    Ok(())
}

fn read_tags_chunk(c: &mut Cursor<&[u8]>, sprite: &mut Sprite) -> IoResult<()> {
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
        sprite.tags.push(Tag {
            name,
            from,
            to,
            direction,
            repeat,
        });
    }
    Ok(())
}

fn read_palette_chunk(c: &mut Cursor<&[u8]>, sprite: &mut Sprite) -> IoResult<()> {
    let size = read_u32(c)? as usize;
    let first = read_u32(c)? as usize;
    let last = read_u32(c)? as usize;
    let mut pad = [0u8; 8];
    c.read_exact(&mut pad)?;
    if sprite.palette.len() < size {
        sprite.palette.resize(size, Rgba::TRANSPARENT);
    }
    for i in first..=last {
        let flags = read_u16(c)?;
        let r = read_u8(c)?;
        let g = read_u8(c)?;
        let b = read_u8(c)?;
        let a = read_u8(c)?;
        if flags & 1 != 0 {
            let _name = read_string(c)?;
        }
        if i < sprite.palette.len() {
            sprite.palette[i] = Rgba { r, g, b, a };
        }
    }
    Ok(())
}

fn read_old_palette_chunk(
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

fn read_slice_chunk(c: &mut Cursor<&[u8]>, sprite: &mut Sprite) -> IoResult<()> {
    let n_keys = read_u32(c)? as usize;
    let flags = read_u32(c)?;
    let _ = read_u32(c)?;
    let name = read_string(c)?;
    let mut keys = Vec::with_capacity(n_keys);
    for _ in 0..n_keys {
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
    sprite.slices.push(Slice { name, keys });
    Ok(())
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
    Ok(read_u16(c)? as i16)
}
fn read_u32(c: &mut Cursor<&[u8]>) -> IoResult<u32> {
    let mut b = [0u8; 4];
    c.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}
fn read_i32(c: &mut Cursor<&[u8]>) -> IoResult<i32> {
    Ok(read_u32(c)? as i32)
}
fn read_string(c: &mut Cursor<&[u8]>) -> IoResult<String> {
    let len = read_u16(c)? as usize;
    let mut buf = vec![0u8; len];
    c.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| IoError::new(format!("utf8: {e}")))
}
