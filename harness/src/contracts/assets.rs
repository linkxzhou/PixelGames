//! Asset package contracts: requirements come from the design stage; the
//! manifest describes what was actually delivered.

use serde::{Deserialize, Serialize};

use super::{stable_id, ContractHeader};

pub const ASSETS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetManifest {
    #[serde(flatten)]
    pub header: ContractHeader,
    pub design_revision: String,
    pub assets_revision: String,
    pub pxl_version: String,
    pub pxl_binary_digest: String,
    #[serde(default)]
    pub entries: Vec<ManifestEntry>,
    /// Gates that passed for this revision.
    #[serde(default)]
    pub gates_passed: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// `not_run` unless an image-capable provider reviewed the previews.
    pub visual_review: VisualReviewState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub asset_id: String,
    pub status: DeliveryStatus,
    pub required: bool,
    /// Recipe + spec paths (relative to the revision) with digests.
    pub recipe_path: String,
    pub recipe_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spec_paths: Vec<(String, String)>,
    pub palette_path: String,
    pub palette_digest: String,
    pub seed: u64,
    /// Output files with digests (relative to the revision).
    #[serde(default)]
    pub outputs: Vec<(String, String)>,
    /// Frame keys inside the atlas for this asset, per anim/direction.
    #[serde(default)]
    pub animations: Vec<ManifestAnim>,
    pub size_px: [u32; 2],
    /// Explicit gameplay metadata from the design, never atlas-derived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot: Option<[i32; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestAnim {
    pub name: String,
    pub fps: u32,
    pub looped: bool,
    /// dir -> atlas frame keys.
    #[serde(default)]
    pub dirs: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Delivered,
    Missing,
    Failed,
    /// Declared external dependency (sound, fonts): not generated here.
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualReviewState {
    NotRun,
    Reviewed,
}

pub fn validate_manifest_ids(m: &AssetManifest) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for e in &m.entries {
        if !stable_id(&e.asset_id) {
            errs.push(format!("asset id '{}' is not stable", e.asset_id));
        }
        if !seen.insert(e.asset_id.clone()) {
            errs.push(format!("duplicate asset id '{}'", e.asset_id));
        }
        for (dir, frames) in e
            .animations
            .iter()
            .flat_map(|a| a.dirs.iter())
        {
            if frames.is_empty() {
                errs.push(format!(
                    "asset '{}' anim dir '{}' has no frames",
                    e.asset_id, dir
                ));
            }
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_duplicate_ids_rejected() {
        let entry = ManifestEntry {
            asset_id: "tank".into(),
            status: DeliveryStatus::Delivered,
            required: true,
            recipe_path: "units.recipe.json".into(),
            recipe_digest: "sha256:x".into(),
            spec_paths: vec![],
            palette_path: "palette.json".into(),
            palette_digest: "sha256:p".into(),
            seed: 1,
            outputs: vec![],
            animations: vec![],
            size_px: [16, 16],
            pivot: None,
        };
        let m = AssetManifest {
            header: ContractHeader {
                schema_version: ASSETS_SCHEMA_VERSION,
            },
            design_revision: "d1".into(),
            assets_revision: "a1".into(),
            pxl_version: "0.1.0".into(),
            pxl_binary_digest: "sha256:b".into(),
            entries: vec![entry.clone(), entry],
            gates_passed: vec![],
            warnings: vec![],
            visual_review: VisualReviewState::NotRun,
        };
        assert!(validate_manifest_ids(&m).is_err());
    }
}
