use crate::{IndexFrame, PxlError, PxlResult};
use alloc::string::String;
use alloc::vec::Vec;

pub fn encode_index(i: u8) -> PxlResult<char> {
    match i {
        0 => Ok('.'),
        1..=9 => Ok((b'0' + i) as char),
        10..=35 => Ok((b'a' + (i - 10)) as char),
        _ => Err(PxlError::semantic(alloc::format!(
            "index {i} cannot be written as text (max 35)"
        ))
        .with_hint("palettes used with the text matrix are limited to 36 entries")),
    }
}

pub fn decode_index(c: char) -> PxlResult<u8> {
    match c {
        '.' => Ok(0),
        '0'..='9' => Ok(c as u8 - b'0'),
        'a'..='z' => Ok(c as u8 - b'a' + 10),
        'A'..='Z' => Ok(c as u8 - b'A' + 10),
        _ => Err(PxlError::schema(alloc::format!("bad matrix char '{c}'"))
            .with_hint("use '.' or 0-9a-z for palette indices")),
    }
}

pub fn encode_matrix(frame: &IndexFrame) -> PxlResult<Vec<String>> {
    let mut rows = Vec::with_capacity(frame.h as usize);
    for y in 0..frame.h as usize {
        let mut s = String::with_capacity(frame.w as usize);
        for x in 0..frame.w as usize {
            s.push(encode_index(frame.pixels[y * frame.w as usize + x])?);
        }
        rows.push(s);
    }
    Ok(rows)
}

pub fn decode_matrix(rows: &[String]) -> PxlResult<IndexFrame> {
    if rows.is_empty() {
        return Err(
            PxlError::schema("frame has no rows").with_hint("a frame needs at least one row")
        );
    }
    let w = rows[0].chars().count();
    if w == 0 || w > crate::MAX_DIM as usize || rows.len() > crate::MAX_DIM as usize {
        return Err(PxlError::schema(alloc::format!(
            "matrix {}x{} is outside 1..=64",
            w,
            rows.len()
        )));
    }
    let mut frame = IndexFrame::new(w as u16, rows.len() as u16)?;
    for (y, row) in rows.iter().enumerate() {
        let chars: Vec<char> = row.chars().collect();
        if chars.len() != w {
            return Err(PxlError::schema(alloc::format!(
                "row {y} has width {}, expected {w}",
                chars.len()
            ))
            .with_path(alloc::format!("/frames/{y}")));
        }
        for (x, c) in chars.into_iter().enumerate() {
            frame.pixels[y * w + x] = decode_index(c)?;
        }
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_text() {
        let mut f = IndexFrame::new(4, 2).unwrap();
        f.set(1, 0, 10);
        f.set(2, 1, 2);
        let rows = encode_matrix(&f).unwrap();
        assert_eq!(
            rows,
            alloc::vec![
                alloc::string::String::from(".a.."),
                alloc::string::String::from("..2.")
            ]
        );
        let back = decode_matrix(&rows).unwrap();
        assert_eq!(back.pixels, f.pixels);
    }
}
