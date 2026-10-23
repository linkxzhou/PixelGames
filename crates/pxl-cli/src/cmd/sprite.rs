use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use pxl_core::{IndexFrame, Mirror, Palette, PxlError};
use pxl_format::{Anim, AnimFrames, Op, PxlDoc, Recipe};
use pxl_gen::{generate_bollinger, generate_shapes};
use pxl_ops::expand_macro;

use super::animation::check_anim_timing;
use super::document::{apply_with, doc_from_frames};
use super::generate::{load_mask_maybe_mirror, spec_file_name};
use crate::error::CmdResult;
use crate::util::Ctx;

/// Declared use of an asset. QA follows it: `tiles` are full-bleed surfaces,
/// `effect` is judged by its lifecycle, `ui` and `overlay` may be sparse.
pub(super) const KINDS: &[&str] = &["sprite", "tiles", "parts", "effect", "ui", "overlay"];

pub(super) fn overwrite_ctx(ctx: &Ctx) -> Ctx {
    let mut c = ctx.clone();
    c.overwrite = true;
    c.json = false;
    c.quiet = true;
    c
}

/// `compose` repaints the whole canvas from the parts, discarding anything the
/// source frame already carried (its outline, its shading). Only such a frame
/// needs the sprite's `post` ops again; a frame that merely moves or recolours an
/// already post-processed source must not get a second outline.
pub(super) fn redraws_canvas(ops: &[Op]) -> bool {
    ops.iter().any(|op| matches!(op, Op::Compose { .. }))
}

pub(super) fn build_sprite(
    sprite: &pxl_format::RecipeSprite,
    palette: &Palette,
    palette_ref: &str,
    seed: u64,
    recipe: &Recipe,
    recipe_dir: &Path,
    recipe_path: &Path,
) -> CmdResult<PxlDoc> {
    let base_seed = sprite.base.seed.unwrap_or(seed);
    let (frame, rig) = match sprite.base.gen.as_str() {
        "shapes" => {
            let spec = sprite.base.spec.as_deref().ok_or_else(|| {
                PxlError::schema("shapes base requires spec").with_path("/sprites/0/base/spec")
            })?;
            let path = recipe_dir.join(spec);
            let text = fs::read_to_string(&path)
                .map_err(|e| PxlError::io(format!("cannot read {}: {e}", path.display())))?;
            let shapes = pxl_format::parse_shapes(&text)?;
            (generate_shapes(&shapes, palette)?, None)
        }
        "bollinger" => {
            let template = sprite
                .base
                .template
                .as_deref()
                .ok_or_else(|| PxlError::schema("bollinger base requires template"))?;
            let mirror = sprite
                .base
                .mirror
                .as_deref()
                .map(Mirror::parse)
                .transpose()?;
            let mask = load_mask_maybe_mirror(template, recipe_dir, mirror)?;
            (
                generate_bollinger(&mask, base_seed, palette, sprite.base.ramp.as_deref())?,
                None,
            )
        }
        "parts" => {
            let spec = sprite.base.spec.as_deref().ok_or_else(|| {
                PxlError::schema("parts base requires spec").with_path("/sprites/0/base/spec")
            })?;
            let path = recipe_dir.join(spec);
            let text = fs::read_to_string(&path)
                .map_err(|e| PxlError::io(format!("cannot read {}: {e}", path.display())))?;
            let rig = pxl_format::parse_parts(&text)?;
            let frame = IndexFrame::new(rig.w, rig.h)?;
            (frame, Some(rig))
        }
        other => {
            return Err(
                PxlError::schema(format!("unknown base generator '{other}'"))
                    .with_hint("base.gen is shapes, bollinger, or parts")
                    .into(),
            );
        }
    };
    let parts_mode = rig.is_some();
    let mut doc = doc_from_frames(
        palette_ref,
        &[("base".into(), frame.clone())],
        seed,
        if parts_mode { "parts" } else { "sprite" },
    )?;
    doc.meta.asset = Some(sprite.id.clone());
    doc.meta.recipe = Some(
        recipe_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("recipe.json")
            .into(),
    );
    doc.meta.gen_version = Some(recipe.gen_version);
    if let Some(kind) = &sprite.kind {
        if !KINDS.contains(&kind.as_str()) {
            return Err(PxlError::schema(format!("unknown sprite kind '{kind}'"))
                .with_path("/kind")
                .with_hint("kind is sprite, tiles, parts, effect, ui, or overlay")
                .into());
        }
        doc.meta.kind = Some(kind.clone());
    }
    if let Some(ramp) = &sprite.tile_ramp {
        if !palette.ramps.contains_key(ramp) {
            return Err(PxlError::semantic(format!("tile_ramp '{ramp}' is missing"))
                .with_path("/tile_ramp")
                .with_hint("name a ramp declared on the palette")
                .into());
        }
        doc.meta.tile_ramp = Some(ramp.clone());
    }
    if let Some(spec) = sprite.base.spec.as_ref().filter(|_| parts_mode) {
        doc.meta.parts = Some(spec_file_name(spec));
    }
    // Parts redraw the canvas in `compose`, so outline and palette swaps run after
    // that draw. Shape and mask sprites still post-process the base first.
    let base_ops: &[Op] = if parts_mode { &[] } else { &sprite.post };
    let base_frame = apply_with(
        &frame,
        base_ops,
        palette,
        seed,
        "base",
        recipe_dir,
        rig.as_ref(),
    )?;
    doc.insert_frame("base", &base_frame)?;
    let mut named: BTreeMap<String, IndexFrame> = BTreeMap::new();
    named.insert("base".into(), base_frame);
    for dir in &sprite.directions {
        let src = named.get(&dir.from).cloned().ok_or_else(|| {
            PxlError::semantic(format!(
                "direction '{}' reads '{}' which has not been built yet",
                dir.id, dir.from
            ))
            .with_hint("list directions so each `from` already exists")
        })?;
        let mut ops = dir.ops.clone();
        if parts_mode && redraws_canvas(&ops) {
            ops.extend(sprite.post.iter().cloned());
        }
        let out = apply_with(&src, &ops, palette, seed, &dir.id, recipe_dir, rig.as_ref())?;
        named.insert(dir.id.clone(), out.clone());
        doc.insert_frame(&dir.id, &out)?;
    }
    for anim in &sprite.anims {
        let mut by_dir = BTreeMap::new();
        let mut seq = Vec::new();
        let targets: Vec<Option<String>> = if anim.dirs.is_empty() {
            vec![None]
        } else {
            anim.dirs.iter().cloned().map(Some).collect()
        };
        for target in &targets {
            let source_name = target.clone().unwrap_or_else(|| "base".into());
            let source = named
                .get(&source_name)
                .cloned()
                .or_else(|| doc.frame(&source_name).ok())
                .ok_or_else(|| {
                    PxlError::semantic(format!(
                        "anim '{}' missing source frame '{source_name}'",
                        anim.name
                    ))
                })?;
            let lists = if let Some(mac) = anim.macro_name() {
                expand_macro(mac, target.as_deref(), &source, palette, rig.as_ref())?
            } else if !anim.frames.is_empty() {
                anim.frames.clone()
            } else {
                return Err(PxlError::schema(format!(
                    "anim '{}' has no macro or frames",
                    anim.name
                ))
                .into());
            };
            check_anim_timing(&anim.name, anim.fps, lists.len())?;
            let mut names = Vec::new();
            for (i, ops) in lists.iter().enumerate() {
                let name = match target {
                    Some(dir) => format!("{}_{dir}_{i}", anim.name),
                    None => format!("{}_{i}", anim.name),
                };
                let mut ops = ops.clone();
                if parts_mode && redraws_canvas(&ops) {
                    ops.extend(sprite.post.iter().cloned());
                }
                let frame = apply_with(
                    &source,
                    &ops,
                    palette,
                    seed,
                    &name,
                    recipe_dir,
                    rig.as_ref(),
                )?;
                doc.insert_frame(&name, &frame)?;
                names.push(name);
            }
            if let Some(dir) = target {
                by_dir.insert(dir.clone(), names);
            } else {
                seq = names;
            }
        }
        doc.anims.insert(
            anim.name.clone(),
            Anim {
                fps: anim.fps,
                looped: anim.looped,
                dirs: anim.dirs.clone(),
                frames: if by_dir.is_empty() {
                    AnimFrames::Seq(seq)
                } else {
                    AnimFrames::ByDir(by_dir)
                },
                mode: anim.mode,
            },
        );
    }
    Ok(doc)
}
