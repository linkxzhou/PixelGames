use std::fs;
use std::path::{Path, PathBuf};

use pxl_core::PxlError;
use serde_json::{json, Value};

use crate::cmd::args::BuildArgs;
use crate::error::CmdResult;
use crate::util::{self, Ctx};

mod into;
mod manifest;

pub(super) use into::build_into;
pub(super) use manifest::manifest_file_name;

pub(super) const STAGING_DIR: &str = ".staging";

pub(super) const REJECTED_DIR: &str = ".rejected";

/// Build into `<out>/.staging`, run QA there, and only then publish. A build
/// that fails at any point leaves the files already in `<out>` untouched; the
/// failed attempt is kept under `<out>/.rejected` for diagnosis. Each published
/// file replaces its predecessor atomically (rename), but publishing several
/// files is not one transaction. `<sheet>.build.json` is the commit record: it
/// is removed first and written last, so whenever it exists every file it
/// lists carries the digest it records.
pub(super) fn build_cmd(ctx: &Ctx, args: BuildArgs) -> CmdResult<()> {
    let final_dir = PathBuf::from(&args.out);
    if final_dir.exists() && !final_dir.is_dir() {
        return Err(
            PxlError::semantic(format!("{} is not a directory", final_dir.display()))
                .with_hint("pxl build -o takes a directory")
                .into(),
        );
    }
    let staging = final_dir.join(STAGING_DIR);
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|e| PxlError::io(format!("cannot clear {}: {e}", staging.display())))?;
    }
    fs::create_dir_all(&staging)
        .map_err(|e| PxlError::io(format!("cannot create output dir: {e}")))?;
    match build_into(ctx, &args, &staging) {
        Ok(body) => {
            // The manifest is the bundle's commit record: drop the old one
            // before replacing files and publish the new one last, so a crash
            // mid-publish leaves no manifest claiming a mixed set of files.
            let manifest = body
                .get("manifest")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if let Some(name) = &manifest {
                let current = final_dir.join(name);
                if current.exists() {
                    fs::remove_file(&current).map_err(|e| {
                        PxlError::io(format!("cannot retire {}: {e}", current.display()))
                    })?;
                }
            }
            publish_tree(&staging, &final_dir, manifest.as_deref())?;
            if let Some(name) = &manifest {
                fs::rename(staging.join(name), final_dir.join(name))
                    .map_err(|e| PxlError::io(format!("cannot publish manifest {name}: {e}")))?;
            }
            let _ = fs::remove_dir_all(&staging);
            let _ = fs::remove_dir_all(final_dir.join(REJECTED_DIR));
            util::emit_ok(ctx, &Value::Object(body));
            Ok(())
        }
        Err(mut err) => {
            let rejected = final_dir.join(REJECTED_DIR);
            let _ = fs::remove_dir_all(&rejected);
            let kept = fs::read_dir(&staging)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
                && fs::rename(&staging, &rejected).is_ok();
            if !kept {
                let _ = fs::remove_dir_all(&staging);
            } else if let Some(Value::Object(extra)) = err.extra.as_mut() {
                extra.insert("rejected".into(), json!(rejected.to_string_lossy()));
            } else {
                err.extra = Some(json!({"rejected": rejected.to_string_lossy()}));
            }
            Err(err)
        }
    }
}

/// Move every file under `src` into `dst`, replacing what is there. A top-level
/// file named `hold` stays in `src` so the caller can publish it last.
pub(super) fn publish_tree(src: &Path, dst: &Path, hold: Option<&str>) -> CmdResult<()> {
    fs::create_dir_all(dst)
        .map_err(|e| PxlError::io(format!("cannot create {}: {e}", dst.display())))?;
    let entries = fs::read_dir(src)
        .map_err(|e| PxlError::io(format!("cannot read {}: {e}", src.display())))?;
    for entry in entries.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            publish_tree(&from, &to, None)?;
        } else if hold.is_some_and(|h| entry.file_name().to_str() == Some(h)) {
            continue;
        } else {
            fs::rename(&from, &to)
                .map_err(|e| PxlError::io(format!("cannot publish {}: {e}", to.display())))?;
        }
    }
    Ok(())
}

