use std::collections::BTreeMap;

use pxl_core::{IndexFrame, PxlError, PxlResult, GEN_VERSION, PXL_VERSION};
use pxl_format::{Meta, PxlDoc};

pub fn doc_from_frames(
    palette_ref: &str,
    pairs: &[(String, IndexFrame)],
    seed: u64,
    kind: &str,
) -> PxlResult<PxlDoc> {
    let (w, h) = pairs
        .first()
        .map(|(_, f)| (f.w, f.h))
        .ok_or_else(|| PxlError::semantic("generator produced no frames"))?;
    let mut frames = BTreeMap::new();
    for (name, frame) in pairs {
        if frame.w != w || frame.h != h {
            return Err(PxlError::semantic("generated frames differ in size"));
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
            ascii: None,
            durations: Default::default(),
        },
    })
}
