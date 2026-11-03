//! Color Profile (0x2007) and Cel Extra (0x2006) passthrough helpers.

use std::io::{Cursor, Read};

use pxl_ase_doc::CelExtra;

use crate::error::{IoError, IoResult};

pub fn read_cel_extra(c: &mut Cursor<&[u8]>) -> IoResult<CelExtra> {
    let flags = read_u32(c)?;
    let precise_x = read_i32(c)?;
    let precise_y = read_i32(c)?;
    let width = read_i32(c)?;
    let height = read_i32(c)?;
    let mut pad = [0u8; 16];
    c.read_exact(&mut pad)?;
    Ok(CelExtra {
        flags,
        precise_x,
        precise_y,
        width,
        height,
    })
}

pub fn encode_cel_extra(extra: &CelExtra) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + 16 + 16);
    body.extend_from_slice(&extra.flags.to_le_bytes());
    body.extend_from_slice(&extra.precise_x.to_le_bytes());
    body.extend_from_slice(&extra.precise_y.to_le_bytes());
    body.extend_from_slice(&extra.width.to_le_bytes());
    body.extend_from_slice(&extra.height.to_le_bytes());
    body.extend_from_slice(&[0u8; 16]);
    body
}

/// Read remaining chunk body as opaque bytes (cursor at start of body, through data_end).
pub fn read_raw_body(c: &mut Cursor<&[u8]>, data_end: u64) -> IoResult<Vec<u8>> {
    let n = data_end.saturating_sub(c.position()) as usize;
    let mut buf = vec![0u8; n];
    c.read_exact(&mut buf)?;
    Ok(buf)
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

#[allow(dead_code)]
fn _io_err(msg: impl Into<String>) -> IoError {
    IoError::new(msg)
}
