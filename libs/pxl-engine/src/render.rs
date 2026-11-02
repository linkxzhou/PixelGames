//! Encode one frame, a frame strip, or an animation of a [`PxlDoc`] to PNG or
//! GIF bytes. The caller decides where the bytes go.

use pxl_core::{Palette, PxlError, PxlResult};
use pxl_export::{
    encode_gif_timed, gif_delays_cs, pack_sheet, render_asset_png, render_preview_png, SheetCell,
};
use pxl_format::PxlDoc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Png,
    Gif,
}

/// What a PNG is for. A GIF is always a preview-style animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputPurpose {
    /// Backdrop behind transparent pixels and an optional grid, for looking at.
    #[default]
    Preview,
    /// Transparent PNG, no backdrop and no grid, for use in a game.
    Asset,
}

impl OutputPurpose {
    pub fn name(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Asset => "asset",
        }
    }
}

/// Which frames to draw. With `anim` set the animation's frames are used;
/// otherwise a single frame (`frame`, else `base`, else the first one).
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameSelection<'a> {
    pub frame: Option<&'a str>,
    pub anim: Option<&'a str>,
    pub dir: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct Rendered {
    pub bytes: Vec<u8>,
    /// Output size in pixels, i.e. `native * scale` (a strip is wider).
    pub width: u32,
    pub height: u32,
    pub frame_count: usize,
    /// Size of one drawn frame before any scaling: the resolution the art was
    /// made at. Scaling copies pixels; it never adds detail.
    pub native_width: u32,
    pub native_height: u32,
    /// Each native pixel is copied `scale x scale` times (nearest neighbour).
    pub scale: u32,
    pub grid: bool,
    pub purpose: OutputPurpose,
}

pub fn check_scale(scale: u32) -> PxlResult<()> {
    if scale == 0 || scale > 16 {
        return Err(PxlError::usage("scale must be in 1..=16"));
    }
    Ok(())
}

/// Preview render; see [`render_doc_bytes_as`].
pub fn render_doc_bytes(
    doc: &PxlDoc,
    palette: &Palette,
    selection: FrameSelection<'_>,
    scale: u32,
    grid: bool,
    format: OutputFormat,
) -> PxlResult<Rendered> {
    render_doc_bytes_as(
        doc,
        palette,
        selection,
        scale,
        grid,
        format,
        OutputPurpose::Preview,
    )
}

/// `purpose = Asset` keeps transparency and refuses `grid`, which would mark
/// the asset. GIFs ignore `purpose`.
pub fn render_doc_bytes_as(
    doc: &PxlDoc,
    palette: &Palette,
    selection: FrameSelection<'_>,
    scale: u32,
    grid: bool,
    format: OutputFormat,
    purpose: OutputPurpose,
) -> PxlResult<Rendered> {
    check_scale(scale)?;
    if purpose == OutputPurpose::Asset && grid {
        return Err(PxlError::usage("an asset PNG cannot have a grid")
            .with_hint("drop --grid, or render a preview"));
    }
    let (frames, delays, looped) = if let Some(anim_name) = selection.anim {
        let anim = doc.anims.get(anim_name).ok_or_else(|| {
            PxlError::semantic(format!("unknown anim '{anim_name}'"))
                .with_hint("pxl inspect lists animations")
        })?;
        let names = anim.frames.sequence(selection.dir)?;
        let mut frames = Vec::new();
        for name in names {
            frames.push(doc.frame(&name)?);
        }
        let delays = gif_delays_cs(anim.fps, frames.len());
        (frames, delays, anim.looped)
    } else {
        let name = selection.frame.map(str::to_string).unwrap_or_else(|| {
            if doc.frames.contains_key("base") {
                "base".into()
            } else {
                doc.frame_names()
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| "base".into())
            }
        });
        (vec![doc.frame(&name)?], vec![10u16], true)
    };
    let png = |frame: &pxl_core::IndexFrame| match purpose {
        OutputPurpose::Preview => render_preview_png(frame, palette, scale, grid),
        OutputPurpose::Asset => render_asset_png(frame, palette, scale),
    };
    let (bytes, width, height) = if format == OutputFormat::Gif {
        let bytes = encode_gif_timed(&frames, palette, scale, &delays, looped)?;
        (
            bytes,
            frames[0].w as u32 * scale,
            frames[0].h as u32 * scale,
        )
    } else if frames.len() == 1 {
        let bytes = png(&frames[0])?;
        (
            bytes,
            frames[0].w as u32 * scale,
            frames[0].h as u32 * scale,
        )
    } else {
        // A horizontal strip so a multi-frame preview still has a PNG.
        let mut row = Vec::new();
        for (i, frame) in frames.iter().enumerate() {
            row.push(SheetCell {
                name: format!("f{i}"),
                frame: frame.clone(),
                duration_ms: 100,
                tag: None,
            });
        }
        let packed = pack_sheet(&[row], "rows-by-anim", 1)?;
        let bytes = png(&packed.image)?;
        (
            bytes,
            packed.image.w as u32 * scale,
            packed.image.h as u32 * scale,
        )
    };
    Ok(Rendered {
        bytes,
        width,
        height,
        frame_count: frames.len(),
        native_width: frames[0].w as u32,
        native_height: frames[0].h as u32,
        scale,
        grid: grid && format == OutputFormat::Png && purpose == OutputPurpose::Preview,
        purpose: if format == OutputFormat::Gif {
            OutputPurpose::Preview
        } else {
            purpose
        },
    })
}
