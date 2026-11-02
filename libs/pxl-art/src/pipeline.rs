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
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn frame(rows: &[&str]) -> IndexFrame {
        let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
        pxl_core::decode_matrix(&rows).unwrap()
    }

    fn palette() -> Palette {
        let mut p = Palette {
            id: "t".into(),
            colors: alloc::vec![
                Rgba::TRANSPARENT,
                Rgba::rgb(32, 32, 40),
                Rgba::rgb(232, 64, 64),
                Rgba::rgb(250, 200, 80),
            ],
            ramps: Default::default(),
            cycles: Default::default(),
            special: pxl_core::Special::default(),
        };
        p.special.outline = 1;
        p.special.flash = 1;
        p
    }

    fn sprite() -> IndexFrame {
        frame(&[
            "00000000", "00111100", "01222210", "01223210", "01222210", "01222210", "00111100",
            "00000000",
        ])
    }

    #[test]
    fn output_size_is_native_times_scale_and_no_colour_leaves_the_palette() {
        let f = sprite();
        let pal = palette();
        for (mode, k) in [
            (Upscale::Copy, 2),
            (Upscale::Hybrid, 2),
            (Upscale::Hybrid, 3),
            (Upscale::Contour, 4),
            (Upscale::Scale3x, 3),
            (Upscale::Xbr, 2),
            (Upscale::Mmpx, 2),
            (Upscale::Mmpx, 4),
        ] {
            let opts = EnhanceOptions {
                scale: k,
                upscale: mode,
                ..EnhanceOptions::default()
            };
            let out = enhance(&f, &pal, &opts).unwrap();
            assert_eq!((out.frame.w, out.frame.h), (f.w * k, f.h * k), "{mode:?}");
            out.palette.validate().unwrap();
            for &p in &out.frame.pixels {
                assert!((p as usize) < out.palette.colors.len(), "{mode:?}: {p}");
            }
        }
    }

    #[test]
    fn the_same_input_gives_the_same_bytes() {
        let f = sprite();
        let pal = palette();
        let opts = EnhanceOptions::default();
        let a = enhance(&f, &pal, &opts).unwrap();
        let b = enhance(&f, &pal, &opts).unwrap();
        assert_eq!(a.frame, b.frame);
        assert_eq!(a.palette, b.palette);
        assert_eq!(a.report.metrics, b.report.metrics);
    }

    #[test]
    fn enhance_to_chains_to_8x() {
        let f = sprite();
        let out = enhance_to(&f, &palette(), 8, &EnhanceOptions::default()).unwrap();
        assert_eq!((out.frame.w, out.frame.h), (64, 64));
        assert_eq!(out.report.scale, 8);
        out.palette.validate().unwrap();
        assert!(enhance_to(&f, &palette(), 5, &EnhanceOptions::default()).is_err());
    }

    #[test]
    fn keep_with_copy_is_the_plain_nearest_scale() {
        let f = sprite();
        let opts = EnhanceOptions {
            scale: 2,
            upscale: Upscale::Copy,
            cleanup: CleanOptions::NONE,
            outline: OutlineMode::Keep,
            ..EnhanceOptions::default()
        };
        let out = enhance(&f, &palette(), &opts).unwrap();
        assert_eq!(out.frame, pxl_core::scale_copy(&f, 2).unwrap());
        assert_eq!(out.palette, palette(), "no ramps are generated");
        assert_eq!(out.report.metrics.new_info_bps, 0);
        assert_eq!(out.report.metrics.copy2x_bps, Some(10_000));
    }

    #[test]
    fn a_redrawn_outline_is_one_pixel_thick() {
        let f = sprite();
        let opts = EnhanceOptions {
            scale: 3,
            upscale: Upscale::Copy,
            cleanup: CleanOptions::NONE,
            outline: OutlineMode::Flat,
            ..EnhanceOptions::default()
        };
        let out = enhance(&f, &palette(), &opts).unwrap();
        assert!(out.report.stripped > 0);
        // Native row 1 holds outline at x 2..=5; scaled to 3x the fill starts
        // at row 6, and the ring is the single row above it.
        assert_eq!(out.frame.get(8, 5), 1);
        assert_eq!(out.frame.get(8, 4), 0, "no second outline row");
    }

    #[test]
    fn selout_adds_ramp_colours_without_breaking_the_palette() {
        let f = sprite();
        let out = enhance(&f, &palette(), &EnhanceOptions::default()).unwrap();
        assert!(out.palette.colors.len() > palette().colors.len());
        assert!(!out.report.ramps.generated.is_empty());
        assert!(out.report.stages.contains(&"outline"));
        out.palette.validate().unwrap();
    }

    #[test]
    fn an_unsupported_scale_names_the_fix() {
        let opts = EnhanceOptions {
            scale: 3,
            upscale: Upscale::Epx,
            ..EnhanceOptions::default()
        };
        let err = enhance(&sprite(), &palette(), &opts).unwrap_err();
        assert!(err.message.contains("epx"), "{}", err.message);
        assert!(err.hint.is_some());
    }

    #[test]
    fn a_wrong_sized_material_list_is_rejected() {
        let opts = EnhanceOptions {
            materials: Some(alloc::vec![Material::Metal; 3]),
            ..EnhanceOptions::default()
        };
        assert!(enhance(&sprite(), &palette(), &opts).is_err());
    }

    #[test]
    fn a_small_feature_survives_scaling() {
        let f = sprite();
        let out = enhance(&f, &palette(), &EnhanceOptions::default()).unwrap();
        assert_eq!(out.report.metrics.small_feature_kept_bps, 10_000);
    }

    fn big_sprite() -> IndexFrame {
        let mut rows: Vec<String> = Vec::new();
        for y in 0..14 {
            let row: String = (0..14)
                .map(|x| {
                    let edge = x == 0 || y == 0 || x == 13 || y == 13;
                    if edge {
                        '1'
                    } else {
                        '2'
                    }
                })
                .collect();
            rows.push(row);
        }
        pxl_core::decode_matrix(&rows).unwrap()
    }

    #[test]
    fn shading_is_off_by_default_and_changes_nothing() {
        let f = big_sprite();
        let off = enhance(&f, &palette(), &EnhanceOptions::default()).unwrap();
        assert_eq!(off.report.shade, ShadeReport::default());
        assert!(!off.report.stages.contains(&"shade"));
    }

    #[test]
    fn volume_shading_adds_a_lit_and_a_shaded_side_and_raises_the_value_range() {
        let f = big_sprite();
        let flat = enhance(&f, &palette(), &EnhanceOptions::default()).unwrap();
        let opts = EnhanceOptions {
            shade: ShadeMode::Volume,
            ..EnhanceOptions::default()
        };
        let lit = enhance(&f, &palette(), &opts).unwrap();
        assert!(lit.report.stages.contains(&"shade"));
        assert!(
            lit.report.shade.lit > 0 && lit.report.shade.shaded > 0,
            "{:?}",
            lit.report.shade
        );
        assert!(lit.report.metrics.value_range >= flat.report.metrics.value_range);
        assert!(lit.report.metrics.ramp_coverage_bps >= flat.report.metrics.ramp_coverage_bps);
        lit.palette.validate().unwrap();
        for &p in &lit.frame.pixels {
            assert!((p as usize) < lit.palette.colors.len());
        }
        let again = enhance(&f, &palette(), &opts).unwrap();
        assert_eq!(again.frame, lit.frame);
    }

    #[test]
    fn shading_never_changes_the_silhouette() {
        let f = big_sprite();
        let base = enhance(&f, &palette(), &EnhanceOptions::default()).unwrap();
        let shaded = enhance(
            &f,
            &palette(),
            &EnhanceOptions {
                shade: ShadeMode::Volume,
                ..EnhanceOptions::default()
            },
        )
        .unwrap();
        for (a, b) in base.frame.pixels.iter().zip(&shaded.frame.pixels) {
            assert_eq!(*a == 0, *b == 0, "opacity must match");
        }
    }
}
