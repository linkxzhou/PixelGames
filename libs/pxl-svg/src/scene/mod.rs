//! Walk the SVG tree and paint it: styles, transforms, `use`, clip paths,
//! gradients, frames, size filtering and feature protection.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use pxl_art::{FinishOptions, Material};
use pxl_core::{IndexFrame, Palette, PxlError, PxlResult, SizeProfile};

use crate::palette_map::{ColorMode, ColorReport, ColorResolver};
use crate::xml::Node;

mod ctx;
mod render;
mod style;

use style::{parse_style, Style};
use ctx::{view, Ctx};
use render::render_frame;

fn auto_samples(side: u16) -> usize {
    match side {
        0..=32 => 8,
        33..=96 => 4,
        _ => 2,
    }
}

#[derive(Clone, Debug)]
pub struct SvgOptions {
    pub w: u16,
    pub h: u16,
    /// Samples per pixel on each axis; 0 picks by size (8 for 32, 4 for 64/128, 2 for 256).
    pub samples: u8,
    pub color_mode: ColorMode,
    /// Steps in a gradient. Every step is a palette entry.
    pub gradient_bands: usize,
    /// Refuse unsupported elements and non-palette colours instead of warning.
    pub strict: bool,
    pub max_colors: usize,
    /// Overrides the document `viewBox`.
    pub view_box: Option<[f64; 4]>,
    /// Snap translations inside `data-frame` groups to whole pixels.
    pub snap: bool,
    /// Run the native-size finishing pass (`pxl-art::finish`) on every frame.
    pub finish: Option<FinishOptions>,
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            w: 64,
            h: 64,
            samples: 0,
            color_mode: ColorMode::Strict,
            gradient_bands: 4,
            strict: false,
            max_colors: 32,
            view_box: None,
            snap: true,
            finish: None,
        }
    }
}

impl SvgOptions {
    pub fn samples_per_pixel(&self) -> usize {
        if self.samples == 0 {
            auto_samples(self.w.max(self.h))
        } else {
            self.samples.clamp(1, 16) as usize
        }
    }
}

#[derive(Clone, Debug)]
pub struct SvgReport {
    pub samples: usize,
    pub shapes: usize,
    pub frames: usize,
    pub unsupported: Vec<String>,
    /// Shapes that produced no pixel at this size.
    pub lost: Vec<String>,
    pub forced: u32,
    /// Shapes skipped because `data-sizes` excludes this tier.
    pub size_filtered: usize,
    pub outline_role: bool,
    pub colors: ColorReport,
    pub finish: Vec<String>,
}

impl SvgReport {
    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        out.extend(self.unsupported.iter().cloned());
        for l in &self.lost {
            out.push(format!("svg.feature_lost: {l}"));
        }
        if self.forced > 0 {
            out.push(format!(
                "svg.feature_forced: {} detail pixel(s) kept by data-min",
                self.forced
            ));
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct FrameOut {
    pub frame: IndexFrame,
    /// Detail pixels the finishing pass must not edit.
    pub protect: Vec<bool>,
    pub materials: Option<Vec<Material>>,
}

#[derive(Clone, Debug)]
pub struct SvgOut {
    pub frames: Vec<(String, FrameOut)>,
    pub palette: Palette,
    pub report: SvgReport,
}


pub fn build(root: &Node, opts: &SvgOptions, palette: &Palette) -> PxlResult<SvgOut> {
    let mut ids: BTreeMap<String, &Node> = BTreeMap::new();
    root.walk(&mut |n| {
        if let Some(id) = n.attr("id") {
            ids.entry(id.to_string()).or_insert(n);
        }
    });
    let mut ctx = Ctx {
        opts,
        colors: ColorResolver::new(palette, opts.color_mode, opts.max_colors),
        warnings: Vec::new(),
        unsupported: Vec::new(),
        ids,
        using: Vec::new(),
        shapes: Vec::new(),
        order: 0,
        logical: 0,
        frame: None,
        outline_role: false,
        size_filtered: 0,
    };

    let (vb, root_m) = view(root, opts)?;
    let style = parse_style(root, &Style::base());
    ctx.walk(root, root_m, &style, vb)?;

    // Frames, in document order; no `data-frame` means one frame called `base`.
    let mut names: Vec<Option<String>> = Vec::new();
    for s in &ctx.shapes {
        if !names.contains(&s.frame) {
            names.push(s.frame.clone());
        }
    }
    if names.is_empty() {
        names.push(None);
    }
    let side = opts.w.max(opts.h);
    let profile = SizeProfile::for_size(opts.w, opts.h)?;
    let mut frames = Vec::new();
    let mut report = SvgReport {
        samples: opts.samples_per_pixel(),
        shapes: ctx.shapes.len(),
        frames: names.len(),
        unsupported: core::mem::take(&mut ctx.unsupported),
        lost: Vec::new(),
        forced: 0,
        size_filtered: ctx.size_filtered,
        outline_role: ctx.outline_role,
        colors: ctx.colors.report.clone(),
        finish: Vec::new(),
    };
    let mut raw_frames = Vec::new();
    for name in names {
        let label = name.clone().unwrap_or_else(|| "base".into());
        let (frame, protect, materials, mut lost, forced) =
            render_frame(&mut ctx, name.as_deref(), &profile, side)?;
        report.lost.append(&mut lost);
        report.forced += forced;
        raw_frames.push((label, frame, protect, materials));
    }
    let anchor = raw_frames.first().map(|(_, frame, _, _)| frame.clone());
    let shared = opts
        .finish
        .as_ref()
        .zip(anchor.as_ref())
        .map(|(fin, frame)| {
            let mut usage = [0u32; 256];
            for pixel in &frame.pixels {
                usage[*pixel as usize] += 1;
            }
            let (palette, shades, _) =
                pxl_art::palette::build_shades(&ctx.colors.palette, &usage, fin.ramps);
            (palette, shades)
        });
    for (label, raw, protect, materials) in raw_frames {
        let mut palette = ctx.colors.palette.clone();
        let mut frame = raw;
        if let Some(fin) = &opts.finish {
            let done = pxl_art::finish(
                &frame,
                &palette,
                &FinishOptions {
                    materials: materials.clone(),
                    protect: Some(protect.clone()),
                    decision_frame: anchor.clone(),
                    shared_shades: shared.clone(),
                    ..fin.clone()
                },
            )?;
            report.finish = done.report.stages.iter().map(|s| s.to_string()).collect();
            frame = done.frame;
            palette = done.palette;
        }
        frames.push((
            label,
            FrameOut {
                frame,
                protect,
                materials,
            },
        ));
        ctx.colors.palette = palette;
    }
    if opts.strict && (!report.unsupported.is_empty() || report.colors.snapped > 0) {
        let mut all = report.unsupported.clone();
        all.extend(report.colors.entries.iter().cloned());
        return Err(PxlError::schema(format!(
            "svg.strict: {} unsupported or approximated item(s)",
            all.len()
        ))
        .with_hint(all.join("; ")));
    }
    Ok(SvgOut {
        frames,
        palette: ctx.colors.palette,
        report,
    })
}

