use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

pub(crate) fn pxl(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_pxl"))
        .args(args)
        .output()
        .expect("pxl binary");
    let code = out.status.code().unwrap_or(1);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    (code, stdout, stderr)
}

pub(crate) fn json_line(stdout: &str) -> Value {
    let line = stdout.trim();
    assert!(!line.contains('\n'), "stdout must be one JSON line: {line}");
    serde_json::from_str(line).unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {line}"))
}

pub(crate) fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub(crate) fn build_to(recipe: &std::path::Path, tag: &str) -> PathBuf {
    let out = std::env::temp_dir().join(format!("pxl-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{tag}\nstdout={stdout}\nstderr={stderr}");
    out
}

pub(crate) fn validate_sheet(path: &std::path::Path) -> (i32, Value) {
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "validate",
        path.to_str().unwrap(),
        "--rules",
        "default",
    ]);
    let value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}\n{stderr}"));
    (code, value)
}
