//! Shared helpers for `cli_cases` integration tests.

use std::fs;
use std::path::{Path, PathBuf};

/// Unique temp directory: `pxl-{prefix}-{tag}-{pid}` (removed if it already exists).
pub(crate) fn temp_dir(prefix: &str, tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pxl-{prefix}-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

pub(crate) fn path_str(path: &Path) -> &str {
    path.to_str().unwrap()
}

pub(crate) fn sha256_file(path: &Path) -> String {
    let bytes = fs::read(path).unwrap();
    pxl_core::sha256_hex(&bytes)
}
