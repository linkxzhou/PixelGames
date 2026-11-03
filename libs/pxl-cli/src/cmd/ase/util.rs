//! `step-300 ase` — headless Aseprite-aligned batch subset (v8.0).

use std::path::Path;

use pxl_ase::doc::ColorMode;
use pxl_ase::frames_for_tag;
use pxl_core::PxlError;

use crate::error::CmdResult;
use crate::util::Ctx;


pub(crate) fn parse_color_mode(s: &str) -> CmdResult<ColorMode> {
    match s.to_ascii_lowercase().as_str() {
        "rgba" | "rgb" | "32" => Ok(ColorMode::Rgba),
        "indexed" | "index" | "8" => Ok(ColorMode::Indexed),
        "grayscale" | "gray" | "grey" | "16" => Ok(ColorMode::Grayscale),
        other => Err(PxlError::usage(format!("unknown color-mode '{other}'"))
            .with_hint("rgba, indexed, or grayscale")
            .into()),
    }
}

pub(crate) fn ensure_out(ctx: &Ctx, path: &Path) -> CmdResult<()> {
    if path.exists() && !ctx.overwrite {
        return Err(PxlError::exists(&path.display().to_string()).into());
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| PxlError::io(e.to_string()))?;
        }
    }
    Ok(())
}

pub(crate) fn map_err(e: impl std::fmt::Display) -> PxlError {
    PxlError::io(e.to_string())
}

// silence unused import if layer filter inlined
#[allow(dead_code)]
pub(crate) fn _keep_frames_for_tag() {
    let _ = frames_for_tag;
}

pub(crate) fn parse_crop_rect(s: &str) -> CmdResult<(i32, i32, u16, u16)> {
    let parts: Vec<_> = s.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return Err(PxlError::usage("--crop expects x,y,w,h").into());
    }
    let x: i32 = parts[0]
        .parse()
        .map_err(|_| PxlError::usage("bad --crop x"))?;
    let y: i32 = parts[1]
        .parse()
        .map_err(|_| PxlError::usage("bad --crop y"))?;
    let w: u16 = parts[2]
        .parse()
        .map_err(|_| PxlError::usage("bad --crop w"))?;
    let h: u16 = parts[3]
        .parse()
        .map_err(|_| PxlError::usage("bad --crop h"))?;
    Ok((x, y, w, h))
}

/// Parse `WxH` / `W,H` / `W H` into `(w, h)`.
pub(crate) fn parse_wxh(s: &str) -> CmdResult<(u32, u32)> {
    let t = s.trim().to_ascii_lowercase().replace('×', "x");
    let parts: Vec<&str> = if t.contains('x') {
        t.split('x').collect()
    } else if t.contains(',') {
        t.split(',').collect()
    } else {
        t.split_whitespace().collect()
    };
    if parts.len() != 2 {
        return Err(PxlError::usage(format!(
            "expected WxH for size, got '{s}'"
        ))
        .into());
    }
    let w: u32 = parts[0]
        .trim()
        .parse()
        .map_err(|_| PxlError::usage(format!("bad width in '{s}'")))?;
    let h: u32 = parts[1]
        .trim()
        .parse()
        .map_err(|_| PxlError::usage(format!("bad height in '{s}'")))?;
    if w == 0 || h == 0 {
        return Err(PxlError::usage("size must be non-zero").into());
    }
    Ok((w, h))
}
