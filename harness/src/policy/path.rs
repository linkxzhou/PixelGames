//! Path policy: writes are confined to the current attempt/candidate area.
//! `cwd` is not a sandbox; every write is checked component by component and
//! symlinked parents are resolved away before comparison.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::error::{HarnessError, Result};

/// A write root. All writes must stay inside `root` (after resolving symlinks
/// on the existing part of the path). The root itself is canonicalized at
/// construction so symlinked parents (e.g. `/tmp` → `/private/tmp` on macOS)
/// do not break prefix comparisons.
#[derive(Debug, Clone)]
pub struct WriteRoot {
    root: PathBuf,
}

impl WriteRoot {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let raw: PathBuf = root.into();
        let root = fs::canonicalize(&raw).unwrap_or(raw);
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Validate a relative output path and return the absolute target.
    pub fn resolve_write(&self, rel: &str) -> Result<PathBuf> {
        if rel.is_empty() {
            return Err(HarnessError::policy("empty write path"));
        }
        let rel_path = Path::new(rel);
        if rel_path.is_absolute() {
            return Err(HarnessError::policy(format!(
                "absolute write path '{rel}' is not allowed"
            ))
            .with_hint("write targets are relative to the attempt directory"));
        }
        for comp in rel_path.components() {
            match comp {
                Component::Normal(_) => {}
                Component::CurDir => {}
                other => {
                    return Err(HarnessError::policy(format!(
                        "write path '{rel}' contains '{other:?}'"
                    ))
                    .with_hint("relative paths only; no .. or absolute segments"))
                }
            }
        }
        let target = self.root.join(rel_path);
        let resolved = self.resolve_existing(&target)?;
        if !resolved.starts_with(&self.root) {
            return Err(HarnessError::policy(format!(
                "write path '{rel}' escapes the attempt directory (resolved to {})",
                resolved.display()
            ))
            .with_hint("a symlinked parent points outside the write root"));
        }
        Ok(resolved)
    }

    /// Validate a read path: must be inside one of the allowed roots.
    pub fn resolve_read(&self, rel_or_abs: &str, extra_roots: &[PathBuf]) -> Result<PathBuf> {
        let path = Path::new(rel_or_abs);
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        let resolved = self.resolve_existing(&absolute)?;
        let mut allowed: Vec<&Path> = vec![&self.root];
        allowed.extend(extra_roots.iter().map(|p| p.as_path()));
        if !allowed.iter().any(|root| resolved.starts_with(root)) {
            return Err(HarnessError::policy(format!(
                "read path '{rel_or_abs}' resolves outside the allowed roots"
            ))
            .with_hint("reads stay inside the project and the approved template roots"));
        }
        Ok(resolved)
    }

    fn resolve_existing(&self, path: &Path) -> Result<PathBuf> {
        let mut resolved = PathBuf::new();
        let mut entered_root = false;
        for comp in path.components() {
            resolved.push(comp);
            if let Ok(real) = fs::canonicalize(&resolved) {
                // Keep the not-yet-existing tail unresolved.
                resolved = real;
                if resolved.starts_with(&self.root) {
                    entered_root = true;
                    continue;
                }
                if entered_root {
                    // We were inside the root and a symlink took us out.
                    return Err(HarnessError::policy(format!(
                        "symlink at {} escapes the write root",
                        resolved.display()
                    )));
                }
                // Still walking the root's ancestor prefixes (/, /private, ...).
            }
        }
        Ok(resolved)
    }
}

/// Validate an output path that appears *inside* a recipe file (pxl writes
/// these relative to the recipe dir). Rejects escapes without resolving.
pub fn check_recipe_relative_path(base: &Path, rel: &str) -> Result<()> {
    let path = Path::new(rel);
    if path.is_absolute() {
        return Err(HarnessError::policy(format!(
            "recipe references absolute path '{rel}'"
        ))
        .with_hint("recipe outputs must be relative"));
    }
    let mut depth: i32 = 0;
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return Err(HarnessError::policy(format!(
                        "recipe path '{rel}' escapes its directory"
                    )));
                }
            }
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            other => {
                return Err(HarnessError::policy(format!(
                    "recipe path '{rel}' has component {other:?}"
                )))
            }
        }
    }
    if base.join(path).is_symlink() {
        return Err(HarnessError::policy(format!(
            "recipe path '{rel}' is a symlink"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pxl_h_policy_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_absolute_and_dotdot() {
        let root = WriteRoot::new(tmp());
        assert!(root.resolve_write("/etc/passwd").is_err());
        assert!(root.resolve_write("../x").is_err());
        assert!(root.resolve_write("a/../../x").is_err());
        match root.resolve_write("out/sheet.png") {
            Ok(resolved) => {
                assert!(resolved.starts_with(root.root()));
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn rejects_symlink_escape() {
        let dir = tmp();
        // The escape target must live OUTSIDE the write root.
        let outside = dir.parent().unwrap().join(format!(
            "pxl_h_policy_outside_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, dir.join("escape")).unwrap();
        let root = WriteRoot::new(&dir);
        let r = root.resolve_write("escape/x.txt");
        #[cfg(unix)]
        assert!(r.is_err(), "symlink to {} must be rejected", outside.display());
        #[cfg(not(unix))]
        let _ = r;
        #[cfg(unix)]
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn recipe_paths_stay_relative() {
        let dir = tmp();
        assert!(check_recipe_relative_path(&dir, "out.png").is_ok());
        assert!(check_recipe_relative_path(&dir, "../out.png").is_err());
        assert!(check_recipe_relative_path(&dir, "/tmp/out.png").is_err());
    }
}
