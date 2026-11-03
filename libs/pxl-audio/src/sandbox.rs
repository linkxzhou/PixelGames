//! Path sandbox for files an audio spec refers to.
//!
//! A spec may only name regular files that stay inside the directory holding
//! the spec. Absolute paths, `..`, URLs and symlinks that resolve outside the
//! root are rejected, and every read is size-capped.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::error::{AudioError, AudioResult};

pub fn resolve(root: &Path, rel: &str) -> AudioResult<PathBuf> {
    if rel.is_empty() {
        return Err(AudioError::semantic("file path is empty"));
    }
    if rel.contains("://") || rel.contains('\0') || rel.contains('\\') {
        return Err(AudioError::semantic(format!(
            "'{rel}' is not a plain relative path (URLs and backslashes are refused)"
        )));
    }
    let path = Path::new(rel);
    if path.is_absolute() {
        return Err(AudioError::semantic(format!(
            "'{rel}' is absolute; spec files must be relative to the spec directory"
        )));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => {
                return Err(AudioError::semantic(format!(
                    "'{rel}' leaves the spec directory"
                )))
            }
        }
    }
    let joined = root.join(path);
    let canonical = fs::canonicalize(&joined).map_err(|e| {
        AudioError::io(format!("cannot resolve {rel}: {e}")).with_hint("check song_file")
    })?;
    let canonical_root = fs::canonicalize(root)
        .map_err(|e| AudioError::io(format!("cannot resolve spec directory: {e}")))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(AudioError::semantic(format!(
            "'{rel}' resolves outside the spec directory"
        )));
    }
    let meta =
        fs::metadata(&canonical).map_err(|e| AudioError::io(format!("cannot stat {rel}: {e}")))?;
    if !meta.is_file() {
        return Err(AudioError::semantic(format!(
            "'{rel}' is not a regular file"
        )));
    }
    Ok(canonical)
}

pub fn read_bounded(path: &Path, limit: usize) -> AudioResult<Vec<u8>> {
    let file = fs::File::open(path)
        .map_err(|e| AudioError::io(format!("cannot open {}: {e}", path.display())))?;
    let mut buf = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|e| AudioError::io(format!("cannot read {}: {e}", path.display())))?;
    if buf.len() > limit {
        return Err(AudioError::limit(format!(
            "{} exceeds the {limit}-byte limit",
            path.display()
        )));
    }
    Ok(buf)
}
