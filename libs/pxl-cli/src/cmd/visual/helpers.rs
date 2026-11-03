
use pxl_art::{FinishOptions, Material};
use pxl_contract::plan::AssetPlan;
use pxl_contract::visual::FinishPolicy;
use pxl_core::{IndexFrame, Palette, PxlError};
use pxl_format::PxlDoc;
use serde_json::{json, Value};

use crate::error::CmdResult;



pub(crate) fn material_index(material: Material) -> u8 {
    match material {
        Material::Organic => 1,
        Material::Outline => 2,
        Material::Metal => 3,
        Material::Wood => 4,
        Material::Stone => 5,
        Material::Water => 6,
        Material::Foliage => 7,
        Material::Cloth => 8,
        Material::Skin => 9,
        Material::Hair => 10,
        Material::Glass => 11,
        Material::Emissive => 12,
        Material::Flat => 13,
    }
}

pub(crate) fn finish_options(asset: &AssetPlan, doc: &PxlDoc) -> CmdResult<FinishOptions> {
    let profile = pxl_core::SizeProfile::for_size(doc.w, doc.h)?;
    let mut options = FinishOptions::for_profile(&profile);
    options.ramps.generate = false;
    options.depillow = false;
    match asset.finish {
        FinishPolicy::Keep => {
            options.cleanup = pxl_art::CleanOptions::NONE;
            options.outline = pxl_art::OutlineMode::Keep;
            options.shade = pxl_art::ShadeMode::None;
            options.aa = pxl_core::AaPolicy::Off;
            options.dither = false;
            options.inner_lines = false;
        }
        FinishPolicy::Flat => {
            options.outline = pxl_art::OutlineMode::Keep;
            options.shade = pxl_art::ShadeMode::None;
        }
        FinishPolicy::Volume | FinishPolicy::HardEdge => {
            options.outline = pxl_art::OutlineMode::Keep;
            options.shade = pxl_art::ShadeMode::Volume;
        }
        FinishPolicy::Selout => {
            options.outline = pxl_art::OutlineMode::Selout;
            options.shade = pxl_art::ShadeMode::None;
        }
    }
    if asset.tile.is_some() {
        options.cleanup = pxl_art::CleanOptions::NONE;
        options.outline = pxl_art::OutlineMode::Keep;
        options.aa = pxl_core::AaPolicy::Off;
        options.dither = false;
        options.inner_lines = false;
    }
    Ok(options)
}

pub(crate) fn transformed_regions(
    asset: &AssetPlan,
    frame_name: &str,
    materials: &[Material],
    protect: &[bool],
    frame: &IndexFrame,
) -> CmdResult<(Vec<Material>, Vec<bool>)> {
    let mut offset = [0, 0];
    for animation in &asset.animations {
        for phase in &animation.phases {
            let matches = phase.frames.iter().any(|candidate| {
                candidate == frame_name
                    || candidate.ends_with(frame_name)
                    || frame_name.ends_with(candidate)
            });
            if !matches {
                continue;
            }
            if let Some(transform) = &phase.transform {
                if transform.rotate90 != 0 || !matches!(transform.kind.as_str(), "translate") {
                    return Err(PxlError::semantic(format!(
                        "asset '{}' frame '{frame_name}' uses unsupported region transform '{}'",
                        asset.asset_id, transform.kind
                    ))
                    .into());
                }
                offset = transform.offset;
            }
        }
    }
    if offset == [0, 0] {
        return Ok((materials.to_vec(), protect.to_vec()));
    }
    let size = frame.w as usize * frame.h as usize;
    let mut next_materials = vec![Material::Organic; size];
    let mut next_protect = vec![false; size];
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let tx = x + offset[0];
            let ty = y + offset[1];
            if tx < 0 || ty < 0 || tx >= frame.w as i32 || ty >= frame.h as i32 {
                continue;
            }
            let source = (y * frame.w as i32 + x) as usize;
            let target = (ty * frame.w as i32 + tx) as usize;
            next_materials[target] = materials[source];
            next_protect[target] = protect[source];
        }
    }
    Ok((next_materials, next_protect))
}

pub(crate) fn decode_mask(rows: &[String], w: u16, h: u16) -> CmdResult<Vec<bool>> {
    if rows.len() != h as usize || rows.iter().any(|row| row.chars().count() != w as usize) {
        return Err(PxlError::semantic(format!(
            "region mask is {} rows, expected {} rows of {} pixels",
            rows.len(), h, w
        ))
        .into());
    }
    let mut mask = Vec::with_capacity(w as usize * h as usize);
    for row in rows {
        for cell in row.chars() {
            mask.push(!matches!(cell, '.' | '0' | ' '));
        }
    }
    Ok(mask)
}

pub(crate) fn changed_pixels(before: &IndexFrame, after: &IndexFrame) -> Value {
    let mut count = 0u32;
    let mut min = [u32::MAX, u32::MAX];
    let mut max = [0, 0];
    for (index, (a, b)) in before.pixels.iter().zip(&after.pixels).enumerate() {
        if a == b {
            continue;
        }
        count += 1;
        let x = index as u32 % u32::from(before.w);
        let y = index as u32 / u32::from(before.w);
        min[0] = min[0].min(x);
        min[1] = min[1].min(y);
        max[0] = max[0].max(x);
        max[1] = max[1].max(y);
    }
    let bbox = if count == 0 {
        None
    } else {
        Some([
            min[0],
            min[1],
            max[0] - min[0] + 1,
            max[1] - min[1] + 1,
        ])
    };
    json!({"pixels": count, "bbox": bbox})
}

pub(crate) fn palette_bytes(palette: &Palette) -> CmdResult<Vec<u8>> {
    let mut text = serde_json::to_string_pretty(&json!({
        "v": 1,
        "id": palette.id,
        "colors": palette.colors.iter().map(|color| color.to_hex()).collect::<Vec<_>>(),
        "ramps": palette.ramps,
        "cycles": palette.cycles,
        "special": palette.special,
    }))
    .map_err(|e| PxlError::io(format!("palette JSON: {e}")))?;
    text.push('\n');
    Ok(text.into_bytes())
}
