use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pxl_core::{IndexFrame, Palette, PxlError, GEN_VERSION, PXL_VERSION};
use pxl_format::{Meta, Op, PxlDoc};
use pxl_ops::{apply_ops, ApplyCx};

use crate::error::CmdResult;
use crate::util::{self};

pub(super) fn doc_from_frames(
    palette_ref: &str,
    pairs: &[(String, IndexFrame)],
    seed: u64,
    kind: &str,
) -> CmdResult<PxlDoc> {
    let (w, h) = pairs
        .first()
        .map(|(_, f)| (f.w, f.h))
        .ok_or_else(|| PxlError::semantic("generator produced no frames"))?;
    let mut frames = BTreeMap::new();
    for (name, frame) in pairs {
        if frame.w != w || frame.h != h {
            return Err(PxlError::semantic("generated frames differ in size").into());
        }
        frames.insert(name.clone(), pxl_core::encode_matrix(frame)?);
    }
    Ok(PxlDoc {
        v: 1,
        palette: palette_ref.to_string(),
        w,
        h,
        frames,
        anims: BTreeMap::new(),
        meta: Meta {
            seed: Some(seed),
            recipe: None,
            pxl_version: Some(PXL_VERSION.into()),
            gen_version: Some(GEN_VERSION),
            asset: None,
            kind: Some(kind.into()),
            tile_ramp: None,
            parts: None,
        },
    })
}

pub(super) fn collect_ops(file: Option<&str>, inline: &[String]) -> CmdResult<Vec<Op>> {
    let mut ops = Vec::new();
    if let Some(path) = file {
        ops.extend(pxl_format::parse_ops_value(&util::read_text(path)?)?);
    }
    for text in inline {
        ops.extend(pxl_format::parse_ops_value(text)?);
    }
    if ops.is_empty() {
        return Err(PxlError::usage("no ops given")
            .with_hint("pass --op '{\"op\":\"flip\",\"axis\":\"x\"}' or --ops ops.json")
            .into());
    }
    Ok(ops)
}

pub(super) fn apply_with(
    frame: &IndexFrame,
    ops: &[Op],
    palette: &Palette,
    seed: u64,
    frame_name: &str,
    base: &Path,
    parts: Option<&pxl_format::PartsFile>,
) -> CmdResult<IndexFrame> {
    let loader = |name: &str| util::load_mask(name, base);
    let cx = ApplyCx {
        palette,
        seed,
        frame_name,
        load_template: Some(&loader),
        parts,
        pose: RefCell::new(BTreeMap::new()),
    };
    Ok(apply_ops(frame, ops, &cx)?)
}

pub(super) fn load_parts(doc: &PxlDoc, base: &Path) -> CmdResult<Option<pxl_format::PartsFile>> {
    let Some(name) = &doc.meta.parts else {
        return Ok(None);
    };
    let path = if Path::new(name).is_absolute() {
        PathBuf::from(name)
    } else {
        base.join(name)
    };
    let text = fs::read_to_string(&path).map_err(|e| {
        PxlError::io(format!("cannot read parts {}: {e}", path.display()))
            .with_hint("meta.parts is the rig file name next to the pxl document")
    })?;
    Ok(Some(pxl_format::parse_parts(&text)?))
}

pub(super) fn frames_vec(doc: &PxlDoc) -> CmdResult<Vec<IndexFrame>> {
    Ok(util::frames_of(doc)?.into_iter().map(|(_, f)| f).collect())
}
