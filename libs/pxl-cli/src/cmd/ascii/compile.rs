//! File-facing wrappers around the engine's ASCII compilation: read the spec,
//! hand the engine a directory resolver, and write any palette it generated.

use std::path::Path;

use pxl_core::{IndexFrame, Palette, PxlError};
use pxl_engine::{file_name, frames_for_recipe as engine_frames_for_recipe};
use pxl_format::{AsciiSpec, PaletteFile, PxlDoc};
use serde_json::{json, Value};

use super::super::args::GenArgs;
use super::parent_dir;
use crate::error::CmdResult;
use crate::util::{self, Ctx, DirResolver};

pub(crate) use pxl_engine::RecipeAscii;

/// `pxl gen ascii`: compile the spec, and write a palette file when colours
/// had to be generated or appended.
pub(crate) fn gen_ascii(
    ctx: &Ctx,
    args: &GenArgs,
) -> CmdResult<(PxlDoc, Vec<IndexFrame>, Vec<String>, Vec<(String, Value)>)> {
    let spec_path = args.spec.as_deref().ok_or_else(|| {
        PxlError::usage("ascii requires --spec")
            .with_hint("pass a .ascii.json written by `pxl ascii import`")
    })?;
    let text = util::read_text(spec_path)?;
    let spec = pxl_format::parse_ascii_spec(&text)?;
    let dir = parent_dir(spec_path);
    let seed = ctx.seed.unwrap_or(1);
    let (mut doc, _palette, palette_file, warnings) =
        build_parts(&spec, &dir, args.palette.as_deref(), seed, true)?;
    doc.meta.ascii = Some(file_name(spec_path));
    let mut extras = Vec::new();
    if let Some(file) = palette_file {
        let stem = util::stem_id(Path::new(&args.out));
        let name = format!("{stem}.palette.json");
        doc.palette = name.clone();
        let path = Path::new(&args.out)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&name);
        write_palette(ctx, file, &stem, &path)?;
        extras.push(("palette".into(), json!(name)));
    }
    let frames = doc
        .frame_names()
        .into_iter()
        .map(|name| doc.frame(&name))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((doc, frames, warnings, extras))
}

pub(crate) fn frames_for_recipe(
    path: &Path,
    palette: &Palette,
    seed: u64,
    recipe_has_anims: bool,
) -> CmdResult<RecipeAscii> {
    let display = path.display().to_string();
    let text = std::fs::read_to_string(path)
        .map_err(|e| PxlError::io(format!("cannot read ascii spec {display}: {e}")))?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    Ok(engine_frames_for_recipe(
        &text,
        &display,
        palette,
        seed,
        recipe_has_anims,
        &DirResolver::new(dir),
    )?)
}

pub(crate) fn build_parts(
    spec: &AsciiSpec,
    spec_dir: &Path,
    palette_override: Option<&str>,
    seed: u64,
    with_anims: bool,
) -> CmdResult<(PxlDoc, Palette, Option<PaletteFile>, Vec<String>)> {
    Ok(pxl_engine::build_parts(
        spec,
        &DirResolver::new(spec_dir),
        palette_override,
        seed,
        with_anims,
    )?)
}

pub(crate) fn write_doc_and_palette(
    ctx: &Ctx,
    doc: &mut PxlDoc,
    palette_file: Option<PaletteFile>,
    out: &str,
) -> CmdResult<()> {
    if let Some(file) = palette_file {
        let stem = util::stem_id(Path::new(out));
        let name = format!("{stem}.palette.json");
        doc.palette = name.clone();
        let path = Path::new(out)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&name);
        write_palette(ctx, file, &stem, &path)?;
    }
    Ok(())
}

fn write_palette(ctx: &Ctx, mut file: PaletteFile, id: &str, path: &Path) -> CmdResult<()> {
    file.id = id.to_string();
    let text = pxl_format::to_pretty(&file)?;
    let display = path.display().to_string();
    util::write_out(&display, text.as_bytes(), ctx.overwrite, ctx.json)?;
    Ok(())
}
