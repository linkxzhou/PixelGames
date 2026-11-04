use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use pxl_contract::error::{HarnessError, Result};
use pxl_contract::Verdict;
use serde_json::Value;

pub fn review_verdict(dir: &Path) -> Result<Option<Verdict>> {
    let path = dir.join("visual-review.json");
    if !path.is_file() {
        return Ok(None);
    }
    let review: Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .map_err(|e| HarnessError::io(format!("visual-review.json: {e}")))?,
    )
    .map_err(|e| HarnessError::schema(format!("visual-review.json: {e}")))?;
    if review["contract"] != "visual-review/v1" {
        return Err(HarnessError::integrity(
            "visual review must use visual-review/v1",
        ));
    }
    if review["scale_version"].as_str().unwrap_or_default().is_empty() {
        return Err(HarnessError::integrity(
            "visual review scale_version is required",
        ));
    }
    let current = super::run::recompute_revision(dir)?;
    if review["revision_digest"] != current {
        return Err(HarnessError::integrity(
            "visual review belongs to another revision",
        ));
    }
    let reviewers = review["reviewers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|reviewer| reviewer["id"].as_str().map(str::to_string))
        .collect::<BTreeSet<_>>();
    if reviewers.len() < 2 {
        return Err(HarnessError::integrity(
            "visual review needs at least two independent reviewers",
        ));
    }
    let package: Value = serde_json::from_str(
        &fs::read_to_string(dir.join("asset-package.json"))
            .map_err(|e| HarnessError::io(format!("asset-package.json: {e}")))?,
    )
    .map_err(|e| HarnessError::schema(format!("asset-package.json: {e}")))?;
    if review["subject_digest"] != package["visual"]["report"]["digest"] {
        return Err(HarnessError::integrity(
            "visual review subject digest does not match visual-report.json",
        ));
    }
    let mut required = vec![
        "silhouette",
        "colour_value",
        "material_form",
        "pixel_clusters",
    ];
    let assets = package["assets"].as_array().cloned().unwrap_or_default();
    if assets.iter().any(|asset| asset["kind"] == "animated") {
        required.push("animation");
    }
    if assets.iter().any(|asset| asset["kind"] == "tile") {
        required.push("tile_scene");
    }
    if assets.iter().any(|asset| asset["role"] == "ui") {
        required.push("ui");
    }
    let dimensions = review["dimensions"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    for id in required {
        let Some(dimension) = dimensions.get(id) else {
            return Err(HarnessError::integrity(format!(
                "visual review is missing dimension '{id}'"
            )));
        };
        let status = dimension["status"].as_str().unwrap_or_default();
        if !matches!(status, "pass" | "concerns" | "not_applicable") {
            return Err(HarnessError::integrity(format!(
                "visual review dimension '{id}' has invalid status"
            )));
        }
        if status == "not_applicable" {
            if dimension["reason"].as_str().unwrap_or_default().is_empty() {
                return Err(HarnessError::integrity(format!(
                    "visual review dimension '{id}' needs a not_applicable reason"
                )));
            }
            continue;
        }
        if dimension["score"].as_u64().filter(|score| (1..=5).contains(score)).is_none() {
            return Err(HarnessError::integrity(format!(
                "visual review dimension '{id}' needs a 1..=5 score"
            )));
        }
        if dimension["evidence"].as_array().map(Vec::is_empty) != Some(false) {
            return Err(HarnessError::integrity(format!(
                "visual review dimension '{id}' needs evidence"
            )));
        }
    }
    for exception in review["exceptions"].as_array().cloned().unwrap_or_default() {
        for field in ["rule", "target", "reason", "evidence", "accepted_revision"] {
            if exception[field].as_str().unwrap_or_default().is_empty() {
                return Err(HarnessError::integrity(
                    "visual review exceptions need rule/target/reason/evidence/accepted_revision",
                ));
            }
        }
        if exception["accepted_revision"] != current {
            return Err(HarnessError::integrity(
                "visual review exception belongs to another revision",
            ));
        }
    }
    match review["verdict"].as_str().unwrap_or_default() {
        "PASS" => Ok(Some(Verdict::Pass)),
        "CONCERNS" => Ok(Some(Verdict::Concerns)),
        _ => Err(HarnessError::integrity(
            "visual review verdict must be PASS or CONCERNS",
        )),
    }
}
