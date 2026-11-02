//! `pxl enhance` and `pxl enhance compare`: rebuild a native-resolution sprite
//! at 2x to 4x, report what each stage did, and lay the choices side by side.

use std::path::Path;

use pxl_art::palette::RampOptions;
use pxl_art::{
    CleanOptions, EnhanceOptions, Light, OutlineMode, ShadeMode,
    Upscale,
};
use pxl_core::{IndexFrame, Palette, PxlError, PxlResult, Rgba};
use pxl_engine::{assemble, InputKind, RenderRequest};
use pxl_export::{
    decode_png_rgba,
    Background,
};

use crate::cmd::args::EnhanceCommon;
use crate::error::CmdResult;
use crate::util::{self, DirResolver};

use super::Source;

pub(crate) fn parse_upscale(text: &str) -> CmdResult<Upscale> {
    Upscale::parse(text).ok_or_else(|| {
        PxlError::usage(format!("unknown upscale '{text}'"))
            .with_hint("upscale is copy, epx, xbr, scale3x, contour, or hybrid")
            .into()
    })
}

pub(super) fn build_options(common: &EnhanceCommon, upscale: Upscale) -> CmdResult<EnhanceOptions> {
    let outline = OutlineMode::parse(&common.outline).ok_or_else(|| {
        PxlError::usage(format!("unknown outline '{}'", common.outline))
            .with_hint("outline is keep, none, flat, selout, or selout_lit")
    })?;
    let light = Light::parse(&common.light).ok_or_else(|| {
        PxlError::usage(format!("unknown light '{}'", common.light))
            .with_hint("light is top_left, top_right, or top")
    })?;
    let shade = ShadeMode::parse(&common.shade).ok_or_else(|| {
        PxlError::usage(format!("unknown shade '{}'", common.shade))
            .with_hint("shade is none or volume")
    })?;
    let protect = match common.protect.as_str() {
        "auto" => true,
        "none" => false,
        _ => {
            return Err(
                PxlError::usage(format!("unknown protect '{}'", common.protect))
                    .with_hint("protect is auto or none")
                    .into(),
            )
        }
    };
    if !(2..=4).contains(&common.scale) {
        return Err(
            PxlError::usage(format!("--scale {} is outside 2..=4", common.scale))
                .with_hint("enhance multiplies the native size by 2, 3 or 4")
                .into(),
        );
    }
    if !(2..=4).contains(&common.levels) {
        return Err(PxlError::usage(format!("--levels {} is outside 2..=4", common.levels)).into());
    }
    if common.hue_shift > 100 {
        return Err(PxlError::usage(format!(
            "--hue-shift {} is outside 0..=100",
            common.hue_shift
        ))
        .into());
    }
    Ok(EnhanceOptions {
        scale: common.scale,
        upscale,
        protect,
        cleanup: parse_cleanup(&common.cleanup)?,
        outline,
        light,
        shade,
        ramps: RampOptions {
            levels: common.levels,
            hue_shift: common.hue_shift,
            max_colors: common.max_colors.clamp(2, 256),
            generate: true,
        },
        materials: None,
        background: Rgba::rgb(28, 26, 24),
    })
}

pub(crate) fn parse_cleanup(text: &str) -> CmdResult<CleanOptions> {
    match text {
        "all" => Ok(CleanOptions::ALL),
        "none" => Ok(CleanOptions::NONE),
        list => {
            let mut out = CleanOptions::NONE;
            for item in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                match item {
                    "orphan" | "orphans" => out.orphans = true,
                    "elbow" | "elbows" | "pixel_perfect" => out.elbows = true,
                    "notch" | "notches" => out.notches = true,
                    "rhythm" | "jaggy" => out.rhythm = true,
                    "symmetry" | "sym" => out.symmetry = true,
                    other => return Err(PxlError::usage(format!(
                        "unknown cleanup rule '{other}'"
                    ))
                    .with_hint(
                        "cleanup is all, none, or a list of orphan, elbow, notch, rhythm, symmetry",
                    )
                    .into()),
                }
            }
            Ok(out)
        }
    }
}

/// `compare` defaults to the checker so a dark outline is visible; a single
/// `enhance` keeps `auto`, which only moves off the classic backdrop when the
/// outline would vanish on it.
pub(super) fn background(common: &EnhanceCommon, palette: &Palette, compare: bool) -> PxlResult<Background> {
    let default = if compare { "checker" } else { "auto" };
    let text = common.background.as_deref().unwrap_or(default);
    let zoom = common.zoom.unwrap_or(if compare { 4 } else { 1 }).max(1);
    // A checker square is `checker_cell` native pixels; the preview is scaled
    // by `scale * zoom` (compare) or `zoom` (enhance, already at full size).
    let per_native = if compare {
        zoom
    } else {
        zoom * common.scale as u32
    };
    Background::resolve(text, palette, common.checker_cell.max(1) * per_native)
}

pub(super) fn load_source(input: &str, common: &EnhanceCommon) -> CmdResult<Source> {
    if input.ends_with(".png") || crate::cmd::render::looks_like_png(input) {
        return load_png(input, common);
    }
    let text = util::read_text(input)?;
    let base = if input == "-" {
        std::env::current_dir().unwrap_or_default()
    } else {
        Path::new(input)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    };
    let res = DirResolver::new(&base);
    // A `.pxl.json` carries `frames` as row arrays, which `assemble` detects.
    let kind = if input.ends_with(".ascii.txt") || input.ends_with(".txt") {
        InputKind::AsciiText
    } else {
        InputKind::Auto
    };
    let req = RenderRequest {
        input: &text,
        kind,
        palette: common.palette.as_deref(),
        seed: 1,
        ..Default::default()
    };
    let (doc, palette, _, warnings) = assemble(&req, &res)?;
    let name = match &common.frame {
        Some(n) => n.clone(),
        None => {
            let names = doc.frame_names();
            if names.iter().any(|n| n == "base") {
                "base".to_string()
            } else {
                names.first().cloned().ok_or_else(|| {
                    PxlError::semantic("the input has no frames")
                        .with_hint("add a frame, or name one with --frame")
                })?
            }
        }
    };
    let frame = doc.frame(&name)?;
    Ok(Source {
        frame,
        palette,
        frame_name: name,
        warnings,
    })
}

/// A png is a preview of a native frame scaled by an integer. Find the largest
/// block size every aligned block of which is one colour, shrink back to the
/// native grid, and index the colours.
pub(super) fn load_png(input: &str, common: &EnhanceCommon) -> CmdResult<Source> {
    let bytes = std::fs::read(input).map_err(|e| PxlError::io(format!("{input}: {e}")))?;
    let (w, h, rgba) = decode_png_rgba(&bytes)?;
    let block = detect_block(w, h, &rgba);
    let (nw, nh) = (w / block, h / block);
    if nw > pxl_core::MAX_DIM as u32 || nh > pxl_core::MAX_DIM as u32 {
        return Err(PxlError::semantic(format!(
            "{input} is {w}x{h} with a block size of {block}, so the native frame would be {nw}x{nh}"
        ))
        .with_hint("enhance takes native frames up to 128x128; pass a smaller png")
        .into());
    }
    let mut colors = vec![Rgba::TRANSPARENT];
    let mut frame = IndexFrame::new(nw as u16, nh as u16)?;
    for y in 0..nh {
        for x in 0..nw {
            let i = (((y * block) * w + x * block) * 4) as usize;
            let px = Rgba {
                r: rgba[i],
                g: rgba[i + 1],
                b: rgba[i + 2],
                a: if rgba[i + 3] == 0 { 0 } else { 255 },
            };
            if px.a == 0 {
                continue;
            }
            let idx = match colors.iter().position(|c| *c == px) {
                Some(p) => p,
                None => {
                    if colors.len() >= 256 {
                        return Err(PxlError::semantic(format!(
                            "{input} uses more than 255 colours"
                        ))
                        .with_hint("enhance works on palette art; reduce the colours first")
                        .into());
                    }
                    colors.push(px);
                    colors.len() - 1
                }
            };
            frame.set(x as i32, y as i32, idx as u8);
        }
    }
    // The darkest colour is the best guess for an outline when the png names none.
    let outline = (1..colors.len())
        .min_by_key(|&i| pxl_core::Oklab::from_rgba(colors[i]).l)
        .unwrap_or(1) as u8;
    let mut palette = Palette {
        id: util::stem_id(Path::new(input)),
        colors,
        ramps: Default::default(),
        cycles: Default::default(),
        special: pxl_core::Special::default(),
    };
    palette.special.outline = outline;
    palette.special.flash = outline;
    let mut warnings = Vec::new();
    if block > 1 {
        warnings.push(format!(
            "png is a {block}x copy of a {nw}x{nh} frame; enhancing the native frame"
        ));
    }
    let _ = common;
    Ok(Source {
        frame,
        palette,
        frame_name: "png".into(),
        warnings,
    })
}

/// Largest `b` for which `w` and `h` divide by `b` and every `b x b` block is
/// one RGBA value (alpha 0 blocks compare equal whatever their RGB).
pub(crate) fn detect_block(w: u32, h: u32, rgba: &[u8]) -> u32 {
    let same = |a: &[u8], b: &[u8]| -> bool {
        if a[3] == 0 && b[3] == 0 {
            true
        } else {
            a == b
        }
    };
    let limit = w.min(h).min(32);
    for b in (2..=limit).rev() {
        if w % b != 0 || h % b != 0 {
            continue;
        }
        let ok = (0..h / b).all(|by| {
            (0..w / b).all(|bx| {
                let at = |x: u32, y: u32| {
                    let i = (((by * b + y) * w + bx * b + x) * 4) as usize;
                    &rgba[i..i + 4]
                };
                let first = at(0, 0);
                (0..b).all(|y| (0..b).all(|x| same(first, at(x, y))))
            })
        });
        if ok {
            return b;
        }
    }
    1
}

