use std::collections::BTreeMap;

use pxl_core::{Palette, PxlError};
use pxl_format::PxlDoc;
use pxl_qa::{AnimSeq, ValidateInput};

use crate::cmd::sheet::FACING_NAMES;
use crate::error::CmdResult;
use crate::util::{self};


mod sheet;
mod helpers;

pub(super) use sheet::sheet_input;

pub(super) fn doc_input(doc: &PxlDoc, palette: &Palette) -> CmdResult<ValidateInput> {
    let mut frames = util::frames_of(doc)?;
    if doc.meta.kind.as_deref() == Some("parts") {
        frames.retain(|(name, frame)| name != "base" || frame.count_opaque() > 0);
    }
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
                frames: helpers::lookup_frames(doc, &names)?,
                frame_names: names,
                static_mode,
                ..Default::default()
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
                    frames: helpers::lookup_frames(doc, &names)?,
                    frame_names: names,
                    static_mode,
                    ..Default::default()
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

pub(super) fn apply_recipe_contract(
    input: &mut ValidateInput,
    sprite: &pxl_format::RecipeSprite,
) -> CmdResult<()> {
    for contract in &sprite.anims {
        if contract.anchor_tolerance_px.is_some() && contract.anchor.is_none() {
            return Err(PxlError::schema(format!(
                "anim '{}' has anchor_tolerance_px without an anchor",
                contract.name
            ))
            .into());
        }
        let anchor = contract
            .anchor
            .as_ref()
            .map(|name| {
                sprite.anchors.get(name).copied().ok_or_else(|| {
                    PxlError::schema(format!(
                        "anim '{}' references unknown anchor '{name}'",
                        contract.name
                    ))
                })
            })
            .transpose()?;
        let mut protected = Vec::new();
        for name in &contract.protected_features {
            let region = sprite.protected_regions.get(name).copied().ok_or_else(|| {
                PxlError::schema(format!(
                    "anim '{}' references unknown protected region '{name}'",
                    contract.name
                ))
            })?;
            protected.push(helpers::rect(region)?);
        }
        let changes = contract
            .change_regions
            .iter()
            .copied()
            .map(helpers::rect)
            .collect::<CmdResult<Vec<_>>>()?;
        for anim in input
            .anims
            .iter_mut()
            .filter(|anim| anim.base_name == contract.name)
        {
            anim.anchor = anchor;
            anim.anchor_tolerance_px = contract
                .anchor_tolerance_px
                .or(contract.anchor.as_ref().map(|_| 0));
            anim.protected_regions = protected.clone();
            anim.change_regions = changes.clone();
            anim.allow_duplicate_frames = contract.allow_duplicate_frames;
            anim.allow_empty_frames = contract.allow_empty_frames;
            anim.allow_terminal_empty = contract.allow_terminal_empty;
        }
    }
    Ok(())
}
