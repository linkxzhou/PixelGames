//! One-call rendering for callers that hold a single piece of text: the web
//! page, tests, and anything else without a directory. Detects the input kind,
//! assembles a document, picks PNG or GIF, and encodes.

use pxl_core::{sha256_hex, Palette, PxlError, PxlResult};
use pxl_format::{Anchor, AnimFrames, BlockKind, Charset, Meta, PxlDoc, ShapesFile};
use pxl_gen::generate_shapes;

use crate::ascii_compile::build_parts;
use crate::ascii_import::{build_spec, AsciiImportOptions};
use crate::document::doc_from_frames;
use crate::render::{render_doc_bytes_as, FrameSelection, OutputFormat, OutputPurpose};
use crate::resolver::{load_palette, Resolver};

/// Largest accepted input, in bytes. A 256 canvas of rows plus its document is
/// well under a megabyte; anything larger is a mistake.
pub const MAX_INPUT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// Detect from the text: `<svg` is a document, JSON object keys decide the
    /// rest, anything else is an ASCII grid.
    Auto,
    AsciiText,
    AsciiJson,
    ShapesJson,
    /// An SVG document; needs a palette and a target size.
    Svg,
    /// A `*.sprite.json` source spec (v3).
    Sprite,
    /// A `*.pxl.json` document.
    DocJson,
}

impl InputKind {
    pub fn parse(value: &str) -> PxlResult<Self> {
        match value {
            "auto" => Ok(Self::Auto),
            "ascii" | "ascii-text" => Ok(Self::AsciiText),
            "ascii-json" => Ok(Self::AsciiJson),
            "shapes" | "shapes-json" => Ok(Self::ShapesJson),
            "svg" => Ok(Self::Svg),
            "sprite" | "sprite-json" => Ok(Self::Sprite),
            "doc" | "doc-json" | "pxl" => Ok(Self::DocJson),
            other => Err(PxlError::usage(format!("unknown input kind '{other}'"))
                .with_hint("kind is auto, ascii, ascii-json, shapes, svg, sprite, or doc")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::AsciiText => "ascii",
            Self::AsciiJson => "ascii-json",
            Self::ShapesJson => "shapes",
            Self::Svg => "svg",
            Self::Sprite => "sprite",
            Self::DocJson => "doc",
        }
    }
}

/// What to produce. `Auto` follows the rule in [`render`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatChoice {
    Auto,
    Png,
    Gif,
}

#[derive(Debug, Clone)]
pub struct RenderRequest<'a> {
    pub input: &'a str,
    pub kind: InputKind,
    /// Built-in palette id or a file the resolver knows. Needed by ASCII text
    /// and shapes input that name no palette themselves.
    pub palette: Option<&'a str>,
    /// ASCII text only: `digits` or `printable`; default follows the text.
    pub charset: Option<Charset>,
    /// ASCII text only: canvas 32 or 64.
    pub size: u16,
    /// ASCII text only: `N`, `NxM`, or `fit`.
    pub cell: Option<&'a str>,
    pub anim: Option<&'a str>,
    pub dir: Option<&'a str>,
    pub scale: u32,
    pub seed: u64,
    pub format: FormatChoice,
    pub grid: bool,
    /// SVG only: `strict` (default), `nearest` or `extend`.
    pub color_mode: Option<&'a str>,
    /// SVG only: samples per pixel and axis; unset picks by size.
    pub samples: Option<u8>,
    /// SVG only: run the size tier's finishing pass.
    pub finish: bool,
    /// `Asset` keeps transparency and draws no grid; default is `Preview`.
    pub purpose: OutputPurpose,
}

impl Default for RenderRequest<'_> {
    fn default() -> Self {
        Self {
            input: "",
            kind: InputKind::Auto,
            palette: None,
            charset: None,
            size: 32,
            cell: None,
            anim: None,
            dir: None,
            scale: 8,
            seed: 1,
            format: FormatChoice::Auto,
            grid: false,
            color_mode: None,
            samples: None,
            finish: true,
            purpose: OutputPurpose::Preview,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnimInfo {
    pub name: String,
    pub fps: u32,
    pub frames: usize,
    pub looped: bool,
    pub dirs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DocInfo {
    pub kind: InputKind,
    pub width: u16,
    pub height: u16,
    pub frames: Vec<String>,
    pub anims: Vec<AnimInfo>,
    pub palette: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RenderOutput {
    pub format: OutputFormat,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub frame_count: usize,
    /// Size of one frame before scaling. `width / scale` is not this for a
    /// multi-frame strip, so it is reported separately.
    pub native_width: u32,
    pub native_height: u32,
    pub scale: u32,
    pub grid: bool,
    pub purpose: OutputPurpose,
    /// Animation actually drawn, when the output came from one.
    pub anim: Option<String>,
    pub info: DocInfo,
    /// `sha256:<hex>` of `bytes`, the same form `pxl render --json` reports.
    pub hash: String,
}

/// Decide what the text is. A JSON object is told apart by its keys; anything
/// else is an ASCII grid.
pub fn detect_kind(input: &str) -> InputKind {
    let trimmed = input.trim_start();
    let head = trimmed.strip_prefix('\u{feff}').unwrap_or(trimmed);
    if head.starts_with("<svg") || head.starts_with("<?xml") || head.starts_with("<!DOCTYPE svg") {
        return InputKind::Svg;
    }
    if !trimmed.starts_with('{') {
        return InputKind::AsciiText;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        // Malformed JSON: report it against the most likely kind.
        return InputKind::AsciiJson;
    };
    if value.get("draw").is_some() {
        InputKind::ShapesJson
    } else if value.get("v").and_then(|v| v.as_u64()) == Some(3)
        && value
            .get("source")
            .map_or(false, |s| s.get("kind").is_some())
    {
        InputKind::Sprite
    } else if value.get("frames").map_or(false, |f| {
        // A document stores frames as row arrays; an ASCII spec as objects.
        f.as_object()
            .map_or(false, |m| m.values().next().map_or(false, |v| v.is_array()))
    }) && value.get("w").is_some()
    {
        InputKind::DocJson
    } else {
        InputKind::AsciiJson
    }
}

/// Assemble the document the input describes, without encoding it.
pub fn assemble(
    req: &RenderRequest<'_>,
    res: &dyn Resolver,
) -> PxlResult<(PxlDoc, Palette, InputKind, Vec<String>)> {
    if req.input.len() > MAX_INPUT_BYTES {
        return Err(PxlError::usage(format!(
            "input is {} bytes, over the {MAX_INPUT_BYTES} byte limit",
            req.input.len()
        )));
    }
    if req.input.trim().is_empty() {
        return Err(
            PxlError::usage("input is empty").with_hint("paste an ASCII grid or a JSON spec")
        );
    }
    let kind = if req.kind == InputKind::Auto {
        detect_kind(req.input)
    } else {
        req.kind
    };
    match kind {
        InputKind::AsciiText => {
            let opts = AsciiImportOptions {
                charset: Some(req.charset.unwrap_or_else(|| guess_charset(req.input))),
                size: req.size,
                cell: req.cell.map(str::to_string),
                anchor: Anchor::default(),
                palette: Some(req.palette.unwrap_or(DEFAULT_PALETTE).to_string()),
                legend: None,
                legend_ref: None,
                blocks: BlockKind::Frames,
                pad: true,
                directives: true,
                kind: None,
                size_explicit: true,
            };
            let (spec, mut warnings) = build_spec(&opts, req.input, "input.txt", res)?;
            let (doc, palette, palette_file, more) =
                build_parts(&spec, res, opts.palette.as_deref(), req.seed, true)?;
            warnings.extend(more);
            no_generated_palette(palette_file.is_some())?;
            Ok((doc, palette, kind, warnings))
        }
        InputKind::AsciiJson => {
            let spec = pxl_format::parse_ascii_spec(req.input)?;
            let (doc, palette, palette_file, warnings) =
                build_parts(&spec, res, req.palette, req.seed, true)?;
            no_generated_palette(palette_file.is_some())?;
            Ok((doc, palette, kind, warnings))
        }
        InputKind::ShapesJson => {
            let shapes: ShapesFile = pxl_format::parse_shapes(req.input)?;
            let palette_ref = req
                .palette
                .map(str::to_string)
                .or_else(|| shapes.palette.clone())
                .unwrap_or_else(|| DEFAULT_PALETTE.to_string());
            let palette = load_palette(&palette_ref, res)?;
            let frame = generate_shapes(&shapes, &palette)?;
            let doc = doc_from_frames(&palette_ref, &[("base".into(), frame)], req.seed, "sprite")?;
            Ok((doc, palette, kind, Vec::new()))
        }
        InputKind::Svg => {
            let palette_ref = req.palette.unwrap_or(DEFAULT_PALETTE).to_string();
            let palette = load_palette(&palette_ref, res)?;
            let profile = pxl_core::SizeProfile::for_size(req.size, req.size)?;
            let opts = svg_options(req, &profile);
            let out = pxl_svg::rasterize(req.input, &opts, &palette)?;
            let mut warnings = out.report.warnings();
            let mut doc = PxlDoc {
                v: 1,
                palette: palette_ref,
                w: opts.w,
                h: opts.h,
                frames: Default::default(),
                anims: Default::default(),
                meta: Meta::default(),
            };
            for (name, f) in &out.frames {
                doc.insert_frame(name, &f.frame)?;
            }
            if out.report.colors.snapped > 0 {
                warnings.push(format!(
                    "svg colours snapped to the palette: {}",
                    out.report.colors.snapped
                ));
            }
            Ok((doc, out.palette, kind, warnings))
        }
        InputKind::Sprite => {
            let built = crate::sprite_build::build_sprite(req.input, req.palette, res)?;
            Ok((built.doc, built.palette, kind, built.warnings))
        }
        InputKind::DocJson => {
            let mut doc: PxlDoc = pxl_format::parse_json(req.input, "pxl document")?;
            if let Some(name) = req.palette {
                doc.palette = name.to_string();
            }
            let palette = load_palette(&doc.palette, res)?;
            Ok((doc, palette, kind, Vec::new()))
        }
        InputKind::Auto => unreachable!("auto is resolved above"),
    }
}

/// SVG options for a request, with the tier's finishing defaults.
fn svg_options(req: &RenderRequest<'_>, profile: &pxl_core::SizeProfile) -> pxl_svg::SvgOptions {
    let color_mode = req
        .color_mode
        .and_then(pxl_svg::ColorMode::parse)
        .unwrap_or_default();
    pxl_svg::SvgOptions {
        w: req.size,
        h: req.size,
        samples: req.samples.unwrap_or(0),
        color_mode,
        max_colors: profile.max_colors,
        finish: if req.finish {
            Some(pxl_art::FinishOptions::for_profile(profile))
        } else {
            None
        },
        ..Default::default()
    }
}

/// `digits` when every layout character is 0-9 (directive lines and blank
/// space aside); `printable` as soon as a symbol appears. A page visitor should
/// not have to know the flag to paste a `.#$` picture.
fn guess_charset(text: &str) -> Charset {
    let only_digits = text
        .lines()
        .filter(|line| !line.starts_with("::"))
        .flat_map(str::chars)
        .filter(|c| !c.is_whitespace())
        .all(|c| c.is_ascii_digit());
    if only_digits {
        Charset::Digits
    } else {
        Charset::Printable
    }
}

/// Palette used when the caller names none and the input does not either.
pub const DEFAULT_PALETTE: &str = "p16";

fn no_generated_palette(generated: bool) -> PxlResult<()> {
    if generated {
        return Err(
            PxlError::semantic("the grid uses symbols the palette does not cover")
                .with_hint("pick a palette and use its colours, or draw with the built-in legend"),
        );
    }
    Ok(())
}

/// Summary used by drop-downs: sizes, frames, animations.
pub fn describe(doc: &PxlDoc, kind: InputKind, warnings: Vec<String>) -> DocInfo {
    let anims = doc
        .anims
        .iter()
        .map(|(name, anim)| {
            let frames = match &anim.frames {
                AnimFrames::Seq(v) => v.len(),
                AnimFrames::ByDir(map) => map.values().map(Vec::len).max().unwrap_or(0),
            };
            AnimInfo {
                name: name.clone(),
                fps: anim.fps,
                frames,
                looped: anim.looped,
                dirs: anim.dir_list(),
            }
        })
        .collect();
    DocInfo {
        kind,
        width: doc.w,
        height: doc.h,
        frames: doc.frame_names(),
        anims,
        palette: doc.palette.clone(),
        warnings,
    }
}

/// Parse and assemble only; no encoding.
pub fn inspect(req: &RenderRequest<'_>, res: &dyn Resolver) -> PxlResult<DocInfo> {
    let (doc, _palette, kind, warnings) = assemble(req, res)?;
    Ok(describe(&doc, kind, warnings))
}

/// Pick the animation to draw. Preference: `idle`, else the first by name.
fn default_anim(doc: &PxlDoc) -> Option<String> {
    if doc.anims.contains_key("idle") {
        return Some("idle".into());
    }
    doc.anims.keys().next().cloned()
}

/// Assemble and encode.
///
/// Output rule (`FormatChoice::Auto`): if the document has an animation (the
/// one named by `req.anim`, else `idle`, else the first by name) the result is
/// a GIF of it; otherwise a PNG of the first frame. Asking for a GIF when the
/// document has no animation is an error, not a one-frame GIF.
pub fn render(req: &RenderRequest<'_>, res: &dyn Resolver) -> PxlResult<RenderOutput> {
    let (doc, palette, kind, warnings) = assemble(req, res)?;
    let chosen = match req.anim {
        Some(name) => {
            if !doc.anims.contains_key(name) {
                return Err(PxlError::semantic(format!("unknown anim '{name}'"))
                    .with_hint("the document defines no animation with that name"));
            }
            Some(name.to_string())
        }
        None => default_anim(&doc),
    };
    let (format, anim) = match req.format {
        FormatChoice::Png => (OutputFormat::Png, None),
        FormatChoice::Gif => match chosen {
            Some(name) => (OutputFormat::Gif, Some(name)),
            None => {
                return Err(PxlError::semantic(
                    "this input has no animation, so it cannot be a GIF",
                )
                .with_hint("add `anims` to the spec, or use a macro such as bob(amp=1)"));
            }
        },
        FormatChoice::Auto => match chosen {
            Some(name) => (OutputFormat::Gif, Some(name)),
            None => (OutputFormat::Png, None),
        },
    };
    // A per-direction animation needs a direction; take the caller's, else the
    // first one the animation declares.
    let anim_def = anim.as_deref().and_then(|n| doc.anims.get(n));
    let dir_owned = match anim_def {
        Some(a) if req.dir.is_none() && matches!(a.frames, AnimFrames::ByDir(_)) => {
            a.dir_list().into_iter().next()
        }
        _ => None,
    };
    let dir = req.dir.or(dir_owned.as_deref());
    let rendered = render_doc_bytes_as(
        &doc,
        &palette,
        FrameSelection {
            frame: None,
            anim: anim.as_deref(),
            dir,
        },
        req.scale,
        req.grid,
        format,
        req.purpose,
    )?;
    let hash = format!("sha256:{}", sha256_hex(&rendered.bytes));
    let info = describe(&doc, kind, warnings);
    Ok(RenderOutput {
        format,
        bytes: rendered.bytes,
        width: rendered.width,
        height: rendered.height,
        frame_count: rendered.frame_count,
        native_width: rendered.native_width,
        native_height: rendered.native_height,
        scale: rendered.scale,
        grid: rendered.grid,
        purpose: rendered.purpose,
        anim,
        info,
        hash,
    })
}
