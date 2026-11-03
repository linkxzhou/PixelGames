//! ASCII spec compilation shared by `gen ascii`, `ascii render`, recipes, and
//! the web page: palette resolution, delta/mirror/post ops, and animation
//! frames. Files the spec refers to come through a [`Resolver`].

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

use pxl_core::{IndexFrame, Mirror, Palette, PxlError, PxlResult, GEN_VERSION, PXL_VERSION};
use pxl_format::{Anim, AnimFrames, AsciiSpec, LegendFile, Meta, PaletteFile, PaletteMode, PxlDoc};
use pxl_gen::compile_ascii;
use pxl_ops::{apply_ops, expand_macro, ApplyCx};

use crate::resolver::{load_legend_file, load_palette, Resolver};
use crate::timing::check_anim_timing;

/// Final path component, or the whole text when it has none.
pub fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

/// Frames an `ascii` base contributes to a recipe.
pub struct RecipeAscii {
    pub file: String,
    pub kind: Option<String>,
    pub frames: BTreeMap<String, IndexFrame>,
    /// Animations the spec declares when the recipe declares none.
    pub anims: BTreeMap<String, Anim>,
}

/// `text` is the `*.ascii.json` content and `display` its path, used for the
/// `meta.ascii` file name.
pub fn frames_for_recipe(
    text: &str,
    display: &str,
    palette: &Palette,
    seed: u64,
    recipe_has_anims: bool,
    res: &dyn Resolver,
) -> PxlResult<RecipeAscii> {
    let spec = pxl_format::parse_ascii_spec(text)?;
    if recipe_has_anims && !spec.anims.is_empty() {
        return Err(PxlError::schema(
            "ascii.recipe_anims: both the recipe and the ascii spec declare anims",
        )
        .with_hint("keep the animation in one place: drop spec anims, or drop the recipe anims"));
    }
    if spec.palette_mode == PaletteMode::Extend {
        return Err(PxlError::schema(
            "ascii.recipe_palette_mode: palette_mode extend is not supported inside a recipe",
        )
        .with_hint("list every colour in the recipe palette"));
    }
    let (frames, _palette, palette_file, _warnings) =
        compile_frames(&spec, res, Some(palette), seed)?;
    if palette_file.is_some() {
        return Err(PxlError::semantic(
            "ascii.recipe_palette_mode: the layout would append colours",
        )
        .with_hint("list every colour in the recipe palette"));
    }
    let (frames, anims) = if spec.anims.is_empty() {
        (frames, BTreeMap::new())
    } else {
        let mut doc = PxlDoc {
            v: 1,
            palette: String::new(),
            w: spec.size,
            h: spec.size,
            frames: BTreeMap::new(),
            anims: BTreeMap::new(),
            meta: Meta::default(),
        };
        for (name, frame) in &frames {
            doc.insert_frame(name, frame)?;
        }
        attach_anims(&mut doc, &spec, palette, seed)?;
        let anims = doc.anims.clone();
        let mut map = BTreeMap::new();
        for name in doc.frame_names() {
            map.insert(name.clone(), doc.frame(&name)?);
        }
        (map, anims)
    };
    Ok(RecipeAscii {
        file: file_name(display),
        kind: spec.kind.clone(),
        frames,
        anims,
    })
}

/// Compile the spec and run the pixel-level steps the generator crate cannot
/// own: delta ops, mirror, then post.
fn compile_frames(
    spec: &AsciiSpec,
    res: &dyn Resolver,
    palette: Option<&Palette>,
    seed: u64,
) -> PxlResult<(
    BTreeMap<String, IndexFrame>,
    Palette,
    Option<PaletteFile>,
    Vec<String>,
)> {
    let legend_file = load_legend_ref(spec, res)?;
    let shared = legend_file.map(|file| file.legend).unwrap_or_default();
    let compiled = compile_ascii(spec, palette, &shared)?;
    let mirror = mirror_ops(spec)?;
    let mut frames = BTreeMap::new();
    for (name, frame) in &compiled.frames {
        let mut ops = compiled.delta_ops.get(name).cloned().unwrap_or_default();
        ops.extend(mirror.iter().cloned());
        ops.extend(compiled.post.iter().cloned());
        let frame = if ops.is_empty() {
            frame.clone()
        } else {
            apply(frame, &ops, &compiled.palette, seed, name)?
        };
        frames.insert(name.clone(), frame);
    }
    let warnings = compiled
        .warnings
        .iter()
        .map(|w| format!("{}: {}", w.id, w.message))
        .collect();
    Ok((frames, compiled.palette, compiled.palette_file, warnings))
}

pub fn build_parts(
    spec: &AsciiSpec,
    res: &dyn Resolver,
    palette_override: Option<&str>,
    seed: u64,
    with_anims: bool,
) -> PxlResult<(PxlDoc, Palette, Option<PaletteFile>, Vec<String>)> {
    let legend_file = load_legend_ref(spec, res)?;
    let palette_ref = palette_override
        .map(str::to_string)
        .or_else(|| spec.palette.clone())
        .or_else(|| legend_file.as_ref().and_then(|file| file.palette.clone()));
    let palette = match &palette_ref {
        Some(name) => Some(load_palette(name, res)?),
        None => None,
    };
    let (frames, resolved, palette_file, warnings) =
        compile_frames(spec, res, palette.as_ref(), seed)?;
    let mut doc = PxlDoc {
        v: 1,
        palette: palette_ref.unwrap_or_else(|| "ascii.palette.json".into()),
        w: spec.size,
        h: spec.size,
        frames: BTreeMap::new(),
        anims: BTreeMap::new(),
        meta: Meta {
            seed: Some(seed),
            recipe: None,
            pxl_version: Some(PXL_VERSION.into()),
            gen_version: Some(GEN_VERSION),
            asset: None,
            kind: Some(spec.kind.clone().unwrap_or_else(|| "sprite".into())),
            tile_ramp: None,
            parts: None,
            ascii: None,
            durations: Default::default(),
        },
    };
    for (name, frame) in &frames {
        doc.insert_frame(name, frame)?;
    }
    if with_anims {
        attach_anims(&mut doc, spec, &resolved, seed)?;
    }
    Ok((doc, resolved, palette_file, warnings))
}

fn attach_anims(doc: &mut PxlDoc, spec: &AsciiSpec, palette: &Palette, seed: u64) -> PxlResult<()> {
    for (index, anim) in spec.anims.iter().enumerate() {
        let targets: Vec<Option<String>> = if anim.dirs.is_empty() {
            vec![None]
        } else {
            anim.dirs.iter().cloned().map(Some).collect()
        };
        let mut by_dir: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut flat = Vec::new();
        for target in &targets {
            if !anim.frames.is_empty() {
                check_anim_timing(&anim.name, anim.fps, anim.frames.len())?;
                for name in &anim.frames {
                    if !doc.frames.contains_key(name) {
                        return Err(PxlError::schema(format!(
                            "ascii.anim_source: anim '{}' plays frame '{name}', which does not exist",
                            anim.name
                        ))
                        .with_path(format!("/anims/{index}/frames"))
                        .with_hint("list frames declared under /frames"));
                    }
                }
                if let Some(dir) = target {
                    by_dir.insert(dir.clone(), anim.frames.clone());
                } else {
                    flat = anim.frames.clone();
                }
                continue;
            }
            let from = anim.from.clone().unwrap_or_else(|| "base".into());
            let source_name = match target {
                Some(dir) => format!("{from}_{dir}"),
                None => from.clone(),
            };
            let source = doc.frame(&source_name).map_err(|_| {
                PxlError::semantic(format!(
                    "ascii.anim_source: anim '{}' needs frame '{source_name}'",
                    anim.name
                ))
                .with_path(format!("/anims/{index}/from"))
                .with_hint(
                    "name an existing frame in `from`, or give the direction frame a matching name",
                )
            })?;
            let lists = if let Some(mac) = &anim.macro_call {
                expand_macro(mac, target.as_deref(), &source, palette, None)?
            } else {
                anim.ops.clone()
            };
            check_anim_timing(&anim.name, anim.fps, lists.len())?;
            let mut names = Vec::new();
            for (n, ops) in lists.iter().enumerate() {
                let name = match target {
                    Some(dir) => format!("{}_{dir}_{n}", anim.name),
                    None => format!("{}_{n}", anim.name),
                };
                let frame = apply(&source, ops, palette, seed, &name)?;
                doc.insert_frame(&name, &frame)?;
                names.push(name);
            }
            if let Some(dir) = target {
                by_dir.insert(dir.clone(), names);
            } else {
                flat = names;
            }
        }
        doc.anims.insert(
            anim.name.clone(),
            Anim {
                fps: anim.fps,
                looped: anim.looped,
                dirs: anim.dirs.clone(),
                frames: if by_dir.is_empty() {
                    AnimFrames::Seq(flat)
                } else {
                    AnimFrames::ByDir(by_dir)
                },
                mode: anim.mode,
            },
        );
    }
    Ok(())
}

fn apply(
    frame: &IndexFrame,
    ops: &[pxl_format::Op],
    palette: &Palette,
    seed: u64,
    name: &str,
) -> PxlResult<IndexFrame> {
    let cx = ApplyCx {
        palette,
        seed,
        frame_name: name,
        load_template: None,
        parts: None,
        pose: RefCell::new(BTreeMap::new()),
    };
    apply_ops(frame, ops, &cx)
}

fn mirror_ops(spec: &AsciiSpec) -> PxlResult<Vec<pxl_format::Op>> {
    let Some(text) = &spec.mirror else {
        return Ok(Vec::new());
    };
    let flip = |axis: &str| pxl_format::Op::Flip { axis: axis.into() };
    Ok(match Mirror::parse(text)? {
        Mirror::None => Vec::new(),
        Mirror::X => vec![flip("x")],
        Mirror::Y => vec![flip("y")],
        Mirror::Xy => vec![flip("x"), flip("y")],
    })
}

fn load_legend_ref(spec: &AsciiSpec, res: &dyn Resolver) -> PxlResult<Option<LegendFile>> {
    let Some(name) = &spec.legend_ref else {
        return Ok(None);
    };
    let file = load_legend_file(Some(name), res)?;
    if let Some(shared) = file.as_ref().and_then(|f| f.charset) {
        if shared != spec.charset {
            return Err(PxlError::schema(format!(
                "ascii.legend_charset: '{name}' is charset {}, the spec is {}",
                shared.name(),
                spec.charset.name()
            ))
            .with_path("/legend_ref")
            .with_hint("a shared legend and the layouts that use it must agree on charset"));
        }
    }
    Ok(file)
}
