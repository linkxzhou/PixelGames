use std::io::{Cursor, Read};
use std::path::Path;

use pxl_ase_doc::{AseDoc, ColorMode, Grid, PixelRatio, Sprite, UserData};
use pxl_core::Rgba;

use crate::chunks::{
    read_cel_chunk, read_layer_chunk, read_old_palette_chunk, read_palette_chunk, read_slice_chunk,
    read_tags_chunk, read_tileset_chunk,
};
use crate::error::{IoError, IoResult};
use crate::passthrough::{read_cel_extra, read_raw_body};
use crate::user_data::decode_user_data;

#[derive(Clone, Debug, Default)]
pub struct ReadOptions {
    /// Load only the first frame (aligned with `--oneframe`).
    pub one_frame: bool,
}

/// Last chunk that may receive a following User Data (0x2020) chunk.
#[derive(Clone, Debug)]
enum UdTarget {
    None,
    Sprite,
    Layer(usize),
    Cel { layer: usize, frame: u16 },
    /// After Tags chunk: assign user data to tags[next], advancing each time.
    TagCursor { next: usize },
    Slice(usize),
}

fn attach_user_data(sprite: &mut Sprite, target: &mut UdTarget, ud: UserData) {
    match target {
        UdTarget::None => {
            sprite
                .read_loss_notes
                .push("user data chunk with no prior object; dropped".into());
        }
        UdTarget::Sprite => sprite.user_data = ud,
        UdTarget::Layer(i) => {
            if let Some(l) = sprite.layers.get_mut(*i) {
                l.user_data = ud;
            }
        }
        UdTarget::Cel { layer, frame } => {
            if let Some(l) = sprite.layers.get_mut(*layer) {
                if let Some(cel) = l.cels.get_mut(frame) {
                    *cel.user_data_mut() = ud;
                }
            }
        }
        UdTarget::TagCursor { next } => {
            if let Some(tag) = sprite.tags.get_mut(*next) {
                if ud.color.is_some() {
                    tag.user_data.color = ud.color;
                }
                if ud.text.is_some() {
                    tag.user_data.text = ud.text;
                }
                if ud.properties_raw.is_some() {
                    tag.user_data.properties_raw = ud.properties_raw;
                }
            }
            *next += 1;
        }
        UdTarget::Slice(i) => {
            if let Some(s) = sprite.slices.get_mut(*i) {
                s.user_data = ud;
            }
        }
    }
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
    let header_flags = read_u32(&mut c)?;
    let speed = read_u16(&mut c)?;
    let _ = read_u32(&mut c)?;
    let _ = read_u32(&mut c)?;
    let transparent_index = read_u8(&mut c)?;
    let mut ignore = [0u8; 3];
    c.read_exact(&mut ignore)?;
    let num_colors = read_u16(&mut c)?;
    let px_w = read_u8(&mut c)?;
    let px_h = read_u8(&mut c)?;
    let grid_x = read_i16(&mut c)?;
    let grid_y = read_i16(&mut c)?;
    let grid_w = read_u16(&mut c)?;
    let grid_h = read_u16(&mut c)?;
    let mut pad = [0u8; 84];
    c.read_exact(&mut pad)?;

    let mut sprite = Sprite::new(width, height, color_mode);
    sprite.transparent_index = transparent_index;
    sprite.header_flags = header_flags;
    sprite.pixel_ratio = PixelRatio::new(px_w, px_h);
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
    let mut ud_target = UdTarget::None;

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
                0x2004 => {
                    read_layer_chunk(&mut c, &mut sprite)?;
                    ud_target = UdTarget::Layer(sprite.layers.len().saturating_sub(1));
                }
                0x2005 => {
                    match read_cel_chunk(
                        &mut c,
                        &mut sprite,
                        frame_index as u16,
                        color_mode,
                        data_end,
                    )? {
                        Some(layer) => {
                            ud_target = UdTarget::Cel {
                                layer,
                                frame: frame_index as u16,
                            };
                        }
                        None => ud_target = UdTarget::None,
                    }
                }
                0x2018 => {
                    read_tags_chunk(&mut c, &mut sprite)?;
                    ud_target = UdTarget::TagCursor { next: 0 };
                }
                0x2019 => {
                    read_palette_chunk(&mut c, &mut sprite, frame_index as u16)?;
                    saw_new_palette = true;
                    if frame_index == 0 {
                        ud_target = UdTarget::Sprite;
                    }
                }
                0x0004 | 0x0011 => {
                    if !saw_new_palette {
                        read_old_palette_chunk(&mut c, &mut sprite, chunk_type == 0x0011)?;
                    } else {
                        c.set_position(data_end);
                    }
                }
                0x2022 => {
                    read_slice_chunk(&mut c, &mut sprite)?;
                    ud_target = UdTarget::Slice(sprite.slices.len().saturating_sub(1));
                }
                0x2020 => {
                    let ud = decode_user_data(&mut c)?;
                    attach_user_data(&mut sprite, &mut ud_target, ud);
                }
                0x2006 => {
                    let extra = read_cel_extra(&mut c)?;
                    if let UdTarget::Cel { layer, frame } = &ud_target {
                        if let Some(l) = sprite.layers.get_mut(*layer) {
                            if let Some(cel) = l.cels.get_mut(frame) {
                                cel.set_extra(extra);
                            }
                        }
                    } else {
                        note_skip(&mut sprite, "Cel Extra chunk (0x2006) with no prior cel");
                    }
                }
                0x2007 => {
                    sprite.color_profile_raw = Some(read_raw_body(&mut c, data_end)?);
                }
                0x2008 => {
                    sprite.external_files_raw = Some(read_raw_body(&mut c, data_end)?);
                }
                0x2023 => {
                    read_tileset_chunk(&mut c, &mut sprite, data_end)?;
                }
                _ => {
                    c.set_position(data_end);
                }
            }
            if c.position() != data_end {
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

fn note_skip(sprite: &mut Sprite, what: &str) {
    let msg = format!("skipped {what}; write-back will drop it");
    if !sprite.read_loss_notes.iter().any(|n| n == &msg) {
        sprite.read_loss_notes.push(msg);
    }
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
