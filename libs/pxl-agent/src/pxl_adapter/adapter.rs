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

use crate::process::{run, ProcOutput, ProcSpec};
use pxl_contract::error::{HarnessError, Result};
use pxl_store::digest::{digest_file, sha256_hex};


use super::helpers::*;
use super::repair::check_failed;

/// Normalized pxl CLI outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxlResult_ {
    pub ok: bool,
    pub exit_code: Option<i32>,
    /// `E_USAGE` etc., or `INFRA_*` for timeout / non-JSON / spawn errors.
    pub error_code: String,
    pub message: String,
    /// `error.hint` from the pxl JSON envelope, when present.
    #[serde(default)]
    pub hint: Option<String>,
    /// `error.path` from the pxl JSON envelope, when present.
    #[serde(default)]
    pub path: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub elapsed_ms: u64,
}

impl PxlResult_ {
    /// Structured checks array parsed from stdout, preserving full
    /// `id/pass/value/limit` objects rather than flattened strings.
    pub fn checks(&self) -> Vec<Value> {
        serde_json::from_str::<Value>(&self.stdout)
            .ok()
            .and_then(|v| v.get("checks").and_then(|c| c.as_array()).cloned())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct PxlAdapter {
    pub(crate) bin: PathBuf,
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
        let bytes =
            fs::read(&bin).map_err(|e| HarnessError::io(format!("{}: {e}", bin.display())))?;
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

    /// The resolved binary path (step-200's lint/capabilities forward it).
    pub fn bin_path(&self) -> &std::path::Path {
        &self.bin
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

    pub fn is_available(&self) -> bool {
        self.bin.is_file()
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
    /// Success is exit 0 plus a JSON document. Unlike other commands, `schema`
    /// does not wrap the document in `{ok: true}`.
    pub fn schema(&self, kind: &str) -> Result<Value> {
        let res = self.exec(&json_args(vec!["schema".into(), kind.into()]), None)?;
        if res.exit_code == Some(0) {
            return serde_json::from_str(&res.stdout)
                .map_err(|e| HarnessError::tool(format!("schema output is not JSON: {e}")));
        }
        Err(HarnessError::tool(format!(
            "pxl schema {kind} failed: {} {}",
            res.error_code, res.message
        )))
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
                parsed.file_digests.push((name.clone(), digest_file(&p)?));
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
        self.checks.iter().filter(|c| check_failed(c)).collect()
    }
}

