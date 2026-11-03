use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};


use super::types::*;

impl VisualIntent {
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let v2 = self.is_v2();
        if self.contract != VISUAL_INTENT_CONTRACT && !v2 {
            errors.push(format!(
                "contract must be {VISUAL_INTENT_CONTRACT} or {VISUAL_INTENT_CONTRACT_V2}, got '{}'",
                self.contract
            ));
        }
        if !v2 {
            if self.modern.is_some() || !self.palette_families.is_empty() {
                errors.push(format!("modern and palette_families need {VISUAL_INTENT_CONTRACT_V2}"));
            }
            if self.asset_policies.iter().any(|asset| {
                asset.palette_family.is_some() || asset.facing.is_some() || asset.anchor.is_some()
            }) {
                errors.push(format!(
                    "asset palette_family, facing and anchor need {VISUAL_INTENT_CONTRACT_V2}"
                ));
            }
        } else {
            self.validate_v2(&mut errors);
        }
        if !stable_id(&self.profile.id) {
            errors.push("profile.id must be a stable id".into());
        }
        if !digest(&self.profile.digest) {
            errors.push("profile.digest must be a sha256 digest".into());
        }
        validate_size("pixel_grid.native_size", self.pixel_grid.native_size, &mut errors);
        if self.pixel_grid.display_scale == 0 {
            errors.push("pixel_grid.display_scale must be at least 1".into());
        }
        if self.palette_policy.palette.trim().is_empty() {
            errors.push("palette_policy.palette must not be empty".into());
        }
        if self.palette_policy.allowed_indices.is_empty() {
            errors.push("palette_policy.allowed_indices must not be empty".into());
        }
        if !self.palette_policy.allowed_indices.contains(&0) {
            errors.push("palette_policy.allowed_indices must include transparent index 0".into());
        }
        let indices = self
            .palette_policy
            .allowed_indices
            .iter()
            .collect::<BTreeSet<_>>();
        if indices.len() != self.palette_policy.allowed_indices.len() {
            errors.push("palette_policy.allowed_indices contains duplicates".into());
        }
        if self.palette_policy.mode == PaletteMode::Strict && self.palette_policy.extension_budget != 0 {
            errors.push("strict palette policy requires extension_budget 0".into());
        }
        if self.roles.is_empty() {
            errors.push("at least one role policy is required".into());
        }
        for (id, role) in &self.roles {
            if !stable_id(id) {
                errors.push(format!("role id '{id}' is invalid"));
            }
            validate_range(&format!("roles.{id}.value"), role.value, &mut errors);
            validate_range(
                &format!("roles.{id}.saturation"),
                role.saturation,
                &mut errors,
            );
        }
        for role in self
            .value_script
            .focal_roles
            .iter()
            .chain(&self.value_script.rest_roles)
        {
            if !self.roles.contains_key(role) {
                errors.push(format!("value script references unknown role '{role}'"));
            }
        }
        if let Some(role) = self
            .value_script
            .focal_roles
            .iter()
            .find(|role| self.value_script.rest_roles.contains(role))
        {
            errors.push(format!("role '{role}' cannot be both focal and rest"));
        }
        if self.materials.is_empty() {
            errors.push("at least one material is required".into());
        }
        for (id, material) in &self.materials {
            if !stable_id(id) {
                errors.push(format!("material id '{id}' is invalid"));
            }
            if material.ramp.trim().is_empty() {
                errors.push(format!("material '{id}' has an empty ramp"));
            }
            if material.regions.is_empty() {
                errors.push(format!("material '{id}' has no named regions"));
            }
        }
        if self.asset_policies.is_empty() {
            errors.push("at least one asset policy is required".into());
        }
        let mut assets = BTreeSet::new();
        for asset in &self.asset_policies {
            if !stable_id(&asset.asset_id) || !assets.insert(asset.asset_id.as_str()) {
                errors.push(format!("asset id '{}' is invalid or duplicated", asset.asset_id));
            }
            if !self.roles.contains_key(&asset.role) {
                errors.push(format!("asset '{}' references unknown role '{}'", asset.asset_id, asset.role));
            }
            if let Some(size) = asset.native_size {
                validate_size(&format!("asset '{}'.native_size", asset.asset_id), size, &mut errors);
            }
            for material in &asset.material_ids {
                if !self.materials.contains_key(material) {
                    errors.push(format!("asset '{}' references unknown material '{material}'", asset.asset_id));
                }
            }
            for region in &asset.protected_regions {
                let located = asset.material_ids.iter().any(|id| {
                    self.materials
                        .get(id)
                        .is_some_and(|material| material.regions.contains(region))
                });
                if !located {
                    errors.push(format!(
                        "asset '{}' protected region '{region}' is not bound to a material region",
                        asset.asset_id
                    ));
                }
            }
            if matches!(asset.kind.as_str(), "ui" | "text")
                && asset.finish != FinishPolicy::Keep
                && !matches!(self.sources.get("asset_policies.finish"), Some(ConfigOrigin::User))
            {
                errors.push(format!("UI asset '{}' must keep pixels unless the user explicitly overrides finish", asset.asset_id));
            }
        }
        if self.animation_policy.max_frames == 0 || self.animation_policy.max_frames > 16 {
            errors.push("animation_policy.max_frames must be in 1..=16".into());
        }
        if self.scene_previews.is_empty() {
            errors.push("at least one scene preview is required".into());
        }
        for scene in &self.scene_previews {
            if !stable_id(&scene.id) {
                errors.push(format!("scene id '{}' is invalid", scene.id));
            }
            if v2 {
                validate_canvas(&format!("scene '{}'.size", scene.id), scene.size, &mut errors);
            } else {
                validate_size(&format!("scene '{}'.size", scene.id), scene.size, &mut errors);
            }
            if scene.scale == 0 {
                errors.push(format!("scene '{}' scale must be at least 1", scene.id));
            }
            if scene.instances.is_empty() {
                errors.push(format!("scene '{}' has no real asset instances", scene.id));
            }
            for instance in &scene.instances {
                if !assets.contains(instance.asset_id.as_str()) {
                    errors.push(format!("scene '{}' references unknown asset '{}'", scene.id, instance.asset_id));
                }
            }
            if scene
                .render_mode
                .as_deref()
                .is_some_and(|mode| !matches!(mode, "L0" | "L1" | "L2"))
            {
                errors.push(format!("scene '{}' has an unsupported render mode", scene.id));
            }
            for light in &scene.lights {
                if !stable_id(&light.id) || light.radius == 0 || light.radius > 256 {
                    errors.push(format!("scene '{}' has an invalid light '{}'", scene.id, light.id));
                }
                if !matches!(light.color.len(), 7 | 9)
                    || !light.color.starts_with('#')
                    || !light.color[1..].chars().all(|c| c.is_ascii_hexdigit())
                {
                    errors.push(format!("scene '{}' light '{}' has an invalid colour", scene.id, light.id));
                }
            }
            for roi in &scene.rois {
                if roi.rect[2] == 0 || roi.rect[3] == 0
                    || roi.rect[0].saturating_add(roi.rect[2]) > u32::from(scene.size[0])
                    || roi.rect[1].saturating_add(roi.rect[3]) > u32::from(scene.size[1])
                {
                    errors.push(format!("scene '{}' ROI '{}' is empty or outside the scene", scene.id, roi.id));
                }
            }
        }
        if self.review_policy.required_dimensions.is_empty() {
            errors.push("review_policy.required_dimensions must not be empty".into());
        }
        if self.review_policy.min_reviewers < 2 {
            errors.push("review_policy.min_reviewers must be at least 2".into());
        }
        for path in self.sources.keys() {
            if path.trim().is_empty() {
                errors.push("source keys must not be empty".into());
            }
        }
        for risk in &self.risks {
            if !stable_id(&risk.risk_id) {
                errors.push(format!("risk id '{}' is invalid", risk.risk_id));
            }
            if risk.target.is_empty() || risk.check.is_empty() || risk.evidence_required.is_empty() {
                errors.push(format!("risk '{}' needs target, check and evidence_required", risk.risk_id));
            }
            match risk.status {
                RiskStatus::Open if risk.resolved_by.is_some() => {
                    errors.push(format!("open risk '{}' cannot have resolved_by", risk.risk_id));
                }
                RiskStatus::Resolved | RiskStatus::Accepted => {
                    match &risk.resolved_by {
                        Some(evidence) if digest(&evidence.digest)
                            && !evidence.report.is_empty()
                            && !evidence.entry.is_empty() => {}
                        _ => errors.push(format!("risk '{}' needs a real report entry and digest", risk.risk_id)),
                    }
                }
                _ => {}
            }
        }
        errors
    }
}

impl VisualIntent {
    pub fn is_v2(&self) -> bool {
        self.contract == VISUAL_INTENT_CONTRACT_V2
    }

    fn validate_v2(&self, errors: &mut Vec<String>) {
        if let Some(modern) = &self.modern {
            if let Err(error) = crate::policy::EffectivePolicy::resolve(self, modern) {
                errors.push(format!("modern: {error}"));
            }
        }
        let mut families = BTreeSet::new();
        for family in &self.palette_families {
            if !stable_id(&family.id) || !families.insert(family.id.as_str()) {
                errors.push(format!("palette family id '{}' is invalid or duplicated", family.id));
            }
            if family.palette.trim().is_empty() {
                errors.push(format!("palette family '{}' has no palette path", family.id));
            }
            if !digest(&family.digest) || !digest(&family.semantic_digest) {
                errors.push(format!("palette family '{}' needs sha256 digests", family.id));
            }
            if family.transparent_index != 0 {
                errors.push(format!("palette family '{}' transparent_index must be 0", family.id));
            }
        }
        for asset in &self.asset_policies {
            match &asset.palette_family {
                Some(id) if !families.contains(id.as_str()) => errors.push(format!(
                    "asset '{}' references unknown palette family '{id}'",
                    asset.asset_id
                )),
                None if !families.is_empty() => errors.push(format!(
                    "asset '{}' must name a palette family",
                    asset.asset_id
                )),
                _ => {}
            }
        }
    }
}

fn stable_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

fn digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn validate_size(label: &str, size: [u16; 2], errors: &mut Vec<String>) {
    if size.contains(&0) || size.iter().any(|value| *value > 256) {
        errors.push(format!("{label} dimensions must be in 1..=256"));
    }
}

fn validate_canvas(label: &str, size: [u16; 2], errors: &mut Vec<String>) {
    if size.contains(&0) || size.iter().any(|value| *value > 4096) {
        errors.push(format!("{label} dimensions must be in 1..=4096"));
    }
}

fn validate_range(label: &str, range: [u8; 2], errors: &mut Vec<String>) {
    if range[0] > range[1] {
        errors.push(format!("{label} minimum exceeds maximum"));
    }
}

