use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use pxl_art::Material;
use pxl_contract::plan::AssetPlan;
use pxl_contract::visual::PaletteMode;
use pxl_core::{Palette, PxlError};
use pxl_format::PxlDoc;
use serde_json::{json, Value};

use crate::error::CmdResult;
use crate::util;


use super::{VisualApplication, VisualContext};
use super::helpers::{changed_pixels, decode_mask, finish_options, material_index, palette_bytes, transformed_regions};

impl VisualContext {
    pub fn apply(
        &self,
        doc: &mut PxlDoc,
        source_palette: &Palette,
        recipe_dir: &Path,
    ) -> CmdResult<VisualApplication> {
        let asset_id = doc.meta.asset.clone().unwrap_or_default();
        let asset = self
            .plan
            .assets
            .iter()
            .find(|asset| asset.asset_id == asset_id)
            .ok_or_else(|| PxlError::semantic(format!("asset '{asset_id}' has no visual plan")))?;
        if asset.native_size != [doc.w, doc.h] {
            return Err(PxlError::semantic(format!(
                "asset '{asset_id}' is {}x{}, visual plan requires {}x{}",
                doc.w, doc.h, asset.native_size[0], asset.native_size[1]
            ))
            .into());
        }
        for animation in &asset.animations {
            let durations = animation
                .phases
                .iter()
                .flat_map(|phase| phase.duration_ms.iter().copied())
                .collect::<Vec<_>>();
            doc.meta
                .durations
                .insert(animation.anim_id.clone(), durations);
        }
        let (materials, protect) = self.base_regions(asset, doc, recipe_dir)?;
        let mut options = finish_options(asset, doc)?;
        options.ramps.generate = self.intent.palette_policy.mode == PaletteMode::ExplicitExtension;
        options.ramps.max_colors = source_palette
            .colors
            .len()
            .saturating_add(usize::from(self.intent.palette_policy.extension_budget))
            .min(pxl_core::MAX_TEXT_COLORS);
        let mut usage = [0u32; 256];
        let base_name = doc
            .frame_names()
            .into_iter()
            .find(|name| name == "base")
            .unwrap_or_else(|| doc.frame_names()[0].clone());
        for pixel in doc.frame(&base_name)?.pixels {
            usage[pixel as usize] += 1;
        }
        let (shared_palette, shared_shades, ramp_report) =
            pxl_art::palette::build_shades(source_palette, &usage, options.ramps);
        options.shared_shades = Some((shared_palette.clone(), shared_shades));
        let mut final_palette = shared_palette;
        let mut frames = Vec::new();
        for name in doc.frame_names() {
            let before = doc.frame(&name)?;
            let (frame_materials, frame_protect) = transformed_regions(
                asset,
                &name,
                &materials,
                &protect,
                &before,
            )?;
            options.materials = Some(frame_materials);
            options.protect = Some(frame_protect.clone());
            let finished = pxl_art::finish(&before, &final_palette, &options)?;
            let changed = changed_pixels(&before, &finished.frame);
            final_palette = finished.palette;
            doc.insert_frame(&name, &finished.frame)?;
            frames.push(json!({
                "frame": name,
                "stages": finished.report.stages,
                "changed": changed,
                "protect_pixels": frame_protect.iter().filter(|value| **value).count(),
                "shade": {
                    "volumes": finished.report.shade.volumes,
                    "flat": finished.report.shade.flat,
                    "lit": finished.report.shade.lit,
                    "shaded": finished.report.shade.shaded,
                    "speculars": finished.report.shade.speculars,
                },
                "warnings": finished.report.warnings,
            }));
        }
        final_palette.validate()?;
        let allowed = self
            .intent
            .palette_policy
            .allowed_indices
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let extension = final_palette.colors.len().saturating_sub(source_palette.colors.len());
        match self.intent.palette_policy.mode {
            PaletteMode::Strict => {
                if final_palette.colors.len() != source_palette.colors.len() {
                    return Err(PxlError::semantic("strict palette finish generated new colours").into());
                }
                for name in doc.frame_names() {
                    if let Some(index) = doc
                        .frame(&name)?
                        .pixels
                        .iter()
                        .find(|index| !allowed.contains(index))
                    {
                        return Err(PxlError::semantic(format!(
                            "asset '{asset_id}' frame '{name}' uses palette index {index} outside the allowed subset"
                        ))
                        .into());
                    }
                }
            }
            PaletteMode::ExplicitExtension => {
                if extension > usize::from(self.intent.palette_policy.extension_budget) {
                    return Err(PxlError::semantic(format!(
                        "palette extension {extension} exceeds budget {}",
                        self.intent.palette_policy.extension_budget
                    ))
                    .into());
                }
            }
        }
        let report = json!({
            "asset_id": asset_id,
            "role": asset.role,
            "finish": asset.finish,
            "lighting": asset.lighting,
            "native_size": asset.native_size,
            "palette": {
                "mode": self.intent.palette_policy.mode,
                "source_colors": source_palette.colors.len(),
                "output_colors": final_palette.colors.len(),
                "extension": extension,
                "budget": self.intent.palette_policy.extension_budget,
                "allowed_indices": self.intent.palette_policy.allowed_indices,
                "generated_ramps": ramp_report.generated,
                "digest": util::hash_of(&palette_bytes(&final_palette)?),
            },
            "frames": frames,
        });
        Ok(VisualApplication {
            palette: final_palette,
            report,
        })
    }

    pub fn channel_doc(
        &self,
        doc: &PxlDoc,
        recipe_dir: &Path,
        channel: &str,
    ) -> CmdResult<PxlDoc> {
        let asset_id = doc.meta.asset.clone().unwrap_or_default();
        let asset = self
            .plan
            .assets
            .iter()
            .find(|asset| asset.asset_id == asset_id)
            .ok_or_else(|| PxlError::semantic(format!("asset '{asset_id}' has no visual plan")))?;
        let (materials, protect) = self.base_regions(asset, doc, recipe_dir)?;
        let mut output = doc.clone();
        output.palette = "channel.palette.json".into();
        for name in doc.frame_names() {
            let frame = doc.frame(&name)?;
            let (frame_materials, _) = transformed_regions(
                asset,
                &name,
                &materials,
                &protect,
                &frame,
            )?;
            let mut mapped = frame.clone();
            for (index, pixel) in mapped.pixels.iter_mut().enumerate() {
                if *pixel == 0 {
                    continue;
                }
                *pixel = match channel {
                    "material" => material_index(frame_materials[index]),
                    "emissive" => u8::from(frame_materials[index] == Material::Emissive),
                    "shadow" => 1,
                    other => {
                        return Err(PxlError::usage(format!("unknown visual channel '{other}'")).into())
                    }
                };
            }
            output.insert_frame(&name, &mapped)?;
        }
        Ok(output)
    }

    pub fn effective_style(&self, assets: Vec<Value>) -> Value {
        json!({
            "contract": "effective-style/v1",
            "intent": {"path": "visual-intent.snapshot.json", "digest": self.intent_digest},
            "plan": {"path": "visual-plan.json", "digest": self.plan_digest},
            "profile": self.intent.profile,
            "sources": self.intent.sources,
            "pixel_grid": self.intent.pixel_grid,
            "scene_previews": self.intent.scene_previews,
            "palette_policy": self.intent.palette_policy,
            "effective_policy": self.policy,
            "palette_families": self.families.summary(),
            "assets": assets,
        })
    }

    fn base_regions(
        &self,
        asset: &AssetPlan,
        doc: &PxlDoc,
        recipe_dir: &Path,
    ) -> CmdResult<(Vec<Material>, Vec<bool>)> {
        let size = doc.w as usize * doc.h as usize;
        let mut materials = vec![Material::Organic; size];
        let mut protect = vec![false; size];
        let mut assigned = vec![false; size];
        for region in &asset.regions {
            let mask = self.region_mask(asset, region, recipe_dir, doc.w, doc.h)?;
            let material = Material::parse(&region.material_id).ok_or_else(|| {
                PxlError::semantic(format!(
                    "asset '{}' region '{}' uses unknown material '{}'",
                    asset.asset_id, region.region_id, region.material_id
                ))
            })?;
            for (index, selected) in mask.into_iter().enumerate() {
                if !selected {
                    continue;
                }
                if assigned[index] && materials[index] != material {
                    return Err(PxlError::semantic(format!(
                        "asset '{}' region '{}' overlaps another material region at pixel {index}",
                        asset.asset_id, region.region_id
                    ))
                    .into());
                }
                assigned[index] = true;
                materials[index] = material;
                protect[index] |= region.protect;
            }
        }
        Ok((materials, protect))
    }

    fn region_mask(
        &self,
        asset: &AssetPlan,
        region: &pxl_contract::plan::RegionPlan,
        recipe_dir: &Path,
        w: u16,
        h: u16,
    ) -> CmdResult<Vec<bool>> {
        match region.source.kind.as_str() {
            "shape" => {
                let (file, selector) = region.source.selector.split_once('#').ok_or_else(|| {
                    PxlError::semantic(format!(
                        "asset '{}' region '{}' needs file#region selector",
                        asset.asset_id, region.region_id
                    ))
                })?;
                let path = recipe_dir.join(file);
                let shapes: pxl_format::ShapesFile = pxl_format::parse_shapes(
                    &fs::read_to_string(&path)
                        .map_err(|e| PxlError::io(format!("{}: {e}", path.display())))?,
                )?;
                let named = shapes.regions.get(selector).ok_or_else(|| {
                    PxlError::semantic(format!(
                        "{} has no region '{selector}'",
                        path.display()
                    ))
                })?;
                decode_mask(&named.pixels, w, h)
            }
            "mask" => {
                let mask = region.mask.as_ref().ok_or_else(|| {
                    PxlError::semantic("mask source needs mask metadata")
                })?;
                let path = self.root.join(&mask.path);
                let bytes = fs::read(&path)
                    .map_err(|e| PxlError::io(format!("{}: {e}", path.display())))?;
                if util::hash_of(&bytes) != mask.digest {
                    return Err(PxlError::semantic(format!(
                        "mask '{}' digest does not match visual plan",
                        mask.path
                    ))
                    .into());
                }
                let region_file: pxl_format::ShapeRegion = pxl_format::parse_json(
                    &String::from_utf8(bytes)
                        .map_err(|_| PxlError::schema(format!("{} is not UTF-8", path.display())))?,
                    "region mask",
                )?;
                decode_mask(&region_file.pixels, w, h)
            }
            other => Err(PxlError::semantic(format!(
                "asset '{}' region '{}' source '{other}' needs an explicit mask in this build",
                asset.asset_id, region.region_id
            ))
            .into()),
        }
    }

}
