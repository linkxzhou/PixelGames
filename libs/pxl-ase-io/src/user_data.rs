//! User Data chunk (0x2020) encode/decode — ase-file-specs.

use std::io::{Cursor, Read};

use pxl_ase_doc::UserData;
use pxl_core::Rgba;

use crate::error::{IoError, IoResult};

pub fn decode_user_data(c: &mut Cursor<&[u8]>) -> IoResult<UserData> {
    let flags = read_u32(c)?;
    let mut ud = UserData::default();
    if flags & 1 != 0 {
        ud.text = Some(read_string(c)?);
    }
    if flags & 2 != 0 {
        ud.color = Some(Rgba {
            r: read_u8(c)?,
            g: read_u8(c)?,
            b: read_u8(c)?,
            a: read_u8(c)?,
        });
    }
    if flags & 4 != 0 {
        // Spec: size includes this DWORD and the number-of-maps DWORD (>= 8).
        let size = read_u32(c)? as usize;
        if size < 8 {
            return Err(IoError::new("user data properties size < 8"));
        }
        let rest = size.saturating_sub(4);
        let mut raw = Vec::with_capacity(size);
        raw.extend_from_slice(&(size as u32).to_le_bytes());
        let mut buf = vec![0u8; rest];
        c.read_exact(&mut buf)?;
        raw.extend_from_slice(&buf);
        ud.properties_raw = Some(raw);
    }
    Ok(ud)
}

pub fn encode_user_data_body(ud: &UserData) -> Vec<u8> {
    let mut flags = 0u32;
    if ud.text.is_some() {
        flags |= 1;
    }
    if ud.color.is_some() {
        flags |= 2;
    }
    if ud.properties_raw.is_some() {
        flags |= 4;
    }
    let mut body = Vec::new();
    body.extend_from_slice(&flags.to_le_bytes());
    if let Some(text) = &ud.text {
        let b = text.as_bytes();
        body.extend_from_slice(&(b.len() as u16).to_le_bytes());
        body.extend_from_slice(b);
    }
    if let Some(c) = &ud.color {
        body.push(c.r);
        body.push(c.g);
        body.push(c.b);
        body.push(c.a);
    }
    if let Some(raw) = &ud.properties_raw {
        body.extend_from_slice(raw);
    }
    body
}

fn read_u8(c: &mut Cursor<&[u8]>) -> IoResult<u8> {
    let mut b = [0u8; 1];
    c.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_u32(c: &mut Cursor<&[u8]>) -> IoResult<u32> {
    let mut b = [0u8; 4];
    c.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u16(c: &mut Cursor<&[u8]>) -> IoResult<u16> {
    let mut b = [0u8; 2];
    c.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

fn read_string(c: &mut Cursor<&[u8]>) -> IoResult<String> {
    let n = read_u16(c)? as usize;
    let mut buf = vec![0u8; n];
    c.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| IoError::new(format!("utf8: {e}")))
}
