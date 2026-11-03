use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::visual::{FinishPolicy, LightingMode, VisualIntent};

pub const VISUAL_PLAN_CONTRACT: &str = "visual-plan/v1";
/// v2 adds per-asset palette family binding and the resolved effective
/// policy digest. Both fields are rejected in v1 plans.
pub const VISUAL_PLAN_CONTRACT_V2: &str = "visual-plan/v2";

/// The only region transform step 300 applies to material and protection masks.
pub const REGION_TRANSFORM_TRANSLATE: &str = "translate";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualPlan {
    pub contract: String,
    pub intent_digest: String,
    pub assets: Vec<AssetPlan>,
    pub previews: Vec<PreviewPlan>,
    /// The effective policy digest (`policy-jcs-sha256:...`) the plan was
    /// authored against. v2 only; absent in v1 plans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_policy_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetPlan {
    pub asset_id: String,
    pub role: String,
    pub native_size: [u16; 2],
    pub finish: FinishPolicy,
    pub lighting: LightingMode,
    pub regions: Vec<RegionPlan>,
    pub animations: Vec<AnimationPlan>,
    pub tile: Option<TilePlan>,
    pub channels: ChannelPlan,
    /// Palette family this asset builds with. v2 only; must match the
    /// intent's asset policy when both are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_family: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionPlan {
    pub region_id: String,
    pub material_id: String,
    pub ramp: String,
    pub light_role: String,
    pub protect: bool,
    pub source: RegionSource,
    pub mask: Option<MaskPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionSource {
    pub kind: String,
    pub selector: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaskPlan {
    pub path: String,
    pub size: [u16; 2],
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationPlan {
    pub anim_id: String,
    pub anchor: [i32; 2],
    pub contact: [i32; 2],
    pub allow_squash: bool,
    pub allow_smear: bool,
    pub terminal_empty: bool,
    pub phases: Vec<AnimationPhase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationPhase {
    pub phase_id: String,
    pub frames: Vec<String>,
    pub duration_ms: Vec<u32>,
    pub transform: Option<RegionTransform>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionTransform {
    pub kind: String,
    pub offset: [i32; 2],
    pub rotate90: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TilePlan {
    pub family: String,
    pub neighbours: Vec<String>,
    pub variant_seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelPlan {
    pub material_labels: bool,
    pub emissive: bool,
    pub shadow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewPlan {
    pub scene_id: String,
    pub assets: Vec<String>,
}

impl VisualPlan {
    pub fn is_v2(&self) -> bool {
        self.contract == VISUAL_PLAN_CONTRACT_V2
    }

    pub fn validate(&self, intent: &VisualIntent) -> Vec<String> {
        let mut errors = Vec::new();
        let v2 = self.is_v2();
        if self.contract != VISUAL_PLAN_CONTRACT && !v2 {
            errors.push(format!(
                "contract must be {VISUAL_PLAN_CONTRACT} or {VISUAL_PLAN_CONTRACT_V2}, got '{}'",
                self.contract
            ));
        }
        if !valid_digest(&self.intent_digest) {
            errors.push("intent_digest must be a sha256 digest".into());
        }
        // v1 plans carry no v2 fields; v2 plans record the resolved policy
        // digest exactly when the intent resolved a modern policy.
        if !v2 {
            if self.effective_policy_digest.is_some() || self.assets.iter().any(|asset| asset.palette_family.is_some()) {
                errors.push(format!(
                    "effective_policy_digest and asset palette_family need {VISUAL_PLAN_CONTRACT_V2}"
                ));
            }
        } else {
            match (&self.effective_policy_digest, &intent.modern) {
                (Some(digest), Some(modern)) => {
                    match crate::policy::EffectivePolicy::resolve(intent, modern) {
                        Ok(policy) if &policy.digest == digest => {}
                        Ok(policy) => errors.push(format!(
                            "effective_policy_digest is {digest} but the intent resolves to {}",
                            policy.digest
                        )),
                        Err(error) => errors.push(format!("effective policy: {error}")),
                    }
                }
                (Some(_), None) => {
                    errors.push("plan records an effective_policy_digest but the intent has no modern profile".into())
                }
                (None, Some(_)) => {
                    errors.push("v2 plan must record the intent's effective_policy_digest".into())
                }
                (None, None) => {}
            }
        }
        if self.assets.len() != intent.asset_policies.len() {
            errors.push("every visual intent asset must have exactly one plan".into());
        }
        let mut asset_ids = BTreeSet::new();
        for asset in &self.assets {
            if !asset_ids.insert(asset.asset_id.as_str()) {
                errors.push(format!("duplicate asset plan '{}'", asset.asset_id));
                continue;
            }
            let Some(policy) = intent
                .asset_policies
                .iter()
                .find(|policy| policy.asset_id == asset.asset_id)
            else {
                errors.push(format!("unknown asset plan '{}'", asset.asset_id));
                continue;
            };
            if asset.role != policy.role
                || asset.finish != policy.finish
                || asset.lighting != policy.lighting
                || asset.native_size != policy.native_size.unwrap_or(intent.pixel_grid.native_size)
            {
                errors.push(format!("asset '{}' differs from the frozen intent", asset.asset_id));
            }
            let mut region_ids = BTreeSet::new();
            for region in &asset.regions {
                if !region_ids.insert(region.region_id.as_str()) {
                    errors.push(format!("asset '{}' has duplicate region '{}'", asset.asset_id, region.region_id));
                }
                if !policy.material_ids.contains(&region.material_id) {
                    errors.push(format!("asset '{}' region '{}' uses an undeclared material", asset.asset_id, region.region_id));
                }
                let Some(material) = intent.materials.get(&region.material_id) else {
                    continue;
                };
                if !material.regions.contains(&region.region_id) {
                    errors.push(format!("material '{}' has no region '{}'", region.material_id, region.region_id));
                }
                if material.ramp != region.ramp {
                    errors.push(format!("asset '{}' region '{}' changes the frozen ramp", asset.asset_id, region.region_id));
                }
                if region.protect != policy.protected_regions.contains(&region.region_id) {
                    errors.push(format!("asset '{}' region '{}' has inconsistent protect policy", asset.asset_id, region.region_id));
                }
                if !matches!(region.source.kind.as_str(), "ascii" | "shape" | "svg" | "mask") {
                    errors.push(format!("asset '{}' region '{}' has unknown source kind", asset.asset_id, region.region_id));
                }
                if region.source.selector.trim().is_empty() {
                    errors.push(format!("asset '{}' region '{}' has no source selector", asset.asset_id, region.region_id));
                }
                if region.source.kind == "mask" {
                    match &region.mask {
                        Some(mask) if mask.size == asset.native_size && valid_digest(&mask.digest) && !mask.path.is_empty() => {}
                        _ => errors.push(format!("asset '{}' region '{}' needs an aligned mask and digest", asset.asset_id, region.region_id)),
                    }
                } else if region.mask.is_some() {
                    errors.push(format!("asset '{}' region '{}' has a mask without mask source", asset.asset_id, region.region_id));
                }
            }
            for region in &policy.protected_regions {
                if !region_ids.contains(region.as_str()) {
                    errors.push(format!("asset '{}' is missing protected region '{region}'", asset.asset_id));
                }
            }
            for animation in &asset.animations {
                let mut frames = 0usize;
                let mut phase_ids = BTreeSet::new();
                for phase in &animation.phases {
                    if !phase_ids.insert(phase.phase_id.as_str())
                        || phase.frames.is_empty()
                        || phase.frames.len() != phase.duration_ms.len()
                        || phase.duration_ms.contains(&0)
                    {
                        errors.push(format!("asset '{}' animation '{}' has invalid phases", asset.asset_id, animation.anim_id));
                    }
                    frames += phase.frames.len();
                    if let Some(transform) = &phase.transform {
                        // Step 300 moves material and protection masks by whole
                        // pixels only. Anything else would be accepted here and
                        // fail at build time, so it is refused at the earliest point.
                        if transform.kind != REGION_TRANSFORM_TRANSLATE || transform.rotate90 != 0 {
                            errors.push(format!(
                                "asset '{}' animation '{}' phase '{}' uses region transform '{}' (rotate90={}); only '{}' with rotate90=0 is executed by step 300",
                                asset.asset_id, animation.anim_id, phase.phase_id,
                                transform.kind, transform.rotate90, REGION_TRANSFORM_TRANSLATE
                            ));
                        }
                    }
                }
                if frames == 0 || frames > usize::from(intent.animation_policy.max_frames) {
                    errors.push(format!("asset '{}' animation '{}' exceeds the frame budget", asset.asset_id, animation.anim_id));
                }
                if animation.allow_squash && !intent.animation_policy.allow_squash
                    || animation.allow_smear && !intent.animation_policy.allow_smear
                {
                    errors.push(format!("asset '{}' animation '{}' uses an undeclared deformation", asset.asset_id, animation.anim_id));
                }
            }
            if policy.kind == "tile" {
                match &asset.tile {
                    Some(tile) if !tile.family.is_empty() && !tile.neighbours.is_empty() => {}
                    _ => errors.push(format!("tile asset '{}' needs a family and neighbours", asset.asset_id)),
                }
            } else if asset.tile.is_some() {
                errors.push(format!("non-tile asset '{}' has tile policy", asset.asset_id));
            }
            // v2: the plan's family binding must equal the intent's. A v1
            // plan under a v2 intent is tolerated (single-palette legacy);
            // a v2 plan must never disagree with the frozen intent.
            if v2 && asset.palette_family != policy.palette_family {
                errors.push(format!(
                    "asset '{}' palette family is {:?} but the intent froze {:?}",
                    asset.asset_id, asset.palette_family, policy.palette_family
                ));
            }
        }
        for preview in &self.previews {
            let Some(scene) = intent.scene_previews.iter().find(|scene| scene.id == preview.scene_id) else {
                errors.push(format!("unknown preview scene '{}'", preview.scene_id));
                continue;
            };
            for id in &preview.assets {
                if !scene.instances.iter().any(|instance| instance.asset_id == *id)
                    || !asset_ids.contains(id.as_str())
                {
                    errors.push(format!("preview '{}' references invalid asset '{id}'", preview.scene_id));
                }
            }
        }
        errors
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual::tests::valid_intent;

    fn plan_with_transform(transform: &str) -> VisualPlan {
        let json = format!(
            r#"{{
              "contract": "visual-plan/v1",
              "intent_digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
              "assets": [{{
                "asset_id": "hero", "role": "actor", "native_size": [16, 16],
                "finish": "keep", "lighting": "unlit",
                "regions": [
                  {{ "region_id": "body", "material_id": "flat", "ramp": "main", "light_role": "base", "protect": false, "source": {{ "kind": "shape", "selector": "hero.shapes.json#body" }}, "mask": null }},
                  {{ "region_id": "eye", "material_id": "flat", "ramp": "main", "light_role": "base", "protect": true, "source": {{ "kind": "shape", "selector": "hero.shapes.json#eye" }}, "mask": null }}
                ],
                "animations": [{{
                  "anim_id": "walk", "anchor": [8, 15], "contact": [8, 15],
                  "allow_squash": false, "allow_smear": false, "terminal_empty": false,
                  "phases": [{{ "phase_id": "contact", "frames": ["hero_walk_0"], "duration_ms": [100], "transform": {transform} }}]
                }}],
                "tile": null,
                "channels": {{ "material_labels": true, "emissive": false, "shadow": false }}
              }}],
              "previews": [{{ "scene_id": "main", "assets": ["hero"] }}]
            }}"#
        );
        serde_json::from_str(&json).expect("fixture plan parses")
    }

    fn errors_for(transform: &str) -> Vec<String> {
        plan_with_transform(transform).validate(&valid_intent())
    }

    #[test]
    fn translate_without_rotation_is_accepted() {
        let errors = errors_for(r#"{ "kind": "translate", "offset": [0, 1], "rotate90": 0 }"#);
        assert_eq!(errors, Vec::<String>::new());
    }

    #[test]
    fn transforms_step_300_does_not_execute_are_rejected_early() {
        for kind in ["rotate", "squash", "smear"] {
            let transform = format!(r#"{{ "kind": "{kind}", "offset": [0, 0], "rotate90": 0 }}"#);
            let errors = errors_for(&transform);
            assert!(
                errors.iter().any(|e| e.contains(kind) && e.contains("translate")),
                "{kind} should be rejected with a pointer to translate, got {errors:?}"
            );
        }
    }

    #[test]
    fn translate_with_rotation_is_rejected() {
        let errors = errors_for(r#"{ "kind": "translate", "offset": [0, 0], "rotate90": 1 }"#);
        assert!(errors.iter().any(|e| e.contains("rotate90=1")), "got {errors:?}");
    }
}
