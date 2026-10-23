use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pxl_core::{IndexFrame, Mirror, Palette, PxlError};
use pxl_format::ShapesFile;
use pxl_gen::{
    generate_bollinger, generate_noise_tile, generate_shapes, variant_seed, RuntimeMask,
};
use pxl_ops::compose_rig;
use serde_json::{json, Value};

use super::args::GenArgs;
use super::document::doc_from_frames;
use crate::error::CmdResult;
use crate::util::{self, Ctx};

pub(super) fn gen(ctx: &Ctx, args: GenArgs) -> CmdResult<()> {
    util::check_lock(ctx, None)?;
    let seed = ctx.seed.unwrap_or(1);
    let mut warnings = Vec::new();
    if ctx.seed.is_none() && args.generator != "shapes" && args.generator != "parts" {
        warnings.push("seed defaulted to 1; pass --seed to make the choice explicit".into());
    }
    let base = PathBuf::from(&args.out);
    let base = base.parent().unwrap_or_else(|| Path::new("."));
    let (palette_ref, palette) = resolve_gen_palette(ctx, &args, base)?;
    let (doc, frames) = match args.generator.as_str() {
        "bollinger" => {
            let template = args.template.as_deref().ok_or_else(|| {
                PxlError::usage("bollinger requires --template")
                    .with_hint("template is a built-in id or a .template.json path")
            })?;
            let mirror = match &args.mirror {
                Some(m) => Some(Mirror::parse(m)?),
                None => None,
            };
            let mask = load_mask_maybe_mirror(template, base, mirror)?;
            let frame = generate_bollinger(&mask, seed, &palette, args.ramp.as_deref())?;
            let doc = doc_from_frames(
                &palette_ref,
                &[("base".into(), frame.clone())],
                seed,
                "sprite",
            )?;
            (doc, vec![frame])
        }
        "shapes" => {
            let spec_path = args.spec.as_deref().ok_or_else(|| {
                PxlError::usage("shapes requires --spec")
                    .with_hint("pass the shapes JSON, for example tank_body.shapes.json")
            })?;
            let text = util::read_text(spec_path)?;
            let spec: ShapesFile = pxl_format::parse_shapes(&text)?;
            let frame = generate_shapes(&spec, &palette)?;
            let doc = doc_from_frames(
                &palette_ref,
                &[("base".into(), frame.clone())],
                seed,
                "sprite",
            )?;
            (doc, vec![frame])
        }
        "noise-tile" => {
            let kind = args.kind.as_deref().unwrap_or("dirt");
            let size = args.size.unwrap_or(16);
            let n = args.variants.max(1);
            if n > 32 {
                return Err(PxlError::usage("variants must be <= 32").into());
            }
            let mut pairs = Vec::new();
            for i in 0..n {
                let frame = generate_noise_tile(
                    kind,
                    size,
                    variant_seed(seed, i),
                    &palette,
                    args.ramp.as_deref(),
                )?;
                let name = if n == 1 {
                    "base".to_string()
                } else {
                    format!("tile_{i}")
                };
                pairs.push((name, frame));
            }
            let frames = pairs.iter().map(|(_, f)| f.clone()).collect();
            // A noise tile is a tile whatever its variant count; the category
            // and the ramp its seams are judged on travel with the document.
            let mut doc = doc_from_frames(&palette_ref, &pairs, seed, "tiles")?;
            if let Some(ramp) = &args.ramp {
                doc.meta.tile_ramp = Some(ramp.clone());
            }
            (doc, frames)
        }
        "parts" => {
            let spec_path = args.spec.as_deref().ok_or_else(|| {
                PxlError::usage("parts requires --spec")
                    .with_hint("pass a .parts.json rig, for example farmer.parts.json")
            })?;
            let text = util::read_text(spec_path)?;
            let rig = pxl_format::parse_parts(&text)?;
            let blank = IndexFrame::new(rig.w, rig.h)?;
            let frame = if let Some(dir) = &args.dir {
                compose_rig(&blank, &rig, dir, &[], &BTreeMap::new())?
            } else {
                blank
            };
            let mut pairs = vec![("base".into(), frame.clone())];
            if let Some(dir) = &args.dir {
                pairs.push((dir.clone(), frame.clone()));
            }
            let mut doc = doc_from_frames(&palette_ref, &pairs, seed, "parts")?;
            doc.meta.parts = Some(spec_file_name(spec_path));
            let frames = pairs.into_iter().map(|(_, f)| f).collect();
            (doc, frames)
        }
        other => {
            return Err(PxlError::usage(format!("unknown generator '{other}'"))
                .with_hint("generators: bollinger, shapes, noise-tile, parts")
                .into());
        }
    };
    let hash = util::save_doc(&doc, &args.out, ctx.overwrite, ctx.json)?;
    util::guard_warnings(ctx, &warnings)?;
    let mut body = util::ok_base("gen");
    body.insert("out".into(), json!(args.out));
    body.insert("w".into(), json!(doc.w));
    body.insert("h".into(), json!(doc.h));
    body.insert("frames".into(), json!(doc.frame_names()));
    body.insert("colors_used".into(), json!(util::colors_used(&frames)));
    body.insert("warnings".into(), json!(warnings));
    body.insert("hash".into(), json!(hash));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

pub(super) fn resolve_gen_palette(
    ctx: &Ctx,
    args: &GenArgs,
    base: &Path,
) -> CmdResult<(String, Palette)> {
    let _ = ctx;
    if let Some(p) = &args.palette {
        let pal = util::load_palette(p, base)?;
        return Ok((p.clone(), pal));
    }
    if let Some(spec) = &args.spec {
        let text = util::read_text(spec)?;
        let palette_ref = if args.generator == "parts" {
            pxl_format::parse_parts(&text)?.palette
        } else {
            let shapes: ShapesFile = pxl_format::parse_shapes(&text)?;
            shapes.palette
        };
        if let Some(p) = palette_ref {
            let spec_base = Path::new(spec).parent().unwrap_or(base);
            let pal = util::load_palette(&p, spec_base)?;
            return Ok((p, pal));
        }
    }
    Err(PxlError::usage("missing --palette")
        .with_hint("pass --palette tank.palette.json or a built-in id")
        .into())
}

pub(super) fn load_mask_maybe_mirror(
    name: &str,
    base: &Path,
    mirror: Option<Mirror>,
) -> CmdResult<RuntimeMask> {
    let mask = util::load_mask(name, base)?;
    if let Some(m) = mirror {
        let mut mask = mask;
        mask.mirror = m;
        return Ok(mask);
    }
    Ok(mask)
}

pub(super) fn spec_file_name(spec: &str) -> String {
    Path::new(spec)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(spec)
        .to_string()
}
