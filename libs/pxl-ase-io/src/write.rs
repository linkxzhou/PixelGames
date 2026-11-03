use std::io::Write;
use std::path::Path;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, LayerKind, Sprite, TagDirection, UserData};

use crate::error::{IoError, IoResult};
use crate::passthrough::encode_cel_extra;
use crate::user_data::encode_user_data_body;

#[derive(Clone, Debug)]
pub struct WriteOptions {
    pub compression: Compression,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            compression: Compression::fast(),
        }
    }
}

pub fn write_aseprite(path: impl AsRef<Path>, sprite: &Sprite, opts: &WriteOptions) -> IoResult<()> {
    let bytes = write_aseprite_bytes(sprite, opts)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn write_aseprite_bytes(sprite: &Sprite, opts: &WriteOptions) -> IoResult<Vec<u8>> {
    emit_write_loss_warnings(sprite);
    let mut frames_blob = Vec::new();
    let frame_count = sprite.frame_count().max(1);

    for fi in 0..frame_count {
        let mut chunks: Vec<Vec<u8>> = Vec::new();
        if fi == 0 {
            for layer in &sprite.layers {
                chunks.push(encode_layer_chunk(layer, sprite.header_flags));
                if !layer.user_data.is_empty() {
                    chunks.push(wrap_chunk(0x2020, &encode_user_data_body(&layer.user_data)));
                }
            }
            if !sprite.palette.is_empty() || sprite.color_mode == ColorMode::Indexed {
                chunks.push(encode_palette_chunk(sprite));
            }
            if let Some(raw) = &sprite.color_profile_raw {
                chunks.push(wrap_chunk(0x2007, raw));
            }
            if let Some(raw) = &sprite.external_files_raw {
                chunks.push(wrap_chunk(0x2008, raw));
            }
            if !sprite.user_data.is_empty() {
                chunks.push(wrap_chunk(0x2020, &encode_user_data_body(&sprite.user_data)));
            }
            if !sprite.tags.is_empty() {
                chunks.push(encode_tags_chunk(sprite));
                for tag in &sprite.tags {
                    if !tag.user_data.is_empty() {
                        chunks.push(wrap_chunk(0x2020, &encode_user_data_body(&tag.user_data)));
                    }
                }
            }
            for slice in &sprite.slices {
                chunks.push(encode_slice_chunk(slice));
                if !slice.user_data.is_empty() {
                    chunks.push(wrap_chunk(0x2020, &encode_user_data_body(&slice.user_data)));
                }
            }
            for ts in &sprite.tilesets {
                chunks.push(encode_tileset_chunk(ts, sprite, opts)?);
            }
        }
        for layer in &sprite.layers {
            if let Some(cel) = layer.cels.get(&(fi as u16)) {
                chunks.push(encode_cel_chunk(layer.id.0, cel, sprite, opts)?);
                if let Some(extra) = cel.extra() {
                    chunks.push(wrap_chunk(0x2006, &encode_cel_extra(extra)));
                }
                if !cel.user_data().is_empty() {
                    chunks.push(wrap_chunk(0x2020, &encode_user_data_body(cel.user_data())));
                }
            }
        }
        let duration = sprite
            .frame_durations_ms
            .get(fi)
            .copied()
            .unwrap_or(100);
        frames_blob.extend(encode_frame(&chunks, duration));
    }

    let mut out = Vec::with_capacity(128 + frames_blob.len());
    // placeholder size
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0xA5E0u16.to_le_bytes());
    out.extend_from_slice(&(frame_count as u16).to_le_bytes());
    out.extend_from_slice(&sprite.width.to_le_bytes());
    out.extend_from_slice(&sprite.height.to_le_bytes());
    out.extend_from_slice(&sprite.color_mode.bits_per_pixel().to_le_bytes());
    out.extend_from_slice(&sprite.header_flags.to_le_bytes());
    out.extend_from_slice(&100u16.to_le_bytes()); // deprecated speed
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(sprite.transparent_index);
    out.extend_from_slice(&[0, 0, 0]);
    let ncolors = if sprite.palette.is_empty() {
        0u16
    } else {
        sprite.palette.len().min(65535) as u16
    };
    out.extend_from_slice(&ncolors.to_le_bytes());
    let (px_w, px_h) = sprite.pixel_ratio.effective();
    out.push(px_w);
    out.push(px_h);
    out.extend_from_slice(&sprite.grid.x.to_le_bytes());
    out.extend_from_slice(&sprite.grid.y.to_le_bytes());
    out.extend_from_slice(&sprite.grid.w.to_le_bytes());
    out.extend_from_slice(&sprite.grid.h.to_le_bytes());
    out.extend_from_slice(&[0u8; 84]);
    out.extend_from_slice(&frames_blob);
    let size = out.len() as u32;
    out[0..4].copy_from_slice(&size.to_le_bytes());
    Ok(out)
}

fn encode_frame(chunks: &[Vec<u8>], duration: u16) -> Vec<u8> {
    let chunks_size: usize = chunks.iter().map(|c| c.len()).sum();
    let frame_size = 16 + chunks_size;
    let mut out = Vec::with_capacity(frame_size);
    out.extend_from_slice(&(frame_size as u32).to_le_bytes());
    out.extend_from_slice(&0xF1FAu16.to_le_bytes());
    let n = chunks.len() as u32;
    let old = if n >= 0xFFFF { 0xFFFFu16 } else { n as u16 };
    out.extend_from_slice(&old.to_le_bytes());
    out.extend_from_slice(&duration.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    for c in chunks {
        out.extend_from_slice(c);
    }
    out
}

fn encode_layer_chunk(layer: &pxl_ase_doc::Layer, header_flags: u32) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&layer.flags.to_bits().to_le_bytes());
    body.extend_from_slice(&layer.kind.to_u16().to_le_bytes());
    body.extend_from_slice(&layer.child_level.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&(layer.blend as u16).to_le_bytes());
    body.push(layer.opacity);
    body.extend_from_slice(&[0, 0, 0]);
    write_string(&mut body, &layer.name);
    if layer.kind == LayerKind::Tilemap {
        body.extend_from_slice(&layer.tileset_index.unwrap_or(0).to_le_bytes());
    }
    if header_flags & 4 != 0 {
        body.extend_from_slice(&layer.uuid.unwrap_or([0u8; 16]));
    }
    wrap_chunk(0x2004, &body)
}

fn encode_palette_chunk(sprite: &Sprite) -> Vec<u8> {
    let colors = if sprite.palette.is_empty() {
        vec![pxl_core::Rgba::TRANSPARENT]
    } else {
        sprite.palette.clone()
    };
    let mut body = Vec::new();
    body.extend_from_slice(&(colors.len() as u32).to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&((colors.len() as u32).saturating_sub(1)).to_le_bytes());
    body.extend_from_slice(&[0u8; 8]);
    for c in &colors {
        body.extend_from_slice(&0u16.to_le_bytes()); // flags
        body.push(c.r);
        body.push(c.g);
        body.push(c.b);
        body.push(c.a);
    }
    wrap_chunk(0x2019, &body)
}

fn encode_tags_chunk(sprite: &Sprite) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&(sprite.tags.len() as u16).to_le_bytes());
    body.extend_from_slice(&[0u8; 8]);
    for t in &sprite.tags {
        body.extend_from_slice(&t.from.to_le_bytes());
        body.extend_from_slice(&t.to.to_le_bytes());
        body.push(t.direction.to_u8());
        body.extend_from_slice(&t.repeat.to_le_bytes());
        body.extend_from_slice(&[0u8; 6]);
        let (r, g, b) = t
            .user_data
            .color
            .map(|c| (c.r, c.g, c.b))
            .unwrap_or((0, 0, 0));
        body.extend_from_slice(&[r, g, b]); // deprecated rgb (compat); canonical color in following UD
        body.push(0);
        write_string(&mut body, &t.name);
        let _ = TagDirection::Forward;
    }
    wrap_chunk(0x2018, &body)
}

fn encode_slice_chunk(slice: &pxl_ase_doc::Slice) -> Vec<u8> {
    let mut flags = 0u32;
    if slice.keys.iter().any(|k| k.center.is_some()) {
        flags |= 1;
    }
    if slice.keys.iter().any(|k| k.pivot.is_some()) {
        flags |= 2;
    }
    let mut body = Vec::new();
    body.extend_from_slice(&(slice.keys.len() as u32).to_le_bytes());
    body.extend_from_slice(&flags.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    write_string(&mut body, &slice.name);
    for k in &slice.keys {
        body.extend_from_slice(&k.frame.to_le_bytes());
        body.extend_from_slice(&k.x.to_le_bytes());
        body.extend_from_slice(&k.y.to_le_bytes());
        body.extend_from_slice(&k.w.to_le_bytes());
        body.extend_from_slice(&k.h.to_le_bytes());
        if flags & 1 != 0 {
            let (cx, cy, cw, ch) = k.center.unwrap_or((0, 0, 0, 0));
            body.extend_from_slice(&cx.to_le_bytes());
            body.extend_from_slice(&cy.to_le_bytes());
            body.extend_from_slice(&cw.to_le_bytes());
            body.extend_from_slice(&ch.to_le_bytes());
        }
        if flags & 2 != 0 {
            let (px, py) = k.pivot.unwrap_or((0, 0));
            body.extend_from_slice(&px.to_le_bytes());
            body.extend_from_slice(&py.to_le_bytes());
        }
    }
    wrap_chunk(0x2022, &body)
}

fn encode_cel_chunk(
    layer_index: u16,
    cel: &Cel,
    sprite: &Sprite,
    opts: &WriteOptions,
) -> IoResult<Vec<u8>> {
    let mut body = Vec::new();
    body.extend_from_slice(&layer_index.to_le_bytes());
    match cel {
        Cel::Linked { frame, z_index, .. } => {
            body.extend_from_slice(&0i16.to_le_bytes());
            body.extend_from_slice(&0i16.to_le_bytes());
            body.push(255);
            body.extend_from_slice(&1u16.to_le_bytes()); // linked
            body.extend_from_slice(&z_index.to_le_bytes());
            body.extend_from_slice(&[0u8; 5]);
            body.extend_from_slice(&frame.to_le_bytes());
        }
        Cel::Image {
            image,
            x,
            y,
            opacity,
            z_index, .. } => {
            body.extend_from_slice(&x.to_le_bytes());
            body.extend_from_slice(&y.to_le_bytes());
            body.push(*opacity);
            body.extend_from_slice(&2u16.to_le_bytes()); // compressed
            body.extend_from_slice(&z_index.to_le_bytes());
            body.extend_from_slice(&[0u8; 5]);
            let buf = sprite
                .images
                .get(image)
                .ok_or_else(|| IoError::new(format!("missing image {}", image.0)))?;
            let (w, h, pixels) = match buf {
                ImageBuffer::Rgba(i) => (i.width, i.height, i.pixels.as_ref().clone()),
                ImageBuffer::Grayscale(i) => (i.width, i.height, i.pixels.as_ref().clone()),
                ImageBuffer::Indexed(i) => (i.width, i.height, i.pixels.as_ref().clone()),
            };
            body.extend_from_slice(&w.to_le_bytes());
            body.extend_from_slice(&h.to_le_bytes());
            body.extend(deflate(&pixels, opts.compression)?);
        }
        Cel::Tilemap {
            x,
            y,
            opacity,
            z_index,
            w,
            h,
            tiles,
            ..
        } => {
            body.extend_from_slice(&x.to_le_bytes());
            body.extend_from_slice(&y.to_le_bytes());
            body.push(*opacity);
            body.extend_from_slice(&3u16.to_le_bytes()); // tilemap
            body.extend_from_slice(&z_index.to_le_bytes());
            body.extend_from_slice(&[0u8; 5]);
            body.extend_from_slice(&w.to_le_bytes());
            body.extend_from_slice(&h.to_le_bytes());
            let mut raw = Vec::with_capacity(tiles.len() * 4);
            for t in tiles {
                raw.extend_from_slice(&t.to_le_bytes());
            }
            let compressed = deflate(&raw, opts.compression)?;
            body.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            body.extend_from_slice(&compressed);
        }
    }
    Ok(wrap_chunk(0x2005, &body))
}


fn encode_tileset_chunk(
    ts: &pxl_ase_doc::Tileset,
    sprite: &Sprite,
    opts: &WriteOptions,
) -> IoResult<Vec<u8>> {
    if let Some(raw) = &ts.raw {
        return Ok(wrap_chunk(0x2023, raw));
    }
    let mut body = Vec::new();
    body.extend_from_slice(&ts.id.to_le_bytes());
    body.extend_from_slice(&ts.flags.to_le_bytes());
    body.extend_from_slice(&ts.ntiles.to_le_bytes());
    body.extend_from_slice(&ts.tile_width.to_le_bytes());
    body.extend_from_slice(&ts.tile_height.to_le_bytes());
    body.extend_from_slice(&ts.base_index.to_le_bytes());
    body.extend_from_slice(&[0u8; 14]);
    write_string(&mut body, &ts.name);
    if ts.flags & 2 != 0 && !ts.pixels.is_empty() {
        let compressed = deflate(&ts.pixels, opts.compression)?;
        body.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        body.extend_from_slice(&compressed);
    }
    let _ = sprite.color_mode;
    Ok(wrap_chunk(0x2023, &body))
}

fn deflate(data: &[u8], level: Compression) -> IoResult<Vec<u8>> {
    let mut enc = ZlibEncoder::new(Vec::new(), level);
    enc.write_all(data)
        .map_err(|e| IoError::new(format!("zlib write: {e}")))?;
    enc.finish()
        .map_err(|e| IoError::new(format!("zlib finish: {e}")))
}

fn wrap_chunk(ty: u16, body: &[u8]) -> Vec<u8> {
    let size = 6 + body.len();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&ty.to_le_bytes());
    out.extend_from_slice(body);
    out
}

fn write_string(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    out.extend_from_slice(&(b.len() as u16).to_le_bytes());
    out.extend_from_slice(b);
}

fn emit_write_loss_warnings(sprite: &Sprite) {
    for note in &sprite.read_loss_notes {
        eprintln!("pxl-ase-io write warning: {note}");
    }
    let _ = UserData::default();
}
