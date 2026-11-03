//! Finding the pxl `400-package` no matter where the binary was started from.
//!
//! The remakes are usually launched as `target/release/<game>` from a shell in
//! any directory (or by double-clicking the binary), so a cwd-relative default
//! is not enough. Resolution order:
//!
//! 1. an explicit `--package` (a package directory, or a game directory that
//!    contains `work/400-package`),
//! 2. `PXL_PACKAGE`,
//! 3. `default_rel` under the current directory and each of its ancestors,
//! 4. `default_rel` under the executable's directory and each of its ancestors.
//!
//! The executable chain covers `repo/target/release/game` -> `repo`, which is
//! why a release build works from anywhere.

use std::path::{Path, PathBuf};

/// Where to search from. Passed explicitly so tests never mutate the process
/// environment or working directory.
#[derive(Debug, Clone, Default)]
pub struct Candidates {
    pub cwd: Option<PathBuf>,
    pub exe: Option<PathBuf>,
    pub env: Option<PathBuf>,
}

impl Candidates {
    /// Read the real process state.
    pub fn from_env() -> Candidates {
        Candidates {
            cwd: std::env::current_dir().ok(),
            exe: std::env::current_exe().ok(),
            env: std::env::var_os("PXL_PACKAGE").map(PathBuf::from),
        }
    }
}

/// A directory is a package when it carries the packaged manifest.
pub fn is_package(dir: &Path) -> bool {
    dir.join("asset-package.json").is_file()
}

/// Resolve the package directory, or explain every path that was tried.
pub fn resolve_package(
    explicit: Option<&Path>,
    default_rel: &str,
    roots: &Candidates,
) -> Result<PathBuf, String> {
    let relative = Path::new(default_rel);
    let mut tried: Vec<PathBuf> = Vec::new();

    if let Some(requested) = explicit {
        if let Some(found) = package_at(requested, &mut tried) {
            return Ok(found);
        }
        return Err(failure(Requested::Argument, default_rel, &tried));
    }

    if let Some(from_env) = roots.env.as_deref() {
        if let Some(found) = package_at(from_env, &mut tried) {
            return Ok(found);
        }
        return Err(failure(Requested::Env, default_rel, &tried));
    }

    if let Some(cwd) = roots.cwd.as_deref() {
        for root in ancestors(cwd) {
            let candidate = root.join(relative);
            if let Some(found) = accept(&candidate, &mut tried) {
                return Ok(found);
            }
        }
    }
    if let Some(exe) = roots.exe.as_deref() {
        let start = exe.parent().unwrap_or(exe);
        for root in ancestors(start) {
            let candidate = root.join(relative);
            if let Some(found) = accept(&candidate, &mut tried) {
                return Ok(found);
            }
        }
    }

    Err(failure(Requested::Search, default_rel, &tried))
}

enum Requested {
    Argument,
    Env,
    Search,
}

fn package_at(requested: &Path, tried: &mut Vec<PathBuf>) -> Option<PathBuf> {
    if let Some(found) = accept(requested, tried) {
        return Some(found);
    }
    // A game directory (`games/todo-01-pac-man`) is friendlier than spelling
    // out `work/400-package` every time.
    let nested = requested.join("work").join("400-package");
    accept(&nested, tried)
}

fn accept(candidate: &Path, tried: &mut Vec<PathBuf>) -> Option<PathBuf> {
    tried.push(candidate.to_path_buf());
    if !is_package(candidate) {
        return None;
    }
    Some(candidate.canonicalize().unwrap_or_else(|_| candidate.to_path_buf()))
}

/// The path itself plus every parent, deepest first. Bounded so a pathological
/// tree cannot make startup slow.
fn ancestors(start: &Path) -> Vec<PathBuf> {
    let mut chain: Vec<PathBuf> = start
        .ancestors()
        .take(16)
        .map(|path| path.to_path_buf())
        .collect();
    chain.dedup();
    chain
}

fn game_label(default_rel: &str) -> String {
    let parts: Vec<&str> = default_rel
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    match parts.as_slice() {
        ["games", game, ..] => (*game).to_string(),
        [only] => (*only).to_string(),
        _ => default_rel.to_string(),
    }
}

fn failure(requested: Requested, default_rel: &str, tried: &[PathBuf]) -> String {
    let game = game_label(default_rel);
    let mut message = String::new();
    match requested {
        Requested::Argument => message.push_str(&format!(
            "--package does not point at an asset package (no asset-package.json)\n"
        )),
        Requested::Env => message.push_str(&format!(
            "PXL_PACKAGE does not point at an asset package (no asset-package.json)\n"
        )),
        Requested::Search => message.push_str(&format!("cannot find the asset package for {game}\n")),
    }
    if !tried.is_empty() {
        message.push_str("  tried:\n");
        for path in tried.iter().take(12) {
            message.push_str(&format!("    {}\n", path.display()));
        }
        if tried.len() > 12 {
            message.push_str(&format!("    ... {} more\n", tried.len() - 12));
        }
    }
    message.push_str(&format!(
        "hint: build it with `python3 scripts/arcade/arcade.py pipeline games/{game}`\n      or pass `--package <package-dir>` (a game directory also works)"
    ));
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempTree(PathBuf);

    impl TempTree {
        fn new(name: &str) -> TempTree {
            let unique = format!(
                "pxl-path-{}-{}-{}",
                name,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|value| value.subsec_nanos())
                    .unwrap_or(0)
            );
            let root = std::env::temp_dir().join(unique);
            fs::create_dir_all(&root).expect("temp root");
            TempTree(root)
        }

        fn package(&self, game: &str) -> PathBuf {
            let dir = self.0.join("games").join(game).join("work").join("400-package");
            fs::create_dir_all(&dir).expect("package dir");
            fs::write(dir.join("asset-package.json"), "{}").expect("manifest");
            dir
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const REL: &str = "games/demo/work/400-package";

    #[test]
    fn finds_the_package_from_a_deep_working_directory() {
        let tree = TempTree::new("cwd");
        let package = tree.package("demo");
        let deep = tree.0.join("games").join("demo").join("runtime").join("src");
        fs::create_dir_all(&deep).expect("deep dir");
        let roots = Candidates {
            cwd: Some(deep),
            exe: None,
            env: None,
        };
        let found = resolve_package(None, REL, &roots).expect("resolved");
        assert_eq!(found, package.canonicalize().unwrap());
    }

    #[test]
    fn finds_the_package_next_to_the_executable() {
        let tree = TempTree::new("exe");
        let package = tree.package("demo");
        let bin = tree.0.join("target").join("release").join("demo");
        fs::create_dir_all(&bin).expect("bin dir");
        let elsewhere = TempTree::new("elsewhere");
        let roots = Candidates {
            cwd: Some(elsewhere.0.clone()),
            exe: Some(bin.join("demo")),
            env: None,
        };
        let found = resolve_package(None, REL, &roots).expect("resolved");
        assert_eq!(found, package.canonicalize().unwrap());
    }

    #[test]
    fn explicit_game_directory_adds_the_package_suffix() {
        let tree = TempTree::new("explicit");
        let package = tree.package("demo");
        let game_dir = tree.0.join("games").join("demo");
        let roots = Candidates::default();
        let found = resolve_package(Some(&game_dir), REL, &roots).expect("resolved");
        assert_eq!(found, package.canonicalize().unwrap());
    }

    #[test]
    fn explicit_missing_path_explains_the_fix() {
        let tree = TempTree::new("missing");
        let roots = Candidates::default();
        let error = resolve_package(Some(&tree.0.join("nope")), REL, &roots).unwrap_err();
        assert!(error.contains("--package"), "{error}");
        assert!(error.contains("hint:"), "{error}");
        assert!(error.contains("games/demo"), "{error}");
    }

    #[test]
    fn environment_variable_wins_over_search() {
        let tree = TempTree::new("env");
        let package = tree.package("demo");
        let roots = Candidates {
            cwd: Some(tree.0.join("somewhere")),
            exe: None,
            env: Some(package.clone()),
        };
        let found = resolve_package(None, REL, &roots).expect("resolved");
        assert_eq!(found, package.canonicalize().unwrap());
    }

    #[test]
    fn nothing_found_lists_every_attempt() {
        let tree = TempTree::new("none");
        let roots = Candidates {
            cwd: Some(tree.0.clone()),
            exe: Some(tree.0.join("target").join("release").join("demo")),
            env: None,
        };
        let error = resolve_package(None, REL, &roots).unwrap_err();
        assert!(error.contains("cannot find the asset package for demo"), "{error}");
        assert!(error.contains("tried:"), "{error}");
        assert!(error.contains("hint:"), "{error}");
    }
}
