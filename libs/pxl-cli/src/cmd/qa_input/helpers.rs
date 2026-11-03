use std::collections::BTreeMap;

use pxl_core::{IndexFrame, Palette, PxlError};
use pxl_format::{PaletteFile, PxlDoc, SpecialFile};
use serde_json::Value;

use crate::error::{CmdError, CmdResult};


pub(crate) fn rect(value: [u32; 4]) -> CmdResult<pxl_core::Rect> {
    let [x, y, w, h] = value;
    if w == 0 || h == 0 || x.checked_add(w).is_none() || y.checked_add(h).is_none() {
        return Err(PxlError::schema(format!("invalid animation region {value:?}")).into());
    }
    Ok(pxl_core::Rect::new(x as i32, y as i32, w as i32, h as i32))
}

pub(crate) fn lookup_frames(doc: &PxlDoc, names: &[String]) -> CmdResult<Vec<IndexFrame>> {
    let mut out = Vec::new();
    for name in names {
        out.push(doc.frame(name)?);
    }
    Ok(out)
}

pub(crate) fn pxl_field<T: serde::de::DeserializeOwned + Default>(
    pxl: &Value,
    key: &str,
) -> CmdResult<T> {
    match pxl.get(key) {
        None | Some(Value::Null) => Ok(T::default()),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| {
            PxlError::schema(format!("sheet pxl.{key} is malformed: {e}"))
                .with_path(format!("/meta/pxl/{key}"))
                .into()
        }),
    }
}

pub(crate) fn palette_from_pxl(pxl: &Value) -> CmdResult<Palette> {
    let colors = pxl
        .get("colors")
        .and_then(|v| v.as_array())
        .ok_or_else(|| PxlError::schema("sheet pxl.colors is missing or not an array"))?;
    let colors = colors
        .iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_str().map(|s| s.to_string()).ok_or_else(|| {
                CmdError::from(PxlError::schema(format!(
                    "sheet pxl.colors[{i}] must be a string, got {v}"
                )))
            })
        })
        .collect::<CmdResult<Vec<_>>>()?;
    let special: SpecialFile = pxl_field(pxl, "special")?;
    let file = PaletteFile {
        v: 1,
        id: "sheet".into(),
        colors,
        ramps: pxl_field(pxl, "ramps")?,
        cycles: pxl_field(pxl, "cycles")?,
        special,
    };
    Ok(file.into_palette("sheet")?)
}

/// The palette the metadata declares and the one the PNG carries must show the
/// same colours; otherwise validation would judge an image nobody can see.
pub(crate) fn reconcile_palette(declared: &Palette, png: &[pxl_core::Rgba]) -> CmdResult<()> {
    if png.len() < declared.colors.len() {
        return Err(PxlError::semantic(format!(
            "sheet png palette has {} colours but the metadata declares {}",
            png.len(),
            declared.colors.len()
        ))
        .with_hint("rebuild the sheet; the png and its json must come from the same build")
        .into());
    }
    for (i, want) in declared.colors.iter().enumerate() {
        let got = png[i];
        let same_alpha = (want.a == 0) == (got.a == 0);
        let same_rgb = want.a == 0 || (want.r, want.g, want.b) == (got.r, got.g, got.b);
        if !same_alpha || !same_rgb {
            return Err(PxlError::semantic(format!(
                "sheet png palette entry {i} is {got:?} but the metadata declares {want:?}"
            ))
            .with_hint("the png palette and meta.pxl.colors must describe the same colours")
            .into());
        }
    }
    Ok(())
}

pub(crate) fn palette_from_colors(colors: &[pxl_core::Rgba]) -> CmdResult<Palette> {
    Ok(Palette {
        id: "png".into(),
        colors: colors.to_vec(),
        ramps: BTreeMap::new(),
        cycles: BTreeMap::new(),
        special: pxl_core::Special::default(),
    })
}

pub(crate) fn crop_indexed(
    pixels: &[u8],
    sheet_w: u32,
    sheet_h: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> CmdResult<IndexFrame> {
    let end_x = x.checked_add(w);
    let end_y = y.checked_add(h);
    if w == 0
        || h == 0
        || end_x.map_or(true, |e| e > sheet_w)
        || end_y.map_or(true, |e| e > sheet_h)
    {
        return Err(PxlError::semantic(format!(
            "frame rect x={x} y={y} w={w} h={h} does not fit the {sheet_w}x{sheet_h} sheet image"
        ))
        .with_hint("every frame rect must be non-empty and lie inside the png")
        .into());
    }
    let expected = sheet_w as usize * sheet_h as usize;
    if pixels.len() != expected {
        return Err(PxlError::semantic(format!(
            "sheet png holds {} pixels, expected {expected} for {sheet_w}x{sheet_h}",
            pixels.len()
        ))
        .into());
    }
    let mut frame = IndexFrame::canvas(
        u16::try_from(w)
            .map_err(|_| PxlError::semantic(format!("frame width {w} is too large")))?,
        u16::try_from(h)
            .map_err(|_| PxlError::semantic(format!("frame height {h} is too large")))?,
    )?;
    for row in 0..h {
        for col in 0..w {
            let src = ((y + row) as usize) * sheet_w as usize + (x + col) as usize;
            frame.set(col as i32, row as i32, pixels[src]);
        }
    }
    Ok(frame)
}

/// A non-negative integer that fits `u32`. Strings, fractions, negatives and
/// values that would wrap are all errors that name the field.
pub(crate) fn json_u32(v: &Value, key: &str) -> CmdResult<u32> {
    let field = v
        .get(key)
        .ok_or_else(|| PxlError::schema(format!("missing integer '{key}'")))?;
    let n = field.as_u64().ok_or_else(|| {
        PxlError::schema(format!(
            "'{key}' must be a non-negative integer, got {field}"
        ))
    })?;
    u32::try_from(n)
        .map_err(|_| PxlError::semantic(format!("'{key}' = {n} does not fit in 32 bits")).into())
}

pub(crate) fn string_list(v: &Value) -> CmdResult<Vec<String>> {
    let Some(arr) = v.as_array() else {
        return Ok(Vec::new());
    };
    Ok(arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect())
}
