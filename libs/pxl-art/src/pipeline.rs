//! The enhancement chain: analyse, strip the outline, upscale, clean, shade and
//! outline, then measure. Each stage is optional and reports what it did, so a
//! caller can tell a sprite that needed no help from one that was rewritten.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult, Rgba};

use crate::analyze::{Material, SpriteGraph};
use crate::cleanup::{self, CleanReport};
use crate::metrics::{self, Metrics};
use crate::outline::{self, OutlineReport};
use crate::palette::{self, RampOptions, RampReport, ShadeMap};
use crate::shade::{self, ShadeReport};

pub use crate::cleanup::CleanOptions;
pub use crate::outline::OutlineMode;
pub use crate::shade::ShadeMode;
pub use crate::upscale::Mode as Upscale;

/// More than this share of the opaque pixels changed by cleanup is reported
/// instead of passing silently: a large edit is probably a wrong input.
const CLEAN_WARN_BPS: u32 = 500;

/// Where the light comes from; selout lifts the outline on the lit side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Light {
    TopLeft,
    TopRight,
    Top,
}

impl Light {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "top_left" | "top-left" => Self::TopLeft,
            "top_right" | "top-right" => Self::TopRight,
            "top" => Self::Top,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::TopLeft => "top_left",
            Self::TopRight => "top_right",
            Self::Top => "top",
        }
    }

    /// Direction from the sprite toward the light, in pixels (y grows down).
    pub fn vector(self) -> (i32, i32) {
        match self {
            Self::TopLeft => (-1, -1),
            Self::TopRight => (1, -1),
            Self::Top => (0, -1),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EnhanceOptions {
    pub scale: u16,
    pub upscale: Upscale,
    /// Keep small and thin features as axis-aligned native blocks.
    pub protect: bool,
    pub cleanup: CleanOptions,
    pub outline: OutlineMode,
    pub light: Light,
    pub shade: ShadeMode,
    pub ramps: RampOptions,
    /// Per-pixel materials for the native frame; the heuristic applies when absent.
    pub materials: Option<Vec<Material>>,
    /// Preview background used by the outline-contrast metric.
    pub background: Rgba,
}

impl Default for EnhanceOptions {
    fn default() -> Self {
        Self {
            scale: 3,
            upscale: Upscale::Hybrid,
            protect: true,
            cleanup: CleanOptions::ALL,
            outline: OutlineMode::Selout,
            light: Light::TopLeft,
            shade: ShadeMode::None,
            ramps: RampOptions::default(),
            materials: None,
            background: Rgba::rgb(28, 26, 24),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProtectReport {
    pub enabled: bool,
    /// Native pixels in the final mask, including the one-pixel safety margin.
    pub pixels: u32,
    pub small: u32,
    pub thin: u32,
}

#[derive(Clone, Debug)]
pub struct EnhanceReport {
    pub scale: u16,
    pub native: (u16, u16),
    pub stages: Vec<&'static str>,
    pub protect: ProtectReport,
    /// Outer outline pixels removed before scaling.
    pub stripped: u32,
    pub ramps: RampReport,
    pub clean: CleanReport,
    pub outline: OutlineReport,
    pub shade: ShadeReport,
    pub metrics: Metrics,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Enhanced {
    pub frame: IndexFrame,
    /// The input palette, extended with generated ramp colours.
    pub palette: Palette,
    pub report: EnhanceReport,
}

pub fn enhance(
    native: &IndexFrame,
    palette: &Palette,
    opts: &EnhanceOptions,
) -> PxlResult<Enhanced> {
    let k = opts.scale;
    if !opts.upscale.supports(k) {
        return Err(PxlError::usage(format!(
            "upscale '{}' does not support scale {k}",
            opts.upscale.name()
        ))
        .with_hint("epx and xbr are 2x, scale3x is 3x, mmpx 2x or 4x; copy, contour and hybrid take 2 to 4"));
    }
    if let Some(list) = &opts.materials {
        if list.len() != native.pixels.len() {
            return Err(PxlError::schema(format!(
                "materials has {} entries for a {}x{} frame",
                list.len(),
                native.w,
                native.h
            )));
        }
    }
    let mut stages: Vec<&'static str> = vec!["analyze"];
    let mut warnings: Vec<String> = Vec::new();

    // The outer outline is drawn again at one pixel after scaling, so it must
    // not be scaled with the fill.
    let outline_index = palette.special.outline;
    let (fill, stripped) = if opts.outline.strips() {
        stages.push("strip_outline");
        outline::strip_outer(native, outline_index)
    } else {
        (native.clone(), 0)
    };
    let mut graph = SpriteGraph::build(&fill, palette, opts.materials.as_deref());
    let protect = ProtectReport {
        enabled: opts.protect,
        pixels: graph.protect.iter().filter(|p| **p).count() as u32,
        small: graph.small.iter().filter(|p| **p).count() as u32,
        thin: graph.thin.iter().filter(|p| **p).count() as u32,
    };
    if !opts.protect {
        graph.protect.fill(false);
    }

    let mut usage = [0u32; 256];
    for &p in &fill.pixels {
        usage[p as usize] += 1;
    }
    let (pal, shades, ramps) = if opts.outline.needs_ramps() || opts.shade != ShadeMode::None {
        stages.push("ramps");
        palette::build_shades(palette, &usage, opts.ramps)
    } else {
        (palette.clone(), ShadeMap::identity(), RampReport::default())
    };
    if !ramps.skipped.is_empty() {
        warnings.push(format!(
            "palette cap {} reached: {} colour(s) have no generated ramp",
            opts.ramps.max_colors,
            ramps.skipped.len()
        ));
    }

    stages.push("upscale");
    let mut frame = crate::upscale::run(&fill, k, opts.upscale, &graph, &pal)?;

    let clean = if opts.cleanup != CleanOptions::NONE {
        stages.push("cleanup");
        let report = cleanup::clean(&mut frame, &fill, k, &graph, opts.cleanup);
        let share = (report.changed() as usize * 10_000 / frame.count_opaque().max(1)) as u32;
        if share > CLEAN_WARN_BPS {
            warnings.push(format!(
                "cleanup changed {} pixels ({}.{:02}% of the sprite)",
                report.changed(),
                share / 100,
                share % 100
            ));
        }
        report
    } else {
        CleanReport::default()
    };

    // Shading runs before the ring is drawn so the outline is never taken for art.
    let shade = if opts.shade == ShadeMode::Volume {
        stages.push("shade");
        let mask: Vec<bool> = frame.pixels.iter().map(|p| *p != 0).collect();
        shade::apply(
            &mut frame,
            &mask,
            k,
            &graph,
            &shades,
            pal.special.outline,
            opts.light,
        )
    } else {
        ShadeReport::default()
    };

    let outline = if opts.outline.strips() && opts.outline != OutlineMode::None {
        stages.push("outline");
        outline::apply(&mut frame, &pal, &shades, opts.outline, opts.light)
    } else {
        OutlineReport::default()
    };

    stages.push("measure");
    let metrics = metrics::measure(&frame, native, k, &graph, &pal, opts.background);
    Ok(Enhanced {
        frame,
        palette: pal,
        report: EnhanceReport {
            scale: k,
            native: (native.w, native.h),
            stages,
            protect,
            stripped,
            ramps,
            clean,
            outline,
            shade,
            metrics,
            warnings,
        },
    })
}

/// Enhance to `factor` (2..=8) as a chain of steps the upscaler supports, so
/// 32 -> 256 is 4x then 2x. Every step re-analyses the frame at its own size.
/// Shading runs on the last step only, so ramps are not stacked.
pub fn enhance_to(
    native: &IndexFrame,
    palette: &Palette,
    factor: u16,
    opts: &EnhanceOptions,
) -> PxlResult<Enhanced> {
    let steps: &[u16] = match factor {
        2 => &[2],
        3 => &[3],
        4 => &[4],
        6 => &[3, 2],
        8 => &[4, 2],
        _ => {
            return Err(
                PxlError::usage(format!("enhance factor {factor} is not 2, 3, 4, 6 or 8"))
                    .with_hint("the size tiers are 2x apart: 32->64->128->256"),
            )
        }
    };
    let mut frame = native.clone();
    let mut pal = palette.clone();
    let mut last: Option<Enhanced> = None;
    for (i, &k) in steps.iter().enumerate() {
        let final_step = i + 1 == steps.len();
        let upscale = if opts.upscale.supports(k) {
            opts.upscale
        } else {
            Upscale::Hybrid
        };
        let step = EnhanceOptions {
            scale: k,
            upscale,
            shade: if final_step {
                opts.shade
            } else {
                ShadeMode::None
            },
            materials: if i == 0 { opts.materials.clone() } else { None },
            ..opts.clone()
        };
        let done = enhance(&frame, &pal, &step)?;
        frame = done.frame.clone();
        pal = done.palette.clone();
        last = Some(done);
    }
    let mut done = last.ok_or_else(|| PxlError::usage("enhance_to has no steps"))?;
    done.report.scale = factor;
    done.report.native = (native.w, native.h);
    Ok(done)
}


#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
