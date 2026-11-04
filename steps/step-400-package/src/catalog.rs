//! Prototype-library provenance for the package.
//!
//! The package copies the frozen `catalog.lock.json` next to its other files
//! and binds it into the revision digest, so a later change to any prototype or
//! surface the build used invalidates the approval. Nothing here reads the
//! library itself: the lock is the closed set of what the build consumed.

use std::fs;
use std::path::Path;

use pxl_contract::error::{HarnessError, Result};
use pxl_contract::prototype::CatalogLock;
use pxl_store::digest::digest_file;
use serde_json::{json, Value};

pub const LOCK_FILE: &str = "catalog.lock.json";

fn read_lock(path: &Path) -> Result<CatalogLock> {
    let text = fs::read_to_string(path)
        .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|e| HarnessError::schema(format!("{}: {e}", path.display())))
}

/// Verify a lock and refuse packaging when it is inconsistent or still holds
/// entries a human has not approved.
pub fn validate(lock: &CatalogLock) -> Result<()> {
    let problems = lock.verify(true);
    if problems.is_empty() {
        return Ok(());
    }
    Err(HarnessError::integrity(format!(
        "catalog lock cannot be packaged: {}",
        problems.join("; ")
    ))
    .with_hint("approve every listed prototype and surface, then re-run `step-300 prototype lock`"))
}

/// Copy the lock into the package and return its digest entry, or `None` when
/// the build used no prototype library.
pub fn package(source: Option<&Path>, out: &Path) -> Result<Option<Value>> {
    let Some(source) = source else {
        return Ok(None);
    };
    let lock = read_lock(source)?;
    validate(&lock)?;
    let target = out.join(LOCK_FILE);
    fs::copy(source, &target)
        .map_err(|e| HarnessError::io(format!("cannot copy {}: {e}", source.display())))?;
    Ok(Some(json!({
        "path": LOCK_FILE,
        "digest": digest_file(&target)?,
        "entries": lock.entries.len(),
        "lock_digest": lock.digest,
    })))
}

/// Files that belong in the revision aggregate.
pub fn revision_files(out: &Path) -> Result<Vec<(String, String)>> {
    let path = out.join(LOCK_FILE);
    if path.is_file() {
        Ok(vec![(LOCK_FILE.to_string(), digest_file(&path)?)])
    } else {
        Ok(Vec::new())
    }
}

/// Re-check a packaged lock for `step-400 check`: the file on disk must still
/// be a consistent, fully approved lock whose own digest matches the entry the
/// package recorded.
pub fn recheck(dir: &Path, recorded: &Value) -> Vec<String> {
    let path = dir.join(LOCK_FILE);
    let lock = match read_lock(&path) {
        Ok(lock) => lock,
        Err(e) => return vec![format!("{LOCK_FILE}: {}", e.message)],
    };
    let mut problems: Vec<String> = lock.verify(true);
    if let Some(expected) = recorded["lock_digest"].as_str() {
        if expected != lock.digest {
            problems.push(format!(
                "{LOCK_FILE}: lock digest {} != recorded {expected}",
                lock.digest
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_contract::prototype::{sha256_digest, EntryKind, LockedEntry, ReviewState};

    fn entry(id: &str, requires: &[&str], review: ReviewState) -> LockedEntry {
        LockedEntry {
            id: id.into(),
            kind: EntryKind::Prototype,
            version: 1,
            digest: sha256_digest(id.as_bytes()),
            requires: requires.iter().map(|s| s.to_string()).collect(),
            review,
        }
    }

    fn temp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pxl-400-catalog-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_lock(path: &Path, lock: &CatalogLock) {
        fs::write(path, serde_json::to_string_pretty(lock).unwrap()).unwrap();
    }

    #[test]
    fn no_lock_means_no_entry_and_no_file() {
        let out = temp("none");
        assert!(package(None, &out).unwrap().is_none());
        assert!(revision_files(&out).unwrap().is_empty());
    }

    #[test]
    fn approved_closed_lock_is_copied_and_bound_into_the_revision() {
        let dir = temp("ok");
        let lock = CatalogLock::seal(vec![
            entry("house", &["roof_tile"], ReviewState::Approved),
            entry("roof_tile", &[], ReviewState::Approved),
        ]);
        let source = dir.join("src.lock.json");
        write_lock(&source, &lock);
        let out = dir.join("pkg");
        fs::create_dir_all(&out).unwrap();

        let recorded = package(Some(&source), &out).unwrap().unwrap();
        assert_eq!(recorded["entries"], 2);
        assert_eq!(recorded["lock_digest"], lock.digest);
        let files = revision_files(&out).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, LOCK_FILE);
        assert!(recheck(&out, &recorded).is_empty());
    }

    #[test]
    fn unapproved_or_open_locks_are_refused() {
        let dir = temp("bad");
        let out = dir.join("pkg");
        fs::create_dir_all(&out).unwrap();

        let pending = CatalogLock::seal(vec![entry("house", &[], ReviewState::Pending)]);
        let source = dir.join("pending.lock.json");
        write_lock(&source, &pending);
        let error = package(Some(&source), &out).unwrap_err();
        assert!(error.message.contains("not approved"), "{}", error.message);

        let open = CatalogLock::seal(vec![entry("house", &["ghost"], ReviewState::Approved)]);
        write_lock(&source, &open);
        let error = package(Some(&source), &out).unwrap_err();
        assert!(error.message.contains("missing entry 'ghost'"), "{}", error.message);
        assert!(!out.join(LOCK_FILE).exists(), "nothing is copied on refusal");
    }

    #[test]
    fn recheck_detects_a_tampered_lock_and_a_missing_file() {
        let dir = temp("tamper");
        let lock = CatalogLock::seal(vec![entry("house", &[], ReviewState::Approved)]);
        let source = dir.join("src.lock.json");
        write_lock(&source, &lock);
        let out = dir.join("pkg");
        fs::create_dir_all(&out).unwrap();
        let recorded = package(Some(&source), &out).unwrap().unwrap();

        let mut edited = lock.clone();
        edited.entries[0].version = 7;
        write_lock(&out.join(LOCK_FILE), &edited);
        // Entries changed but the lock's own digest field did not: the lock no
        // longer matches its entries.
        let problems = recheck(&out, &recorded).join("\n");
        assert!(problems.contains("digest does not match its entries"), "{problems}");

        // A forged lock that recomputes its digest is still caught, because the
        // package recorded the digest of the lock it originally accepted.
        let forged = CatalogLock::seal(edited.entries.clone());
        write_lock(&out.join(LOCK_FILE), &forged);
        let problems = recheck(&out, &recorded).join("\n");
        assert!(problems.contains("!= recorded"), "{problems}");

        fs::remove_file(out.join(LOCK_FILE)).unwrap();
        assert!(!recheck(&out, &recorded).is_empty());
    }
}
