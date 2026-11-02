use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use pxl_art::{FinishOptions, Material};
use pxl_contract::plan::{AssetPlan, VisualPlan};
use pxl_contract::visual::{FinishPolicy, PaletteMode, VisualIntent};
use pxl_core::{IndexFrame, Palette, PxlError};
use pxl_format::PxlDoc;
use serde_json::{json, Value};

use crate::error::CmdResult;
use crate::util;

pub(super) struct VisualContext {
    root: PathBuf,
    pub intent: VisualIntent,
    pub plan: VisualPlan,
    pub intent_digest: String,
    pub plan_digest: String,
    /// Effective policy when the intent carries the modern profile. The same
    /// thresholds drive generation, dry-run and final QA, so a build can
    /// never pass QA under rules it was not authored with. `None` keeps the
    /// retro defaults the pipeline has always enforced.
    pub policy: Option<pxl_contract::policy::EffectivePolicy>,
    /// Resolved palette families (P04). Empty for v1 intents; for v2 intents
    /// every declared family is verified (palette bytes + semantic digest)
    /// at load time, so a stale palette fails before any build work starts.
    pub families: pxl_contract::palette_family::FamilyRegistry,
}

pub(super) struct VisualApplication {
    pub palette: Palette,
    pub report: Value,
}

impl VisualContext {
    pub fn for_recipe(recipe_path: &Path) -> CmdResult<Option<Self>> {
        let recipe_dir = recipe_path.parent().unwrap_or_else(|| Path::new("."));
        let root = recipe_dir.parent().unwrap_or(recipe_dir);
        let required = root.join("spec-manifest.json").is_file();
        Self::load(root, required)
    }

    pub fn load(root: &Path, required: bool) -> CmdResult<Option<Self>> {
        let intent_path = root.join("visual-intent.snapshot.json");
        let plan_path = root.join("visual-plan.json");
        if !required && !intent_path.is_file() && !plan_path.is_file() {
            return Ok(None);
        }
        let intent: VisualIntent = pxl_format::parse_json(
            &fs::read_to_string(&intent_path)
                .map_err(|e| PxlError::io(format!("visual-intent.snapshot.json: {e}")))?,
            "visual intent",
        )?;
        let plan: VisualPlan = pxl_format::parse_json(
            &fs::read_to_string(&plan_path)
                .map_err(|e| PxlError::io(format!("visual-plan.json: {e}")))?,
            "visual plan",
        )?;
        let intent_digest = util::hash_of(
            &fs::read(&intent_path)
                .map_err(|e| PxlError::io(format!("visual-intent.snapshot.json: {e}")))?,
        );
        let plan_digest = util::hash_of(
            &fs::read(&plan_path)
                .map_err(|e| PxlError::io(format!("visual-plan.json: {e}")))?,
        );
        let mut errors = intent.validate();
        errors.extend(plan.validate(&intent));
        if plan.intent_digest != intent_digest {
            errors.push("visual plan intent digest does not match the frozen snapshot".into());
        }
        if !errors.is_empty() {
            return Err(PxlError::semantic(format!(
                "visual contract failed: {}",
                errors.join("; ")
            ))
            .into());
        }
        let policy = intent
            .modern
            .as_ref()
            .map(|modern| {
                pxl_contract::policy::EffectivePolicy::resolve(&intent, modern)
                    .map_err(|e| PxlError::semantic(format!("effective policy: {e}")))
            })
            .transpose()?;
        // P04: resolve and verify every declared palette family before the
        // build starts. Digest mismatches carry the family id.
        let families = if intent.is_v2() {
            pxl_contract::palette_family::FamilyRegistry::resolve(&intent, root)
                .map_err(|e| PxlError::semantic(format!("palette families: {e}")))?
        } else {
            Default::default()
        };
        Ok(Some(Self {
            root: root.to_path_buf(),
            intent,
            plan,
            intent_digest,
            plan_digest,
            policy,
            families,
        }))
    }

    /// QA thresholds that match this context: modern intents derive them from
    /// the effective policy, retro intents keep the tier defaults.
    pub fn qa_thresholds(&self) -> pxl_contract::policy::QaThresholds {
        self.policy
            .as_ref()
            .map(|policy| policy.qa_thresholds())
            .unwrap_or_else(pxl_contract::policy::QaThresholds::retro_defaults)
    }

    /// Palette family of one asset (P04). The intent's asset policy is the
    /// authority; a v2 plan that disagrees was already refused by validation.
    /// Returns `None` for v1 intents so the legacy single-palette path stays
    /// byte-identical.
    pub fn asset_family(&self, asset_id: &str) -> Option<&str> {
        if !self.intent.is_v2() {
            return None;
        }
        self.intent
            .asset_policies
            .iter()
            .find(|policy| policy.asset_id == asset_id)
            .and_then(|policy| policy.palette_family.as_deref())
    }

    /// Verify the palette a recipe declares is the family's palette: the
    /// recipe must load the exact palette file the intent froze, and the
    /// loaded colours must equal the family's colour array. Mixed-family
    /// input fails here with the family id, never silently at pack time.
    pub fn check_recipe_family(
        &self,
        asset_id: &str,
        recipe_palette: &str,
        palette: &Palette,
    ) -> CmdResult<()> {
        if !self.intent.is_v2() {
            return Ok(());
        }
        let Some(family_id) = self.asset_family(asset_id) else {
            return Ok(());
        };
        let Some(family) = self.families.get(family_id) else {
            return Err(PxlError::semantic(format!(
                "asset '{asset_id}' references palette family '{family_id}' that did not resolve"
            ))
            .into());
        };
        let declared = Path::new(recipe_palette)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let expected = Path::new(&family.palette_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if declared != expected {
            return Err(PxlError::semantic(format!(
                "asset '{asset_id}' declares palette '{declared}' but palette family '{family_id}' froze '{expected}'"
            ))
            .with_hint("recipes must build with the family palette; cross-family quantisation is not performed")
            .into());
        }
        let actual: Vec<String> = palette.colors.iter().map(|color| color.to_hex()).collect();
        if actual != family.colors {
            return Err(PxlError::semantic(format!(
                "asset '{asset_id}' palette does not equal palette family '{family_id}' colours"
            ))
            .with_hint("the recipe palette must be the family palette, unchanged")
            .into());
        }
        Ok(())
    }

    /// Validate the declared palette against the effective policy: every
    /// allowed index must exist in the palette, and the palette must not
    /// exceed the declared entry budget. This runs at load time so a broken
    /// contract fails before any build work starts.
    pub fn check_palette_budget(&self, palette: &Palette) -> CmdResult<()> {
        let Some(policy) = &self.policy else {
            return Ok(());
        };
        if palette.colors.len() > usize::from(policy.palette_entries) {
            return Err(PxlError::semantic(format!(
                "palette has {} colors but the effective policy budgets {} entries",
                palette.colors.len(),
                policy.palette_entries
            ))
            .into());
        }
        let allowed: BTreeSet<u8> = self
            .intent
            .palette_policy
            .allowed_indices
            .iter()
            .copied()
            .collect();
        if let Some(index) = allowed
            .iter()
            .copied()
            .find(|index| usize::from(*index) >= palette.colors.len())
        {
            return Err(PxlError::semantic(format!(
                "allowed palette index {index} is outside the palette ({} colors)",
                palette.colors.len()
            ))
            .into());
        }
        Ok(())
    }

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

fn material_index(material: Material) -> u8 {
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

fn finish_options(asset: &AssetPlan, doc: &PxlDoc) -> CmdResult<FinishOptions> {
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

fn transformed_regions(
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

fn decode_mask(rows: &[String], w: u16, h: u16) -> CmdResult<Vec<bool>> {
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

fn changed_pixels(before: &IndexFrame, after: &IndexFrame) -> Value {
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

pub(super) fn palette_bytes(palette: &Palette) -> CmdResult<Vec<u8>> {
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

#[cfg(test)]
mod tests {
    #[test]
    fn contract_hard_limits_match_step_300() {
        use pxl_contract::policy;
        assert_eq!(policy::MAX_DIM, pxl_core::MAX_DIM);
        assert_eq!(policy::MAX_TEXT_COLORS, pxl_core::MAX_TEXT_COLORS);
        assert_eq!(policy::MAX_CANVAS, pxl_core::MAX_CANVAS);
        assert_eq!(usize::from(policy::MAX_ANIM_FRAMES), pxl_core::MAX_ANIM_FRAMES);
    }
}
