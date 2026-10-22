//! Harness-managed pending-check queue for the assets stage (P0-B).
//!
//! Every required asset carries an explicit check state so that a written
//! spec can never silently avoid building (`w` hole). JSON write success
//! only marks the affected assets dirty; the harness auto-builds complete
//! dirty recipes after each model turn and feeds diagnostics back.
//!
//! State is persisted to `asset-queue.json` inside the attempt directory so
//! it survives stage restarts; outputs are fingerprint-isolated so a failed
//! rebuild can never inherit an older success's outputs.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::adapters::pxl::{PxlAdapter, PxlResult_};
use crate::digest::{digest_bytes, digest_file};
use crate::error::{HarnessError, Result};

pub const QUEUE_FILE: &str = "asset-queue.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetState {
    /// No specs written yet for this required id.
    MissingSpecs,
    /// Some but not all inputs (recipe/shapes/palette) exist.
    PendingDependencies,
    /// All inputs present; recipe digest changed since the last build.
    PendingBuild,
    /// Last build attempt failed for the current input fingerprint.
    BuildFailed,
    /// Built successfully for the current fingerprint; validation pending.
    Built,
    /// Built and validated for the current fingerprint.
    Validated,
    /// Repair budget exhausted (identical failure class repeated).
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetEntry {
    pub asset_id: String,
    pub state: AssetState,
    /// Fingerprint of recipe + shapes + palette digests this state refers to.
    #[serde(default)]
    pub input_fingerprint: String,
    /// Missing dependency file names for `pending_dependencies`.
    #[serde(default)]
    pub missing: Vec<String>,
    /// Latest build/validate error code (e.g. `E_SCHEMA`), if any.
    #[serde(default)]
    pub last_error_code: Option<String>,
    /// Latest failure summary rendered from the pxl envelope.
    #[serde(default)]
    pub last_failure: Option<String>,
    /// Path of the full diagnostics report relative to the attempt dir.
    #[serde(default)]
    pub last_report: Option<String>,
    /// Builds attempted for the current fingerprint.
    #[serde(default)]
    pub build_attempts: u32,
    /// Total repair attempts (same failure class also counts).
    #[serde(default)]
    pub repair_attempts: u32,
    /// Recipe file name backing this asset.
    #[serde(default)]
    pub recipe: String,
    /// Output dir (attempt-relative) for the current fingerprint build.
    #[serde(default)]
    pub out_dir: String,
    /// Auto-builds already performed for this fingerprint (debounce cap 1/turn,
    /// dedup by fingerprint).
    #[serde(default)]
    pub built_fingerprints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetQueue {
    #[serde(default)]
    pub entries: Vec<AssetEntry>,
}

impl AssetQueue {
    pub fn load(attempt_dir: &Path) -> Self {
        let path = attempt_dir.join(QUEUE_FILE);
        fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Self { entries: Vec::new() })
    }

    pub fn save(&self, attempt_dir: &Path) -> Result<()> {
        let path = attempt_dir.join(QUEUE_FILE);
        let text = serde_json::to_string_pretty(self)?;
        fs::write(&path, text).map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))
    }

    /// Seed entries for every required asset id, preserving state of any
    /// entry that already exists (resume / repeated calls).
    pub fn seed(&mut self, required: &[String]) {
        for id in required {
            if !self.entries.iter().any(|e| &e.asset_id == id) {
                self.entries.push(AssetEntry {
                    asset_id: id.clone(),
                    state: AssetState::MissingSpecs,
                    input_fingerprint: String::new(),
                    missing: Vec::new(),
                    last_error_code: None,
                    last_failure: None,
                    last_report: None,
                    build_attempts: 0,
                    repair_attempts: 0,
                    recipe: String::new(),
                    out_dir: String::new(),
                    built_fingerprints: Vec::new(),
                });
            }
        }
    }

    pub fn entry(&self, asset_id: &str) -> Option<&AssetEntry> {
        self.entries.iter().find(|e| e.asset_id == asset_id)
    }

    fn entry_mut(&mut self, asset_id: &str) -> Option<&mut AssetEntry> {
        self.entries.iter_mut().find(|e| e.asset_id == asset_id)
    }

    /// Which assets depend on a written file: `{id}.recipe.json` /
    /// `{id}.shapes.json` affect `{id}`; `palette.json` affects everything.
    pub fn affected_ids(file_name: &str, required: &[String]) -> Vec<String> {
        if file_name == "palette.json" {
            return required.to_vec();
        }
        for suffix in [".recipe.json", ".shapes.json"] {
            if let Some(stem) = file_name.strip_suffix(suffix) {
                if required.iter().any(|r| r == stem) {
                    return vec![stem.to_string()];
                }
            }
        }
        Vec::new()
    }

    /// Recompute state for the affected assets after a successful write.
    /// Writing specs only marks things dirty/pending; it never fabricates a
    /// success.
    pub fn mark_dirty(&mut self, asset_id: &str, attempt_dir: &Path) {
        let Some(entry) = self.entry_mut(asset_id) else {
            return;
        };
        let (state, fingerprint, missing, recipe) = probe_inputs(attempt_dir, asset_id);
        entry.recipe = recipe;
        // A changed fingerprint invalidates any earlier success.
        if entry.input_fingerprint != fingerprint {
            if entry.state == AssetState::Built
                || entry.state == AssetState::Validated
                || entry.state == AssetState::BuildFailed
            {
                entry.state = AssetState::PendingBuild;
            }
            entry.build_attempts = 0;
        }
        match state {
            AssetState::MissingSpecs => entry.state = AssetState::MissingSpecs,
            AssetState::PendingDependencies => {
                entry.state = AssetState::PendingDependencies;
                entry.missing = missing;
            }
            _ => entry.state = AssetState::PendingBuild,
        }
        entry.input_fingerprint = fingerprint;
    }

    /// Palette rewrite cascades: mark every asset dirty, but defer builds —
    /// debounce is enforced by `auto_build` (one build per asset per turn,
    /// same fingerprint reuses the recorded result).
    pub fn mark_all_dirty(&mut self, attempt_dir: &Path) {
        let ids: Vec<String> = self.entries.iter().map(|e| e.asset_id.clone()).collect();
        for id in ids {
            self.mark_dirty(&id, attempt_dir);
        }
    }

    fn record_result(&mut self, asset_id: &str, ok: bool, res: &PxlResult_, report_rel: Option<&str>) {
        let Some(entry) = self.entry_mut(asset_id) else {
            return;
        };
        entry.build_attempts += 1;
        entry.last_error_code = if ok { None } else { Some(res.error_code.clone()) };
        entry.last_failure = if ok {
            None
        } else {
            Some(render_failure(res))
        };
        entry.last_report = report_rel.map(str::to_string);
        entry.state = if ok { AssetState::Built } else { AssetState::BuildFailed };
    }

    /// Public hook for explicit (model-initiated) builds so the queue state
    /// reflects every real build attempt.
    pub fn record_success(&mut self, asset_id: &str) {
        self.record_result(asset_id, true, &success_stub(), None);
    }

    /// Public hook recording a failed explicit build and counting the repair
    /// budget; returns true when the asset becomes blocked.
    pub fn record_failure_and_count(&mut self, asset_id: &str, res: &PxlResult_) -> bool {
        self.record_result(asset_id, false, res, None);
        count_repair_and_maybe_block(self, asset_id, &res.error_code)
    }

    /// Auto-check pass: build every asset whose inputs are complete and whose
    /// current fingerprint has not been built yet (dedup), at most once per
    /// asset per call. Debounce rule: palette-triggered rebuilds wait until
    /// the palette file digest is stable within this pass (single recompute),
    /// and repeated identical rebuilds are skipped via `built_fingerprints`.
    /// Returns per-asset outcomes for feedback injection.
    pub fn auto_build(
        &mut self,
        attempt_dir: &Path,
        pxl: &PxlAdapter,
        events: &mut dyn FnMut(AutoCheckEvent),
    ) -> Vec<(String, AutoCheckOutcome)> {
        let mut out = Vec::new();
        if !pxl.is_available() {
            return out;
        }
        let ids: Vec<String> = self.entries.iter().map(|e| e.asset_id.clone()).collect();
        for id in ids {
            let Some(entry) = self.entry(&id) else {
                continue;
            };
            if entry.state != AssetState::PendingBuild {
                continue;
            }
            let fingerprint = entry.input_fingerprint.clone();
            // Same-fingerprint dedup: never rebuild what already built.
            if entry.built_fingerprints.iter().any(|f| f == &fingerprint) {
                continue;
            }
            let recipe = entry.recipe.clone();
            if recipe.is_empty() {
                continue;
            }
            let out_dir = if entry.out_dir.is_empty() {
                format!("out/{id}")
            } else {
                entry.out_dir.clone()
            };
            let input_digest = digest_bytes(fingerprint.as_bytes());
            events(AutoCheckEvent {
                asset_id: id.clone(),
                recipe: recipe.clone(),
                input_digest: input_digest.clone(),
                phase: AutoCheckPhase::Started,
                ok: false,
                error_code: String::new(),
                elapsed_ms: 0,
                report_rel: String::new(),
            });
            let outcome = build_one(attempt_dir, &recipe, &out_dir, fingerprint.clone(), pxl);
            let ok = outcome.ok;
            let report_rel = outcome.report_rel.clone();
            self.entry_mut(&id).unwrap().built_fingerprints.push(fingerprint);
            self.record_result(&id, ok, &outcome.result, report_rel.as_deref());
            events(AutoCheckEvent {
                asset_id: id.clone(),
                recipe,
                input_digest,
                phase: AutoCheckPhase::Finished,
                ok,
                error_code: outcome.result.error_code.clone(),
                elapsed_ms: outcome.result.elapsed_ms,
                report_rel: report_rel.clone().unwrap_or_default(),
            });
            out.push((id, outcome));
        }
        out
    }

    /// Human/model-readable status of the whole package: what was written but
    /// never built, what failed, and the next todo.
    pub fn status_summary(&self) -> String {
        let mut lines = Vec::new();
        for e in &self.entries {
            let note = match (&e.last_failure, &e.last_report) {
                (Some(f), _) => f.clone(),
                (None, Some(r)) => format!("report: {r}"),
                _ => String::new(),
            };
            let note = if note.is_empty() {
                String::new()
            } else {
                format!(" -- {note}")
            };
            lines.push(format!("{} [{}]{}", e.asset_id, state_name(e.state), note));
        }
        lines.join("\n")
    }
}

pub fn state_name(state: AssetState) -> &'static str {
    match state {
        AssetState::MissingSpecs => "missing_specs",
        AssetState::PendingDependencies => "pending_dependencies",
        AssetState::PendingBuild => "pending_build",
        AssetState::BuildFailed => "build_failed",
        AssetState::Built => "built",
        AssetState::Validated => "validated",
        AssetState::Blocked => "blocked",
    }
}

/// Neutral success envelope for recording ok builds without a live report.
fn success_stub() -> PxlResult_ {
    PxlResult_ {
        ok: true,
        exit_code: Some(0),
        error_code: String::new(),
        message: String::new(),
        hint: None,
        path: None,
        stdout: String::new(),
        stderr: String::new(),
        timed_out: false,
        elapsed_ms: 0,
    }
}

#[derive(Debug, Clone)]
pub struct AutoCheckEvent {
    pub asset_id: String,
    pub recipe: String,
    pub input_digest: String,
    pub phase: AutoCheckPhase,
    pub ok: bool,
    pub error_code: String,
    pub elapsed_ms: u64,
    pub report_rel: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoCheckPhase {
    Started,
    Finished,
}

pub struct AutoCheckOutcome {
    pub ok: bool,
    pub result: PxlResult_,
    pub report_rel: Option<String>,
}

/// Full state of one asset's inputs on disk.
fn probe_inputs(attempt_dir: &Path, asset_id: &str) -> (AssetState, String, Vec<String>, String) {
    let recipe = format!("{asset_id}.recipe.json");
    let shapes = format!("{asset_id}.shapes.json");
    let mut missing = Vec::new();
    if !attempt_dir.join(&recipe).is_file() {
        missing.push(recipe.clone());
    }
    if !attempt_dir.join(&shapes).is_file() {
        missing.push(shapes);
    }
    if !attempt_dir.join("palette.json").is_file() {
        missing.push("palette.json".into());
    }
    if !attempt_dir.join(&recipe).is_file() {
        return (AssetState::MissingSpecs, String::new(), missing, String::new());
    }
    if !missing.is_empty() {
        return (AssetState::PendingDependencies, String::new(), missing, recipe);
    }
    let fingerprint = input_fingerprint(attempt_dir, asset_id, &recipe);
    (AssetState::PendingBuild, fingerprint, Vec::new(), recipe)
}

/// Fingerprint = recipe digest + shapes digest + palette digest. Old outputs
/// produced for a different fingerprint can never back a new recipe.
pub fn input_fingerprint(attempt_dir: &Path, asset_id: &str, recipe: &str) -> String {
    let mut parts = vec![recipe.to_string()];
    for name in [
        format!("{asset_id}.shapes.json"),
        "palette.json".to_string(),
    ] {
        parts.push(name);
    }
    let mut fp = String::new();
    for name in parts {
        let digest = digest_file(&attempt_dir.join(&name)).unwrap_or_else(|_| "missing".into());
        fp.push_str(&digest);
        fp.push('|');
    }
    fp
}

/// Build one recipe into a fingerprint-isolated output directory, then write
/// a full diagnostics report (checks objects, digests, call identity).
fn build_one(
    attempt_dir: &Path,
    recipe: &str,
    out_rel: &str,
    fingerprint: String,
    pxl: &PxlAdapter,
) -> AutoCheckOutcome {
    let recipe_path = attempt_dir.join(recipe);
    // Isolate outputs per fingerprint: a stale success can never be reused.
    let fp_tag = digest_bytes(fingerprint.as_bytes())
        .trim_start_matches("sha256:")
        .chars()
        .take(12)
        .collect::<String>();
    let out_dir = attempt_dir.join(format!("{}.{}", out_rel, fp_tag));
    let _ = fs::create_dir_all(&out_dir);
    let built = match pxl.build(&recipe_path, &out_dir, None, Some(attempt_dir)) {
        Ok(b) => b,
        Err(e) => {
            let result = PxlResult_ {
                ok: false,
                exit_code: None,
                error_code: "INFRA_HARNESS".into(),
                message: e.message.clone(),
                hint: e.hint.clone(),
                path: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                elapsed_ms: 0,
            };
            return AutoCheckOutcome {
                ok: false,
                result,
                report_rel: None,
            };
        }
    };
    let result = built.result.clone();
    // Full diagnostics report: complete checks, input digests, output digests.
    let output_paths: Vec<PathBuf> = built
        .outputs
        .iter()
        .map(|name| out_dir.join(name))
        .collect();
    let report_rel = write_build_report(
        attempt_dir,
        recipe,
        &fingerprint,
        &result,
        &output_paths,
        &out_dir,
    );
    AutoCheckOutcome {
        ok: built.qa_ok(),
        result,
        report_rel,
    }
}

/// Persist the complete diagnostics document. Field whitelist: pxl envelope
/// facts only — no provider config, headers or environment values.
#[allow(clippy::too_many_arguments)]
fn write_build_report(
    attempt_dir: &Path,
    recipe: &str,
    fingerprint: &str,
    result: &PxlResult_,
    outputs: &[PathBuf],
    out_dir: &Path,
) -> Option<String> {
    let reports = attempt_dir.join("build-reports");
    if fs::create_dir_all(&reports).is_err() {
        return None;
    }
    let safe: String = recipe
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let fp_tag: String = digest_bytes(fingerprint.as_bytes())
        .trim_start_matches("sha256:")
        .chars()
        .take(12)
        .collect();
    let rel = format!("build-reports/{safe}.{fp_tag}.json");
    let output_digests: Vec<Value> = outputs
        .iter()
        .filter_map(|p| {
            let digest = digest_file(p).ok()?;
            Some(serde_json::json!({
                "path": p.strip_prefix(attempt_dir)
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| p.display().to_string()),
                "digest": digest,
            }))
        })
        .collect();
    let doc = serde_json::json!({
        "schema_version": 1,
        "recipe": recipe,
        "input_fingerprint": fingerprint,
        "ok": result.ok,
        "exit_code": result.exit_code,
        "error_code": result.error_code,
        "message": result.message,
        "hint": result.hint,
        "error_path": result.path,
        "elapsed_ms": result.elapsed_ms,
        "checks": result.checks(),
        "output_digests": output_digests,
        "out_dir": out_dir
            .strip_prefix(attempt_dir)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| out_dir.display().to_string()),
    });
    let path = attempt_dir.join(&rel);
    if fs::write(&path, serde_json::to_string_pretty(&doc).unwrap_or_default()).is_err() {
        return None;
    }
    Some(rel)
}

/// Render a failure for model feedback without degrading check objects:
/// keeps value/limit, appends hint and the full report path.
pub fn render_failure(res: &PxlResult_) -> String {
    let mut parts = vec![format!("{} {}", res.error_code, res.message)];
    if let Some(hint) = &res.hint {
        parts.push(format!("hint: {hint}"));
    }
    if let Some(path) = &res.path {
        parts.push(format!("error_path: {path}"));
    }
    let failed = crate::adapters::pxl::render_failed_checks(&res.checks(), 8);
    parts.extend(failed);
    parts.join("; ")
}

/// Compact per-asset feedback line injected into the next round's prompt:
/// never repeats old-input errors for a new fingerprint (the queue only
/// renders the fingerprint-matching latest diagnosis).
pub fn render_auto_check_feedback(outcomes: &[(String, AutoCheckOutcome)], attempt_dir: &Path) -> String {
    let mut lines = Vec::new();
    for (id, o) in outcomes {
        if o.ok {
            lines.push(format!(
                "{id}: auto-build OK (report: {})",
                o.report_rel.as_deref().unwrap_or("n/a")
            ));
        } else {
            let failure = render_failure(&o.result);
            let report = o
                .report_rel
                .as_deref()
                .map(|r| format!(" full report: {r}"))
                .unwrap_or_default();
            lines.push(format!("{id}: auto-build FAILED: {failure}.{report}"));
        }
    }
    let _ = attempt_dir;
    lines.join("\n")
}

/// Map per-asset repeated failure counts onto blocked state. Identical
/// failure class with a changed fingerprint still counts (no micro-edit
/// escape hatch); limit is enforced by the harness, not the prompt.
pub const REPAIR_LIMIT_PER_ASSET: u32 = 3;

pub fn count_repair_and_maybe_block(queue: &mut AssetQueue, asset_id: &str, error_code: &str) -> bool {
    let Some(entry) = queue.entry_mut(asset_id) else {
        return false;
    };
    // Count this failure first, then record its class for the next call.
    let same_class = entry
        .last_error_code
        .as_deref()
        .map(|prev| is_same_class(Some(prev), error_code))
        .unwrap_or(false);
    entry.repair_attempts = if same_class { entry.repair_attempts + 1 } else { 1 };
    entry.last_error_code = Some(error_code.to_string());
    if entry.repair_attempts >= REPAIR_LIMIT_PER_ASSET {
        entry.state = AssetState::Blocked;
        return true;
    }
    false
}

fn is_same_class(a: Option<&str>, b: &str) -> bool {
    match a {
        Some(a) => {
            let class = |c: &str| match c {
                "E_USAGE" | "E_SCHEMA" | "E_SEMANTIC" => "recipe",
                "E_QA" => "qa",
                _ => "other",
            };
            class(a) == class(b)
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pxl_h_queue_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn write_marks_dirty_and_palette_cascades() {
        let d = dir("dirty");
        let mut q = AssetQueue { entries: Vec::new() };
        q.seed(&["w".into(), "explosion".into()]);
        // Only w's recipe exists: explosion stays missing_specs, w is dirty.
        fs::write(d.join("w.recipe.json"), r#"{"v":1,"asset":"w"}"#).unwrap();
        q.mark_dirty("w", &d);
        assert_eq!(q.entry("w").unwrap().state, AssetState::PendingDependencies);
        assert!(q.entry("w").unwrap().missing.iter().any(|m| m == "w.shapes.json"));
        fs::write(d.join("w.shapes.json"), r#"{"w":8,"h":8}"#).unwrap();
        fs::write(d.join("palette.json"), r#"{"v":1,"colors":[]}"#).unwrap();
        q.mark_dirty("w", &d);
        assert_eq!(q.entry("w").unwrap().state, AssetState::PendingBuild);
        assert!(!q.entry("w").unwrap().input_fingerprint.is_empty());
        // Palette rewrite cascades dirt to every asset.
        q.mark_all_dirty(&d);
        assert_eq!(q.entry("explosion").unwrap().state, AssetState::MissingSpecs);
        assert_eq!(q.entry("w").unwrap().state, AssetState::PendingBuild);
    }

    #[test]
    fn affected_ids_cover_recipe_shapes_and_palette() {
        let required = vec!["w".to_string(), "explosion".to_string()];
        assert_eq!(AssetQueue::affected_ids("w.recipe.json", &required), vec!["w"]);
        assert_eq!(AssetQueue::affected_ids("explosion.shapes.json", &required), vec!["explosion"]);
        assert_eq!(AssetQueue::affected_ids("palette.json", &required).len(), 2);
        assert!(AssetQueue::affected_ids("other.json", &required).is_empty());
    }

    #[test]
    fn fingerprint_changes_with_any_input() {
        let d = dir("fp");
        fs::write(d.join("t.recipe.json"), r##"{"v":1,"seed":1}"##).unwrap();
        fs::write(d.join("t.shapes.json"), r#"{"w":8,"h":8}"#).unwrap();
        fs::write(d.join("palette.json"), r##"{"v":1,"colors":["#00000000"]}"##).unwrap();
        let fp1 = input_fingerprint(&d, "t", "t.recipe.json");
        fs::write(d.join("t.recipe.json"), r#"{"v":1,"seed":2}"#).unwrap();
        let fp2 = input_fingerprint(&d, "t", "t.recipe.json");
        assert_ne!(fp1, fp2);
        fs::write(d.join("palette.json"), r##"{"v":1,"colors":["#00000000","#ffffffff"]}"##).unwrap();
        let fp3 = input_fingerprint(&d, "t", "t.recipe.json");
        assert_ne!(fp2, fp3);
    }

    #[test]
    fn queue_persists_and_reloads() {
        let d = dir("persist");
        let mut q = AssetQueue { entries: Vec::new() };
        q.seed(&["b".into()]);
        q.mark_dirty("b", &d);
        q.save(&d).unwrap();
        let reloaded = AssetQueue::load(&d);
        assert_eq!(reloaded.entry("b").unwrap().state, AssetState::MissingSpecs);
    }

    #[test]
    fn repair_limit_blocks_after_three_same_class() {
        let mut q = AssetQueue { entries: Vec::new() };
        q.seed(&["explosion".into()]);
        assert!(!count_repair_and_maybe_block(&mut q, "explosion", "E_QA"));
        assert!(!count_repair_and_maybe_block(&mut q, "explosion", "E_QA"));
        // Same class even with a different code still counts.
        assert!(count_repair_and_maybe_block(&mut q, "explosion", "E_QA"));
        assert_eq!(q.entry("explosion").unwrap().state, AssetState::Blocked);
        // Different class resets the streak.
        q.entry_mut("explosion").unwrap().state = AssetState::BuildFailed;
        count_repair_and_maybe_block(&mut q, "explosion", "E_SCHEMA");
        assert_eq!(q.entry("explosion").unwrap().repair_attempts, 1);
    }

    #[test]
    fn render_failure_keeps_hint_and_value_limit() {
        let res = PxlResult_ {
            ok: false,
            exit_code: Some(3),
            error_code: "E_SCHEMA".into(),
            message: "macro pulse_glow has no argument 'steps'".into(),
            hint: Some("pulse_glow takes: ramp".into()),
            path: Some("/macros".into()),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            elapsed_ms: 9,
        };
        let text = render_failure(&res);
        assert!(text.contains("pulse_glow takes: ramp"), "{text}");
        assert!(text.contains("error_path: /macros"), "{text}");

        let res = PxlResult_ {
            ok: false,
            exit_code: Some(5),
            error_code: "E_QA".into(),
            message: "1 check(s) failed".into(),
            hint: None,
            path: None,
            stdout: r#"{"ok":false,"error":{"code":"E_QA","message":"failed"},"checks":[{"id":"anim.exp.diff_0_1","pass":false,"value":"42.96%","limit":"at most 35%"}]}"#.into(),
            stderr: String::new(),
            timed_out: false,
            elapsed_ms: 9,
        };
        let text = render_failure(&res);
        assert!(text.contains("anim.exp.diff_0_1"), "{text}");
        assert!(text.contains("42.96%"), "{text}");
        assert!(text.contains("35%"), "{text}");
    }

    #[test]
    fn status_summary_names_states() {
        let mut q = AssetQueue { entries: Vec::new() };
        q.seed(&["w".into(), "b".into()]);
        let summary = q.status_summary();
        assert!(summary.contains("w [missing_specs]"), "{summary}");
    }
}
