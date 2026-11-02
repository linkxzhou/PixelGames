use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use pxl_core::PxlError;
use pxl_export::decode_indexed;
use pxl_qa::{AnimSeq, ValidateInput};
use serde_json::{json, Value};

use crate::error::{CmdError, CmdResult};


use super::helpers::{crop_indexed, json_u32, palette_from_colors, palette_from_pxl, reconcile_palette, string_list};

pub(crate) fn sheet_input(json_path: &Path, value: &Value) -> CmdResult<ValidateInput> {
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
                        ..Default::default()
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
            let kind = entry.get("kind").and_then(|v| v.as_str()).ok_or_else(|| {
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
