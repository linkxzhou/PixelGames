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

use super::adapter::PxlResult_;
use pxl_store::digest::{digest_file, sha256_hex};


pub(crate) fn json_args(mut args: Vec<String>) -> Vec<String> {
    let mut out = vec!["--json".to_string()];
    out.append(&mut args);
    out
}

/// Binary names to look for, in priority order. `step-300` is the pipeline
/// name (M6 retired the pxl alias); `pxl` stays as a fallback for stale build
/// trees. Windows binaries carry `.exe`.
pub(crate) fn pxl_names() -> Vec<String> {
    let suffix = std::env::consts::EXE_SUFFIX;
    ["step-300", "pxl"]
        .iter()
        .map(|n| format!("{n}{suffix}"))
        .collect()
}

/// Look for a pxl binary next to `sibling_dir` first (a bundled install keeps
/// all five step binaries in one directory), then in every `PATH` entry.
/// `PATH` is split with the platform separator, so this also works on Windows.
pub(crate) fn find_pxl_in(sibling_dir: Option<&Path>, path_var: &std::ffi::OsStr) -> Option<PathBuf> {
    let names = pxl_names();
    let sibling = sibling_dir.into_iter().map(Path::to_path_buf);
    let on_path = std::env::split_paths(path_var).filter(|d| !d.as_os_str().is_empty());
    for dir in sibling.chain(on_path) {
        for name in &names {
            let cand = dir.join(name);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

pub(crate) fn which_pxl() -> Result<PathBuf> {
    // A bundled install (skill `bin/<platform>/`) keeps every step binary in
    // one directory, so the sibling of the running executable wins.
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    if let Some(found) = exe_dir
        .as_deref()
        .and_then(|dir| find_pxl_in(Some(dir), std::ffi::OsStr::new("")))
    {
        return Ok(canonical(&found));
    }
    // Then the workspace build two levels up (this crate lives in libs/),
    // then the cwd-relative target, then PATH lookup. `cargo test` runs with
    // the crate directory as cwd, so a bare relative `target/debug/pxl` is
    // not enough.
    let manifest_target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
    for candidate in [
        manifest_target.join("debug/step-300"),
        manifest_target.join("release/step-300"),
        manifest_target.join("debug/pxl"),
        manifest_target.join("release/pxl"),
        PathBuf::from("target/debug/step-300"),
        PathBuf::from("target/release/step-300"),
        PathBuf::from("target/debug/pxl"),
        PathBuf::from("target/release/pxl"),
    ] {
        if candidate.is_file() {
            return Ok(canonical(&candidate));
        }
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    if let Some(found) = find_pxl_in(None, &path) {
        return Ok(found);
    }
    Err(
        HarnessError::not_found("no `step-300` binary found on PATH or in target/debug")
            .with_hint("run `cargo build -p pxl-cli` or set pxl_bin"),
    )
}

pub(crate) fn canonical(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

pub(crate) fn probe_version(bin: &Path) -> Result<String> {
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
pub(crate) fn normalize(out: &ProcOutput, argv: &[String]) -> Result<PxlResult_> {
    let base = |error_code: &str, message: String| PxlResult_ {
        ok: false,
        exit_code: out.exit_code,
        error_code: error_code.into(),
        message,
        hint: None,
        path: None,
        stdout: out.stdout.clone(),
        stderr: out.stderr.clone(),
        timed_out: out.timed_out,
        elapsed_ms: out.elapsed_ms,
    };
    if out.timed_out {
        return Ok(base(
            "INFRA_TIMEOUT",
            format!("pxl timed out after {} ms", out.elapsed_ms),
        ));
    }
    if out.exit_code.is_none() {
        return Ok(base(
            "INFRA_NO_EXIT",
            "pxl process exited without a status code".into(),
        ));
    }
    let code = out.exit_code.unwrap();
    let parsed: Option<Value> = serde_json::from_str(&out.stdout).ok();
    if code != 0 && parsed.is_none() && !out.stdout.trim().is_empty() {
        return Ok(base(
            "INFRA_NON_JSON",
            format!("pxl exited {code} with non-JSON stdout"),
        ));
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
        let hint = v
            .get("error")
            .and_then(|e| e.get("hint"))
            .and_then(|h| h.as_str())
            .map(str::to_string);
        let path = v
            .get("error")
            .and_then(|e| e.get("path"))
            .and_then(|p| p.as_str())
            .map(str::to_string);
        if ok && code == 0 {
            return Ok(PxlResult_ {
                ok: true,
                exit_code: Some(0),
                error_code: String::new(),
                message: String::new(),
                hint,
                path,
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
            hint,
            path,
            stdout: out.stdout.clone(),
            stderr: out.stderr.clone(),
            timed_out: false,
            elapsed_ms: out.elapsed_ms,
        });
    }
    // No JSON at all (e.g. crashed before printing).
    let error_code = if code == 0 { "INFRA_NON_JSON" } else { "E_IO" };
    Ok(base(
        error_code,
        format!("pxl exited {code}; argv: {argv:?}"),
    ))
}

