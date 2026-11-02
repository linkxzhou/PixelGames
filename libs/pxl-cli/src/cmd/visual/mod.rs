use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use pxl_contract::plan::VisualPlan;
use pxl_contract::visual::VisualIntent;
use pxl_core::{Palette, PxlError};
use serde_json::Value;

use crate::error::CmdResult;
use crate::util;


mod apply;
mod helpers;

pub(crate) use helpers::palette_bytes;

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


}

#[cfg(test)]
mod tests;
