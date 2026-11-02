// Effective policy resolution: hard limits → project profile → asset category → defaults.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::visual::VisualIntent;

/// Modern profile contract embedded in visual-intent/v2.
pub const MODERN_PROFILE_CONTRACT: &str = "modern-pixel/v1";

/// Hard limits mirrored from step-300 `pxl-core` (`MAX_DIM`, `MAX_TEXT_COLORS`,
/// `MAX_ANIM_FRAMES`, `MAX_CANVAS`) and `pxl-export` (2048 sheet edge).
/// `pxl-cli` tests assert that both sides stay equal.
pub const MAX_DIM: u16 = 256;
pub const MAX_TEXT_COLORS: usize = 62;
pub const MAX_ANIM_FRAMES: u8 = 16;
pub const MAX_CANVAS: u16 = 4096;
pub const MAX_SHEET: u16 = 2048;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModernProfile {
    pub profile_contract: String,
    pub logical_canvas: [u16; 2],
    pub tile_size: [u16; 2],
    pub target_runtime: String,
    pub sampling: String,
    pub budgets: Budgets,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    pub palette_entries: u16,
    pub frame_opaque_colors: u16,
    pub ramp_depth: u8,
    pub scene_rgb: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectivePolicy {
    pub contract: String,
    pub digest: String,
    pub sources: BTreeMap<String, String>,
    pub canvas: [u16; 2],
    pub tile_size: [u16; 2],
    pub target_runtime: String,
    pub sampling: String,
    pub palette_entries: u16,
    pub frame_opaque_colors: u16,
    pub ramp_depth: u8,
    pub scene_rgb: u16,
    pub max_anim_frames: u8,
}

impl EffectivePolicy {
    pub fn resolve(intent: &VisualIntent, modern: &ModernProfile) -> Result<Self, String> {
        let mut sources = BTreeMap::new();
        if modern.profile_contract != MODERN_PROFILE_CONTRACT {
            return Err(format!(
                "modern profile contract must be {MODERN_PROFILE_CONTRACT}, got '{}'",
                modern.profile_contract
            ));
        }
        if modern.logical_canvas[0] == 0
            || modern.logical_canvas[1] == 0
            || modern.logical_canvas[0] > MAX_CANVAS
            || modern.logical_canvas[1] > MAX_CANVAS
        {
            return Err(format!(
                "logical_canvas must be in 1..={MAX_CANVAS}, got {:?}",
                modern.logical_canvas
            ));
        }
        if modern.tile_size[0] == 0 || modern.tile_size[1] == 0 || modern.tile_size[0] > MAX_DIM || modern.tile_size[1] > MAX_DIM {
            return Err(format!("tile_size must be in 1..={MAX_DIM}, got {:?}", modern.tile_size));
        }
        if !matches!(modern.sampling.as_str(), "nearest_integer") {
            return Err(format!("unsupported sampling '{}'", modern.sampling));
        }
        if modern.budgets.palette_entries == 0 || modern.budgets.palette_entries > MAX_TEXT_COLORS as u16 {
            return Err(format!(
                "palette_entries must be in 1..={}, got {}",
                MAX_TEXT_COLORS,
                modern.budgets.palette_entries
            ));
        }
        if modern.budgets.frame_opaque_colors >= modern.budgets.palette_entries {
            return Err(format!(
                "frame_opaque_colors {} must be less than palette_entries {}",
                modern.budgets.frame_opaque_colors, modern.budgets.palette_entries
            ));
        }
        if modern.budgets.ramp_depth == 0 {
            return Err("ramp_depth must be at least 1".into());
        }
        if modern.budgets.scene_rgb == 0 {
            return Err("scene_rgb must be at least 1".into());
        }
        sources.insert("canvas".into(), "modern_profile".into());
        sources.insert("tile_size".into(), "modern_profile".into());
        sources.insert("target_runtime".into(), "modern_profile".into());
        sources.insert("sampling".into(), "modern_profile".into());
        sources.insert("palette_entries".into(), "modern_profile".into());
        sources.insert("frame_opaque_colors".into(), "modern_profile".into());
        sources.insert("ramp_depth".into(), "modern_profile".into());
        sources.insert("scene_rgb".into(), "modern_profile".into());
        sources.insert("max_anim_frames".into(), "intent".into());

        let policy = EffectivePolicy {
            contract: "effective-policy/v1".into(),
            digest: String::new(),
            sources,
            canvas: modern.logical_canvas,
            tile_size: modern.tile_size,
            target_runtime: modern.target_runtime.clone(),
            sampling: modern.sampling.clone(),
            palette_entries: modern.budgets.palette_entries,
            frame_opaque_colors: modern.budgets.frame_opaque_colors,
            ramp_depth: modern.budgets.ramp_depth,
            scene_rgb: modern.budgets.scene_rgb,
            max_anim_frames: intent.animation_policy.max_frames.min(MAX_ANIM_FRAMES),
        };
        let normalized = serde_json::to_vec(&policy).map_err(|e| format!("policy JSON: {e}"))?;
        let hash = Sha256::digest(&normalized);
        let digest = format!("policy-jcs-sha256:{:x}", hash);
        Ok(EffectivePolicy { digest, ..policy })
    }

    /// The effective QA thresholds derived from this policy: generation,
    /// dry-run and final QA must all read budgets from the same source so a
    /// build can never pass QA under rules it was not authored with.
    ///
    /// The modern profile carries its own budgets, so retro-only checks
    /// (per-8x8 four-colour blocks, NES attribute limits) are not applied.
    pub fn qa_thresholds(&self) -> QaThresholds {
        QaThresholds {
            contract: "qa-thresholds/v1".into(),
            policy_digest: self.digest.clone(),
            palette_entries: self.palette_entries,
            frame_opaque_colors: self.frame_opaque_colors,
            ramp_depth: self.ramp_depth,
            scene_rgb: self.scene_rgb,
            max_anim_frames: self.max_anim_frames,
            retro_block_checks: false,
        }
    }
}

/// Budget snapshot the QA layer enforces. Every field mirrors an
/// [`EffectivePolicy`] budget so the checks stay in sync with the design.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaThresholds {
    pub contract: String,
    pub policy_digest: String,
    pub palette_entries: u16,
    pub frame_opaque_colors: u16,
    pub ramp_depth: u8,
    pub scene_rgb: u16,
    pub max_anim_frames: u8,
    /// Retro-only constraints (per-8x8 four colours, NES attribute limits).
    /// Off for the modern profile; the retro default keeps its old checks.
    pub retro_block_checks: bool,
}

impl QaThresholds {
    /// Retro defaults: legacy documents carry no `modern` block, so QA keeps
    /// the thresholds the pipeline has always enforced.
    pub fn retro_defaults() -> Self {
        Self {
            contract: "qa-thresholds/v1".into(),
            policy_digest: String::new(),
            palette_entries: crate::policy::MAX_TEXT_COLORS as u16,
            frame_opaque_colors: crate::policy::MAX_TEXT_COLORS as u16,
            ramp_depth: 1,
            scene_rgb: crate::policy::MAX_TEXT_COLORS as u16,
            max_anim_frames: crate::policy::MAX_ANIM_FRAMES,
            retro_block_checks: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_modern_profile_resolves() {
        let intent = crate::visual::tests::valid_intent();
        let modern = ModernProfile {
            profile_contract: MODERN_PROFILE_CONTRACT.into(),
            logical_canvas: [640, 360],
            tile_size: [32, 32],
            target_runtime: "runtime-js".into(),
            sampling: "nearest_integer".into(),
            budgets: Budgets {
                palette_entries: 48,
                frame_opaque_colors: 16,
                ramp_depth: 5,
                scene_rgb: 128,
            },
        };
        let policy = EffectivePolicy::resolve(&intent, &modern).unwrap();
        assert_eq!(policy.canvas, [640, 360]);
        assert_eq!(policy.tile_size, [32, 32]);
        assert_eq!(policy.palette_entries, 48);
        assert_eq!(policy.max_anim_frames, 16);
        assert!(policy.digest.starts_with("policy-jcs-sha256:"));
    }

    #[test]
    fn canvas_over_max_canvas_fails() {
        let intent = crate::visual::tests::valid_intent();
        let mut modern = ModernProfile {
            profile_contract: MODERN_PROFILE_CONTRACT.into(),
            logical_canvas: [640, 360],
            tile_size: [32, 32],
            target_runtime: "runtime-js".into(),
            sampling: "nearest_integer".into(),
            budgets: Budgets {
                palette_entries: 48,
                frame_opaque_colors: 16,
                ramp_depth: 5,
                scene_rgb: 128,
            },
        };
        modern.logical_canvas = [4097, 360];
        let error = EffectivePolicy::resolve(&intent, &modern).unwrap_err();
        assert!(error.contains("4096"), "{error}");
    }

    #[test]
    fn palette_entries_over_text_limit_fails() {
        let intent = crate::visual::tests::valid_intent();
        let mut modern = ModernProfile {
            profile_contract: MODERN_PROFILE_CONTRACT.into(),
            logical_canvas: [640, 360],
            tile_size: [32, 32],
            target_runtime: "runtime-js".into(),
            sampling: "nearest_integer".into(),
            budgets: Budgets {
                palette_entries: 48,
                frame_opaque_colors: 16,
                ramp_depth: 5,
                scene_rgb: 128,
            },
        };
        modern.budgets.palette_entries = 63;
        let error = EffectivePolicy::resolve(&intent, &modern).unwrap_err();
        assert!(error.contains("1..=62"), "{error}");
    }

    #[test]
    fn frame_opaque_colors_gte_palette_entries_fails() {
        let intent = crate::visual::tests::valid_intent();
        let mut modern = ModernProfile {
            profile_contract: MODERN_PROFILE_CONTRACT.into(),
            logical_canvas: [640, 360],
            tile_size: [32, 32],
            target_runtime: "runtime-js".into(),
            sampling: "nearest_integer".into(),
            budgets: Budgets {
                palette_entries: 48,
                frame_opaque_colors: 16,
                ramp_depth: 5,
                scene_rgb: 128,
            },
        };
        modern.budgets.frame_opaque_colors = 48;
        let error = EffectivePolicy::resolve(&intent, &modern).unwrap_err();
        assert!(error.contains("less than"), "{error}");
    }
}
