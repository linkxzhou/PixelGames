use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use pxl_core::{IndexFrame, Palette, PxlError};
use pxl_export::decode_indexed;
use pxl_format::{PaletteFile, PxlDoc, SpecialFile};
use pxl_qa::{AnimSeq, ValidateInput};
use serde_json::{json, Value};

use super::sheet::FACING_NAMES;
use crate::error::{CmdError, CmdResult};
use crate::util::{self};

pub(super) fn doc_input(doc: &PxlDoc, palette: &Palette) -> CmdResult<ValidateInput> {
    let frames = util::frames_of(doc)?;
    let mut anims = Vec::new();
    for (name, anim) in &doc.anims {
        let dirs = anim.dir_list();
        let static_mode = anim.mode == pxl_format::AnimMode::Static;
        if dirs.is_empty() {
            let names = anim.frames.sequence(None)?;
            anims.push(AnimSeq {
                name: name.clone(),
                asset: doc.meta.asset.clone().unwrap_or_default(),
                base_name: name.clone(),
                dir: None,
                looped: anim.looped,
                frames: lookup_frames(doc, &names)?,
                frame_names: names,
                static_mode,
            });
        } else {
            for dir in dirs {
                let names = anim.frames.sequence(Some(&dir))?;
                anims.push(AnimSeq {
                    name: name.clone(),
                    asset: doc.meta.asset.clone().unwrap_or_default(),
                    base_name: name.clone(),
                    dir: Some(dir.clone()),
                    looped: anim.looped,
                    frames: lookup_frames(doc, &names)?,
                    frame_names: names,
                    static_mode,
                });
            }
        }
    }
    let mut facings = BTreeMap::new();
    for d in FACING_NAMES {
        let idle = format!("idle_{d}_0");
        if doc.frames.contains_key(&idle) {
            facings.insert((*d).to_string(), doc.frame(&idle)?);
        } else if doc.frames.contains_key(*d) {
            facings.insert((*d).to_string(), doc.frame(d)?);
        }
    }
    let tiles = if doc.meta.kind.as_deref() == Some("tiles") {
        frames.clone()
    } else {
        Vec::new()
    };
    let tile_ramp = doc.meta.tile_ramp.clone().unwrap_or_else(|| {
        if palette.ramps.contains_key("ground") {
            "ground".into()
        } else {
            palette
                .ramps
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| "ground".into())
        }
    });
    let tile_ramps = tiles
        .iter()
        .map(|(name, _)| (name.clone(), tile_ramp.clone()))
        .collect();
    let asset = doc.meta.asset.clone().unwrap_or_default();
    let mut asset_kinds = BTreeMap::new();
    if let Some(kind) = &doc.meta.kind {
        asset_kinds.insert(asset.clone(), kind.clone());
    }
    let frame_assets = frames
        .iter()
        .map(|(name, _)| (name.clone(), asset.clone()))
        .collect();
    Ok(ValidateInput {
        palette: palette.clone(),
        frames,
        anims,
        facings,
        tiles,
        tile_ramp,
        tile_ramps,
        asset_kinds,
        frame_assets,
    })
}

pub(super) fn lookup_frames(doc: &PxlDoc, names: &[String]) -> CmdResult<Vec<IndexFrame>> {
    let mut out = Vec::new();
    for name in names {
        out.push(doc.frame(name)?);
    }
    Ok(out)
}

pub(super) fn sheet_input(json_path: &Path, value: &Value) -> CmdResult<ValidateInput> {
    let meta = value
        .get("meta")
        .ok_or_else(|| PxlError::schema("sheet json has no meta"))?;
    let image = meta
        .get("image")
        .and_then(|v| v.as_str())
        .unwrap_or("sheet.png");
    let png_path = json_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(image);
    let bytes = fs::read(&png_path).map_err(|e| {
        PxlError::io(format!(
            "cannot read sheet image {}: {e}",
            png_path.display()
        ))
        .with_hint("the png sits next to the sheet json; meta.image is the file name")
    })?;
    let (w, h, pixels, png_colors) = decode_indexed(&bytes)?;
    let pxl = meta.get("pxl").cloned().unwrap_or_else(|| json!({}));
    // A sheet with no `pxl` block is a plain Aseprite export: its palette is the
    // PNG's. When the block exists it is authoritative and must be well formed
    // and agree with the PNG; a broken block is an error, never a fallback.
    let palette = if pxl.get("colors").is_some() {
        let declared = palette_from_pxl(&pxl)?;
        reconcile_palette(&declared, &png_colors)?;
        declared
    } else {
        palette_from_colors(&png_colors)?
    };
    let frames_obj = value
        .get("frames")
        .and_then(|v| v.as_object())
        .ok_or_else(|| PxlError::schema("sheet json has no frames object"))?;
    let mut named = BTreeMap::new();
    let mut ordered = Vec::new();
    for (name, entry) in frames_obj {
        let fr = entry
            .get("frame")
            .ok_or_else(|| PxlError::schema(format!("frame '{name}' has no frame rect")))?;
        let x = json_u32(fr, "x")?;
        let y = json_u32(fr, "y")?;
        let fw = json_u32(fr, "w")?;
        let fh = json_u32(fr, "h")?;
        let frame = crop_indexed(&pixels, w, h, x, y, fw, fh)?;
        ordered.push((name.clone(), frame.clone()));
        named.insert(name.clone(), frame);
    }
    let mut anims = Vec::new();
    if let Some(obj) = pxl.get("anims").and_then(|v| v.as_object()) {
        for (name, anim) in obj {
            let looped = anim.get("loop").and_then(|v| v.as_bool()).unwrap_or(true);
            // Project rules match the animation's own name and owner, not the
            // display key, which is prefixed once a name is reused. Older sheets
            // without these fields fall back to the display key.
            let asset = anim
                .get("asset")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let base_name = anim
                .get("base")
                .and_then(|v| v.as_str())
                .unwrap_or(name.as_str())
                .to_string();
            // Static intent travels with the sheet so a standalone `pxl
            // validate` agrees with what `pxl build` did.
            let static_mode = anim
                .get("mode")
                .and_then(|v| v.as_str())
                .map(|m| m == "static")
                .unwrap_or(false);
            let dirs = anim.get("dirs").and_then(|v| v.as_object());
            if dirs.map(|d| d.is_empty()).unwrap_or(true) {
                return Err(PxlError::semantic(format!(
                    "sheet animation '{name}' lists no directions or frames"
                ))
                .with_path(format!("/meta/pxl/anims/{name}/dirs"))
                .with_hint("rebuild the sheet; every animation needs at least one row of frames")
                .into());
            }
            if let Some(dirs) = dirs {
                for (dir, list) in dirs {
                    let names = string_list(list)?;
                    let frames = names
                        .iter()
                        .map(|n| {
                            named.get(n).cloned().ok_or_else(|| {
                                CmdError::from(PxlError::semantic(format!(
                                    "sheet is missing frame '{n}'"
                                )))
                            })
                        })
                        .collect::<CmdResult<Vec<_>>>()?;
                    anims.push(AnimSeq {
                        name: name.clone(),
                        asset: asset.clone(),
                        base_name: base_name.clone(),
                        dir: if dir == "_" { None } else { Some(dir.clone()) },
                        looped,
                        frames,
                        frame_names: names,
                        static_mode,
                    });
                }
            }
        }
    }
    let mut facings = BTreeMap::new();
    if let Some(obj) = pxl.get("facings").and_then(|v| v.as_object()) {
        for (dir, name) in obj {
            if let Some(n) = name.as_str() {
                if let Some(frame) = named.get(n) {
                    facings.insert(dir.clone(), frame.clone());
                }
            }
        }
    }
    let mut tiles = Vec::new();
    let mut tile_ramp = if palette.ramps.contains_key("ground") {
        "ground".to_string()
    } else {
        palette
            .ramps
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "ground".into())
    };
    let mut tile_ramps = BTreeMap::new();
    if let Some(obj) = pxl.get("tiles").and_then(|v| v.as_object()) {
        for (_id, tile) in obj {
            let ramp = tile
                .get("ramp")
                .and_then(|v| v.as_str())
                .unwrap_or(&tile_ramp)
                .to_string();
            if palette.ramps.contains_key(&ramp) {
                tile_ramp = ramp.clone();
            }
            for name in string_list(tile.get("frames").unwrap_or(&Value::Null)).unwrap_or_default()
            {
                if let Some(frame) = named.get(&name) {
                    tile_ramps.insert(name.clone(), ramp.clone());
                    tiles.push((name, frame.clone()));
                }
            }
        }
    }
    // Declared use per asset and the frames it owns. Older sheets without the
    // block judge every frame as a sprite, as before.
    let mut asset_kinds = BTreeMap::new();
    let mut frame_assets = BTreeMap::new();
    if let Some(obj) = pxl.get("assets").and_then(|v| v.as_object()) {
        for (id, entry) in obj {
            let kind = entry
                .get("kind")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    PxlError::schema(format!("sheet asset '{id}' has no kind"))
                        .with_path(format!("/meta/pxl/assets/{id}/kind"))
                })?;
            asset_kinds.insert(id.clone(), kind.to_string());
            for name in string_list(entry.get("frames").unwrap_or(&Value::Null))? {
                if !named.contains_key(&name) {
                    return Err(PxlError::semantic(format!(
                        "sheet asset '{id}' lists missing frame '{name}'"
                    ))
                    .with_path(format!("/meta/pxl/assets/{id}/frames"))
                    .into());
                }
                frame_assets.insert(name, id.clone());
            }
        }
    }
    Ok(ValidateInput {
        palette,
        frames: ordered,
        anims,
        facings,
        tiles,
        tile_ramp,
        tile_ramps,
        asset_kinds,
        frame_assets,
    })
}

/// Parse an optional `pxl.<key>` object. Absent is the default; present but
/// malformed is an error that names the field.
pub(super) fn pxl_field<T: serde::de::DeserializeOwned + Default>(
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

pub(super) fn palette_from_pxl(pxl: &Value) -> CmdResult<Palette> {
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
pub(super) fn reconcile_palette(declared: &Palette, png: &[pxl_core::Rgba]) -> CmdResult<()> {
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

pub(super) fn palette_from_colors(colors: &[pxl_core::Rgba]) -> CmdResult<Palette> {
    Ok(Palette {
        id: "png".into(),
        colors: colors.to_vec(),
        ramps: BTreeMap::new(),
        cycles: BTreeMap::new(),
        special: pxl_core::Special::default(),
    })
}

pub(super) fn crop_indexed(
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
pub(super) fn json_u32(v: &Value, key: &str) -> CmdResult<u32> {
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

pub(super) fn string_list(v: &Value) -> CmdResult<Vec<String>> {
    let Some(arr) = v.as_array() else {
        return Ok(Vec::new());
    };
    Ok(arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect())
}
