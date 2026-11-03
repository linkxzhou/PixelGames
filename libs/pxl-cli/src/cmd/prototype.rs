//! `step-300 prototype`: check a prototype library, expand one frozen
//! instance, and write the catalog lock.
//!
//! Library layout (all JSON, nothing executable):
//!
//! ```text
//! <library>/prototype/<id>.json     prototype/v1
//! <library>/surface/<id>.json       surface-recipe/v1
//! <library>/base/<id>.shapes.json   artist-authored pixels for a prototype
//! <library>/recolour/<id>.json      surface id -> { "<from>": "<to>" } index map
//! ```
//!
//! Pixels are still produced only here in step 300. Nothing is read from
//! outside the library directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pxl_contract::prototype::{
    sha256_digest, AssetInstance, CatalogLock, EntryKind, LockedEntry, Prototype, ReviewState,
    SceneKit, SurfaceRecipe,
};
use pxl_core::PxlError;
use pxl_format::ShapesFile;
use pxl_gen::expand_instance;
use serde_json::{json, Value};

use super::args::{PrototypeCmd, PrototypeDirArgs, PrototypeExpandArgs, PrototypeLockArgs};
use crate::error::CmdResult;
use crate::util::{self, Ctx};

pub(super) fn prototype_cmd(ctx: &Ctx, action: PrototypeCmd) -> CmdResult<()> {
    match action {
        PrototypeCmd::Check(args) => check_cmd(ctx, args),
        PrototypeCmd::Expand(args) => expand_cmd(ctx, args),
        PrototypeCmd::Lock(args) => lock_cmd(ctx, args),
    }
}

struct Library {
    root: PathBuf,
    prototypes: BTreeMap<String, Prototype>,
    surfaces: BTreeMap<String, SurfaceRecipe>,
    kits: BTreeMap<String, SceneKit>,
    problems: Vec<String>,
}

fn read_json_dir<T: serde::de::DeserializeOwned>(
    dir: &Path,
    what: &str,
    problems: &mut Vec<String>,
) -> BTreeMap<String, (T, Vec<u8>)> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                problems.push(format!("{what} '{stem}': {e}"));
                continue;
            }
        };
        match serde_json::from_slice::<T>(&bytes) {
            Ok(value) => {
                out.insert(stem, (value, bytes));
            }
            Err(e) => problems.push(format!("{what} '{stem}': {e}")),
        }
    }
    out
}

fn load_library(dir: &str) -> CmdResult<Library> {
    let root = PathBuf::from(dir);
    if !root.is_dir() {
        return Err(PxlError::usage(format!("{dir} is not a directory"))
            .with_hint("--library takes the directory that holds prototype/ and surface/")
            .into());
    }
    let mut problems = Vec::new();
    let mut prototypes = BTreeMap::new();
    for (stem, (proto, _)) in read_json_dir::<Prototype>(&root.join("prototype"), "prototype", &mut problems) {
        if stem != proto.id {
            problems.push(format!("prototype file '{stem}.json' declares id '{}'", proto.id));
        }
        problems.extend(proto.validate());
        prototypes.insert(proto.id.clone(), proto);
    }
    let mut surfaces = BTreeMap::new();
    for (stem, (surface, _)) in read_json_dir::<SurfaceRecipe>(&root.join("surface"), "surface", &mut problems) {
        if stem != surface.id {
            problems.push(format!("surface file '{stem}.json' declares id '{}'", surface.id));
        }
        problems.extend(surface.validate());
        surfaces.insert(surface.id.clone(), surface);
    }
    let mut kits = BTreeMap::new();
    for (stem, (kit, _)) in read_json_dir::<SceneKit>(&root.join("kit"), "kit", &mut problems) {
        if stem != kit.id {
            problems.push(format!("kit file '{stem}.json' declares id '{}'", kit.id));
        }
        problems.extend(kit.validate());
        problems.extend(kit.validate_items(&prototypes));
        kits.insert(kit.id.clone(), kit);
    }
    for id in surfaces.keys().chain(kits.keys()) {
        if prototypes.contains_key(id) || (surfaces.contains_key(id) && kits.contains_key(id)) {
            problems.push(format!("id '{id}' is used by more than one library entry kind"));
        }
    }
    // Every dependency named by a prototype must exist in the library.
    for proto in prototypes.values() {
        for dep in &proto.requires {
            if !prototypes.contains_key(dep) && !surfaces.contains_key(dep) {
                problems.push(format!("prototype '{}' requires missing entry '{dep}'", proto.id));
            }
        }
    }
    // The recorded source digest must match the real base file.
    for proto in prototypes.values() {
        let base = root.join("base").join(&proto.source.path);
        match fs::read(&base) {
            Ok(bytes) => {
                if sha256_digest(&bytes) != proto.source.digest {
                    problems.push(format!(
                        "prototype '{}': source digest does not match {}",
                        proto.id,
                        base.display()
                    ));
                }
            }
            Err(e) => problems.push(format!("prototype '{}': cannot read {}: {e}", proto.id, base.display())),
        }
    }
    Ok(Library { root, prototypes, surfaces, kits, problems })
}

fn locked_entries(library: &Library) -> Vec<LockedEntry> {
    let mut entries = Vec::new();
    for proto in library.prototypes.values() {
        entries.push(LockedEntry {
            id: proto.id.clone(),
            kind: EntryKind::Prototype,
            version: proto.version,
            digest: proto.digest(),
            requires: proto.requires.clone(),
            review: proto.review,
        });
    }
    for surface in library.surfaces.values() {
        entries.push(LockedEntry {
            id: surface.id.clone(),
            kind: EntryKind::Surface,
            version: surface.version,
            digest: surface.digest(),
            requires: Vec::new(),
            review: surface.review,
        });
    }
    for kit in library.kits.values() {
        entries.push(LockedEntry {
            id: kit.id.clone(),
            kind: EntryKind::Kit,
            version: kit.version,
            digest: kit.digest(),
            requires: kit.requires(),
            review: kit.review,
        });
    }
    entries
}

fn check_cmd(ctx: &Ctx, args: PrototypeDirArgs) -> CmdResult<()> {
    let library = load_library(&args.library)?;
    let lock = CatalogLock::seal(locked_entries(&library));
    let mut problems = library.problems.clone();
    problems.extend(lock.verify(false));
    let pending = lock.entries.iter().filter(|e| e.review != ReviewState::Approved).count();
    let mut body = util::ok_base("prototype check");
    body.insert("prototypes".into(), json!(library.prototypes.len()));
    body.insert("surfaces".into(), json!(library.surfaces.len()));
    body.insert("kits".into(), json!(library.kits.len()));
    body.insert("not_approved".into(), json!(pending));
    body.insert("problems".into(), json!(problems));
    body.insert("ok".into(), json!(problems.is_empty()));
    util::emit_ok(ctx, &Value::Object(body));
    if problems.is_empty() {
        Ok(())
    } else {
        Err(PxlError::semantic(format!(
            "prototype library has {} problem(s); first: {}",
            problems.len(),
            problems[0]
        ))
        .into())
    }
}

fn load_recolour(root: &Path) -> CmdResult<BTreeMap<String, BTreeMap<char, char>>> {
    let mut out = BTreeMap::new();
    let dir = root.join("recolour");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(out);
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        let raw: BTreeMap<String, String> = serde_json::from_slice(
            &fs::read(&path).map_err(|e| PxlError::io(format!("{}: {e}", path.display())))?,
        )
        .map_err(|e| PxlError::schema(format!("{}: {e}", path.display())))?;
        let mut table = BTreeMap::new();
        for (from, to) in raw {
            let (mut f, mut t) = (from.chars(), to.chars());
            match (f.next(), f.next(), t.next(), t.next()) {
                (Some(a), None, Some(b), None) => {
                    table.insert(a, b);
                }
                _ => {
                    return Err(PxlError::schema(format!(
                        "{}: recolour entries must map one character to one character",
                        path.display()
                    ))
                    .into())
                }
            }
        }
        out.insert(id, table);
    }
    Ok(out)
}

fn expand_cmd(ctx: &Ctx, args: PrototypeExpandArgs) -> CmdResult<()> {
    let library = load_library(&args.library)?;
    if !library.problems.is_empty() {
        return Err(PxlError::semantic(format!(
            "prototype library is not valid: {}",
            library.problems.join("; ")
        ))
        .with_hint("run `step-300 prototype check` first")
        .into());
    }
    let text = util::read_text(&args.instance)?;
    let instance: AssetInstance = pxl_format::parse_json(&text, "asset instance")?;
    let prototype = library.prototypes.get(&instance.prototype.id).ok_or_else(|| {
        PxlError::semantic(format!("prototype '{}' is not in the library", instance.prototype.id))
    })?;

    if !args.allow_pending {
        let mut unapproved = Vec::new();
        if prototype.review != ReviewState::Approved {
            unapproved.push(prototype.id.clone());
        }
        for surface_id in instance.surfaces.values() {
            if let Some(surface) = library.surfaces.get(surface_id) {
                if surface.review != ReviewState::Approved {
                    unapproved.push(surface.id.clone());
                }
            }
        }
        if !unapproved.is_empty() {
            return Err(PxlError::semantic(format!(
                "not approved by a human reviewer: {}",
                unapproved.join(", ")
            ))
            .with_hint("pass --allow-pending for a draft build; approval is never set by this tool")
            .into());
        }
    }

    let base_path = library.root.join("base").join(&prototype.source.path);
    let base: ShapesFile = pxl_format::parse_shapes(&util::read_text(&base_path.to_string_lossy())?)?;
    let recolour = load_recolour(&library.root)?;
    let expanded = expand_instance(prototype, &instance, &base, &library.surfaces, &recolour)?;

    let mut text = serde_json::to_string_pretty(&expanded.shapes)
        .map_err(|e| PxlError::io(format!("shapes encode failed: {e}")))?;
    text.push('\n');
    let written = util::write_out(&args.out, text.as_bytes(), ctx.overwrite, ctx.json)?;
    let mut body = util::ok_base("prototype expand");
    body.insert("out".into(), json!(args.out));
    body.insert("w".into(), json!(expanded.shapes.w));
    body.insert("h".into(), json!(expanded.shapes.h));
    body.insert("applied_variants".into(), json!(expanded.applied_variants));
    body.insert("surfaces".into(), json!(expanded.surfaces));
    body.insert("instance_digest".into(), json!(instance.digest()));
    body.insert("output_digest".into(), json!(util::hash_of(text.as_bytes())));
    body.insert("written".into(), json!(written.is_some()));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn lock_cmd(ctx: &Ctx, args: PrototypeLockArgs) -> CmdResult<()> {
    let library = load_library(&args.library)?;
    if !library.problems.is_empty() {
        return Err(PxlError::semantic(format!(
            "prototype library is not valid: {}",
            library.problems.join("; ")
        ))
        .with_hint("run `step-300 prototype check` first")
        .into());
    }
    let lock = CatalogLock::seal(locked_entries(&library));
    let problems = lock.verify(!args.allow_pending);
    if !problems.is_empty() {
        return Err(PxlError::semantic(format!(
            "catalog lock cannot be written: {}",
            problems.join("; ")
        ))
        .with_hint("approve the listed entries, or pass --allow-pending for a draft lock")
        .into());
    }
    let mut text = serde_json::to_string_pretty(&lock)
        .map_err(|e| PxlError::io(format!("lock encode failed: {e}")))?;
    text.push('\n');
    util::write_out(&args.out, text.as_bytes(), ctx.overwrite, ctx.json)?;
    let mut body = util::ok_base("prototype lock");
    body.insert("out".into(), json!(args.out));
    body.insert("entries".into(), json!(lock.entries.len()));
    body.insert("digest".into(), json!(lock.digest));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}
