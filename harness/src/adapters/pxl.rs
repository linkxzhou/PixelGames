//! Typed `PxlAdapter`. The agent never sees shell strings; it calls domain
//! tools like `assets.build_candidate` which route here.
//!
//! Contract notes verified against the real CLI:
//! - `pxl --json ...` prints one JSON object on stdout; logs go to stderr.
//! - `build` may leave outputs even when QA fails (exit 5): callers must
//!   treat the attempt dir as partial until checks pass.
//! - `schema` prints a *bare* JSON schema (no ok/error wrapper).
//! - exit codes: 1 E_IO, 2 E_USAGE, 3 E_SCHEMA, 4 E_SEMANTIC, 5 E_QA,
//!   6 E_EXISTS; timeouts / non-JSON are infrastructure errors.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::adapters::process::{run, ProcOutput, ProcSpec};
use crate::digest::{digest_file, sha256_hex};
use crate::error::{HarnessError, Result};

/// Normalized pxl CLI outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxlResult_ {
    pub ok: bool,
    pub exit_code: Option<i32>,
    /// `E_USAGE` etc., or `INFRA_*` for timeout / non-JSON / spawn errors.
    pub error_code: String,
    pub message: String,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone)]
pub struct PxlAdapter {
    bin: PathBuf,
    version: Option<String>,
    binary_digest: Option<String>,
    timeout_secs: u64,
    max_output_bytes: usize,
}

impl PxlAdapter {
    /// Resolve and verify the approved binary. No auto-install, ever.
    pub fn discover(cfg: &crate::config::Config) -> Result<Self> {
        let bin = if cfg.pxl_bin.is_empty() {
            which_pxl()?
        } else {
            PathBuf::from(&cfg.pxl_bin)
        };
        if !bin.is_file() {
            return Err(HarnessError::not_found(format!(
                "pxl binary {} does not exist",
                bin.display()
            ))
            .with_hint("build the workspace or set pxl_bin in harness.config.json"));
        }
        let bytes = fs::read(&bin)
            .map_err(|e| HarnessError::io(format!("{}: {e}", bin.display())))?;
        let digest = format!("sha256:{}", sha256_hex(&bytes));
        if let Some(expected) = &cfg.pxl_binary_digest {
            if *expected != digest {
                return Err(HarnessError::integrity(format!(
                    "pxl binary digest {digest} != configured {expected}"
                ))
                .with_hint("rebuild pxl or update pxl_binary_digest after review"));
            }
        }
        let version = probe_version(&bin)?;
        if let Some(required) = &cfg.pxl_version {
            if &version != required {
                return Err(HarnessError::integrity(format!(
                    "pxl version {version} != configured {required}"
                ))
                .with_hint("--pxl-version-lock is prefix matching; the harness requires exact"));
            }
        }
        Ok(Self {
            bin,
            version: Some(version),
            binary_digest: Some(digest),
            timeout_secs: cfg.pxl_timeout_secs,
            max_output_bytes: cfg.max_tool_output_bytes,
        })
    }

    pub fn version(&self) -> &str {
        self.version.as_deref().unwrap_or("unknown")
    }

    /// A placeholder adapter for stages/tests that never invoke pxl. Any
    /// accidental call fails fast with a clear spawn error.
    pub fn unavailable() -> Self {
        Self {
            bin: PathBuf::from("/nonexistent/pxl"),
            version: Some("0.1.0".into()),
            binary_digest: None,
            timeout_secs: 1,
            max_output_bytes: 4096,
        }
    }

    pub fn binary_digest(&self) -> &str {
        self.binary_digest.as_deref().unwrap_or("unknown")
    }

    fn spec(&self, args: &[String], cwd: Option<&Path>) -> ProcSpec {
        ProcSpec {
            program: self.bin.to_string_lossy().into_owned(),
            args: args.to_vec(),
            cwd: cwd.map(|p| p.to_path_buf()),
            timeout_secs: self.timeout_secs,
            max_output_bytes: self.max_output_bytes,
        }
    }

    fn exec(&self, args: &[String], cwd: Option<&Path>) -> Result<PxlResult_> {
        let out = run(&self.spec(args, cwd))?;
        normalize(&out, args)
    }

    /// `pxl --json list <kind>`
    pub fn list(&self, kind: &str) -> Result<PxlResult_> {
        self.exec(&json_args(vec!["list".into(), kind.into()]), None)
    }

    /// `pxl --json schema <kind>` — returns the bare schema document.
    pub fn schema(&self, kind: &str) -> Result<Value> {
        let res = self.exec(&json_args(vec!["schema".into(), kind.into()]), None)?;
        if !res.ok {
            return Err(HarnessError::tool(format!(
                "pxl schema {kind} failed: {} {}",
                res.error_code, res.message
            )));
        }
        serde_json::from_str(&res.stdout)
            .map_err(|e| HarnessError::tool(format!("schema output is not JSON: {e}")))
    }

    /// `pxl --json build <recipe> -o <dir>` with a version lock.
    pub fn build(
        &self,
        recipe: &Path,
        out_dir: &Path,
        seed: Option<u64>,
        cwd: Option<&Path>,
    ) -> Result<PxlBuildOutput> {
        let mut args = json_args(Vec::new());
        args.push("--pxl-version-lock".into());
        args.push(self.version().to_string());
        if let Some(seed) = seed {
            args.push("--seed".into());
            args.push(seed.to_string());
        }
        args.push("build".into());
        args.push(recipe.to_string_lossy().into_owned());
        args.push("-o".into());
        args.push(out_dir.to_string_lossy().into_owned());
        let res = self.exec(&args, cwd)?;
        let mut parsed = PxlBuildOutput {
            result: res,
            outputs: Vec::new(),
            sheet_hash: None,
            checks: Vec::new(),
            file_digests: Vec::new(),
        };
        if let Ok(v) = serde_json::from_str::<Value>(&parsed.result.stdout) {
            parsed.outputs = v
                .get("outputs")
                .and_then(|o| o.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            parsed.sheet_hash = v
                .get("hash")
                .and_then(|h| h.as_str())
                .map(|s| s.to_string());
            parsed.checks = v
                .get("checks")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
        }
        // Independently hash whatever landed on disk.
        for name in &parsed.outputs {
            let p = out_dir.join(name);
            if p.is_file() {
                parsed
                    .file_digests
                    .push((name.clone(), digest_file(&p)?));
            }
        }
        Ok(parsed)
    }

    /// `pxl --json validate <input> --rules <rules>`
    pub fn validate(&self, input: &Path, rules: &str) -> Result<PxlResult_> {
        self.exec(
            &json_args(vec![
                "validate".into(),
                input.to_string_lossy().into_owned(),
                "--rules".into(),
                rules.to_string(),
            ]),
            None,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxlBuildOutput {
    pub result: PxlResult_,
    pub outputs: Vec<String>,
    pub sheet_hash: Option<String>,
    pub checks: Vec<Value>,
    /// Digests of files that actually exist on disk after the build.
    pub file_digests: Vec<(String, String)>,
}

impl PxlBuildOutput {
    pub fn qa_ok(&self) -> bool {
        self.result.ok
    }
    pub fn failed_checks(&self) -> Vec<&Value> {
        self.checks
            .iter()
            .filter(|c| c.get("pass").and_then(|p| p.as_bool()) == Some(false))
            .collect()
    }
}

fn json_args(mut args: Vec<String>) -> Vec<String> {
    let mut out = vec!["--json".to_string()];
    out.append(&mut args);
    out
}

fn which_pxl() -> Result<PathBuf> {
    // Prefer the workspace build next to the harness, then PATH lookup.
    let local = PathBuf::from("target/debug/pxl");
    if local.is_file() {
        return Ok(canonical(&local));
    }
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in path.split(':') {
        if dir.is_empty() {
            continue;
        }
        let cand = PathBuf::from(dir).join("pxl");
        if cand.is_file() {
            return Ok(cand);
        }
    }
    Err(HarnessError::not_found(
        "no `pxl` binary found on PATH or in target/debug",
    )
    .with_hint("run `cargo build -p pxl-cli` or set pxl_bin"))
}

fn canonical(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn probe_version(bin: &Path) -> Result<String> {
    let out = run(&ProcSpec {
        program: bin.to_string_lossy().into_owned(),
        args: vec!["--version".into()],
        cwd: None,
        timeout_secs: 15,
        max_output_bytes: 4096,
    })?;
    let text = out.stdout.trim();
    let version = text
        .strip_prefix("pxl ")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| text.to_string());
    if version.is_empty() {
        return Err(HarnessError::tool(format!(
            "unexpected `pxl --version` output: {:?}",
            out.stdout
        )));
    }
    Ok(version)
}

/// Map a raw process result onto the normalized error taxonomy.
fn normalize(out: &ProcOutput, argv: &[String]) -> Result<PxlResult_> {
    if out.timed_out {
        return Ok(PxlResult_ {
            ok: false,
            exit_code: out.exit_code,
            error_code: "INFRA_TIMEOUT".into(),
            message: format!("pxl timed out after {} ms", out.elapsed_ms),
            stdout: out.stdout.clone(),
            stderr: out.stderr.clone(),
            timed_out: true,
            elapsed_ms: out.elapsed_ms,
        });
    }
    let Some(code) = out.exit_code else {
        return Ok(PxlResult_ {
            ok: false,
            exit_code: None,
            error_code: "INFRA_NO_EXIT".into(),
            message: "pxl process exited without a status code".into(),
            stdout: out.stdout.clone(),
            stderr: out.stderr.clone(),
            timed_out: false,
            elapsed_ms: out.elapsed_ms,
        });
    };
    let parsed: Option<Value> = serde_json::from_str(&out.stdout).ok();
    if code != 0 && parsed.is_none() && !out.stdout.trim().is_empty() {
        return Ok(PxlResult_ {
            ok: false,
            exit_code: Some(code),
            error_code: "INFRA_NON_JSON".into(),
            message: format!("pxl exited {code} with non-JSON stdout"),
            stdout: out.stdout.clone(),
            stderr: out.stderr.clone(),
            timed_out: false,
            elapsed_ms: out.elapsed_ms,
        });
    }
    // Success and JSON errors share the ok/error envelope.
    if let Some(v) = &parsed {
        let ok = v.get("ok").and_then(|o| o.as_bool()).unwrap_or(false);
        let error_code = v
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("E_IO")
            .to_string();
        let message = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        if ok && code == 0 {
            return Ok(PxlResult_ {
                ok: true,
                exit_code: Some(0),
                error_code: String::new(),
                message: String::new(),
                stdout: out.stdout.clone(),
                stderr: out.stderr.clone(),
                timed_out: false,
                elapsed_ms: out.elapsed_ms,
            });
        }
        return Ok(PxlResult_ {
            ok: false,
            exit_code: Some(code),
            error_code,
            message,
            stdout: out.stdout.clone(),
            stderr: out.stderr.clone(),
            timed_out: false,
            elapsed_ms: out.elapsed_ms,
        });
    }
    // No JSON at all (e.g. crashed before printing).
    Ok(PxlResult_ {
        ok: false,
        exit_code: Some(code),
        error_code: if code == 0 {
            "INFRA_NON_JSON".into()
        } else {
            "E_IO".into()
        },
        message: format!("pxl exited {code}; argv: {argv:?}"),
        stdout: out.stdout.clone(),
        stderr: out.stderr.clone(),
        timed_out: false,
        elapsed_ms: out.elapsed_ms,
    })
}

/// Map pxl error codes onto the harness repair taxonomy.
pub fn repair_class(error_code: &str) -> RepairClass {
    match error_code {
        "E_USAGE" | "E_SCHEMA" => RepairClass::CandidateRevision,
        "E_SEMANTIC" => RepairClass::CandidateRevision,
        "E_QA" => RepairClass::AssetFix,
        "E_EXISTS" => RepairClass::IsolationConflict,
        "E_IO" => RepairClass::Environment,
        _ => RepairClass::Infrastructure,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairClass {
    /// Fix the recipe/spec content (usage/schema/semantic).
    CandidateRevision,
    /// Fix pixel content until QA passes.
    AssetFix,
    /// Check directory isolation or resume reconciliation.
    IsolationConflict,
    /// Environment / permissions: report, no silent retry.
    Environment,
    /// Timeout, crash, non-JSON: infrastructure error.
    Infrastructure,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_adapter() -> PxlAdapter {
        let cfg = crate::config::Config::default();
        PxlAdapter::discover(&cfg).unwrap_or_else(|_| PxlAdapter::unavailable())
    }

    #[test]
    fn discover_and_version() {
        let a = test_adapter();
        if a.bin.as_os_str().len() > 0 && a.bin.is_file() {
            assert!(a.version().starts_with("0."));
        }
    }

    #[test]
    fn list_and_schema_shapes() {
        let a = test_adapter();
        if !a.bin.is_file() {
            return; // pxl not available in this environment
        }
        let res = a.list("generators").unwrap();
        assert!(res.ok, "{}", res.message);
        let v: Value = serde_json::from_str(&res.stdout).unwrap();
        let items: Vec<&str> = v["items"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        assert!(items.contains(&"shapes"));
        let schema = a.schema("recipe").unwrap();
        assert!(schema.get("properties").is_some());
    }

    #[test]
    fn repair_classes() {
        assert_eq!(repair_class("E_SCHEMA"), RepairClass::CandidateRevision);
        assert_eq!(repair_class("E_QA"), RepairClass::AssetFix);
        assert_eq!(repair_class("E_EXISTS"), RepairClass::IsolationConflict);
        assert_eq!(repair_class("INFRA_TIMEOUT"), RepairClass::Infrastructure);
    }

    #[test]
    fn build_normalizes_outputs_and_hashes() {
        let a = test_adapter();
        if !a.bin.is_file() {
            return;
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/tank");
        let out = std::env::temp_dir().join(format!("pxl_h_adapter_{}", std::process::id()));
        let _ = fs::remove_dir_all(&out);
        let built = a.build(&root.join("tank.recipe.json"), &out, None, Some(&root)).unwrap();
        assert!(built.qa_ok(), "{}", built.result.message);
        assert!(built.outputs.iter().any(|o| o == "tank.sheet.png"));
        assert!(built
            .file_digests
            .iter()
            .any(|(n, _)| n == "tank.sheet.png"));
        // Deterministic rebuild.
        let out2 = out.with_extension("again");
        let _ = fs::remove_dir_all(&out2);
        let built2 = a.build(&root.join("tank.recipe.json"), &out2, None, Some(&root)).unwrap();
        let d1: Vec<_> = built.file_digests.clone();
        let d2: Vec<_> = built2.file_digests.clone();
        for (n, d) in &d1 {
            let match_d = d2.iter().find(|(n2, _)| n2 == n);
            assert_eq!(match_d.map(|(_, d2)| d2), Some(d), "digest mismatch for {n}");
        }
    }
}
