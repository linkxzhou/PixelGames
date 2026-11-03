//! `*.sprite.json` v3: one source (ascii, svg, shapes or a document) plus the
//! size tier's finishing, assembled into a `PxlDoc`.
//!
//! The order is fixed: rasterise or compile the source at native size, upscale
//! when the source declares a smaller `native`, run the finishing pass, then
//! put the frames into a document. Finishing never changes the canvas size, so
//! a variant is only a different starting size for the same source.

use std::collections::BTreeMap;

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult, SizeProfile};
use pxl_format::{FinishSpec, Meta, PxlDoc, SourceKind, SpriteSpec};

use crate::ascii_compile::build_parts;
use crate::resolver::Resolver;
use crate::session::DEFAULT_PALETTE;

pub struct BuiltSprite {
    pub doc: PxlDoc,
    pub palette: Palette,
    pub warnings: Vec<String>,
}

/// Build `text` (a sprite spec) against a resolver.
pub fn build_sprite(
    text: &str,
    palette_override: Option<&str>,
    res: &dyn Resolver,
) -> PxlResult<BuiltSprite> {
    let spec = pxl_format::parse_sprite_spec(text)?;
    build_one(&spec, spec.size, spec.height(), palette_override, res)
}

/// Build one size of a spec. `w`/`h` override `spec.size` for a variant.
pub fn build_one(
    spec: &SpriteSpec,
    w: u16,
    h: u16,
    palette_override: Option<&str>,
    res: &dyn Resolver,
) -> PxlResult<BuiltSprite> {
    let profile = SizeProfile::for_size(w, h)?;
    let profile = apply_overrides(profile, spec);
    let palette_ref = palette_override
        .map(str::to_string)
        .or_else(|| spec.palette.clone())
        .unwrap_or_else(|| DEFAULT_PALETTE.to_string());
    let mut warnings = Vec::new();
    let (frames, palette) = match spec.source.kind {
        SourceKind::Ascii => ascii_frames(spec, w, h, &profile, &palette_ref, res, &mut warnings)?,
        SourceKind::Svg => svg_frames(spec, w, h, &profile, &palette_ref, res, &mut warnings)?,
        SourceKind::Shapes => {
            let (frame, palette) = shapes_frame(spec, &palette_ref, res)?;
            let frame = scale_to(frame, &palette, w, h, &profile, &mut warnings)?;
            (vec![("base".to_string(), frame)], palette)
        }
        SourceKind::Doc => {
            return Err(PxlError::schema(
                "sprite.doc_source: a document source is not compiled, render it directly",
            )
            .with_path("/source/kind")
            .with_hint("use `pxl render` on the *.pxl.json, or switch to an ascii/svg source"));
        }
    };
    if frames.is_empty() {
        return Err(PxlError::semantic(
            "sprite.no_frame: the source produced no frames",
        ));
    }
    let mut doc = PxlDoc {
        v: 1,
        palette: palette_ref,
        w,
        h,
        frames: BTreeMap::new(),
        anims: BTreeMap::new(),
        meta: Meta {
            seed: None,
            recipe: None,
            pxl_version: Some(pxl_core::PXL_VERSION.into()),
            gen_version: Some(pxl_core::GEN_VERSION),
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
    let palette = palette;
    if let Some(mode) = spec.palette_mode.as_deref() {
        if mode != "strict" && mode != "extend" {
            return Err(PxlError::usage(format!("unknown palette_mode '{mode}'"))
                .with_hint("palette_mode is strict or extend"));
        }
    }
    if palette.colors.len() > profile.max_colors {
        warnings.push(format!(
            "sprite.palette_over_tier: {} colours for the {} tier (cap {})",
            palette.colors.len(),
            profile.tier.name(),
            profile.max_colors
        ));
    }
    palette.validate()?;
    Ok(BuiltSprite {
        doc,
        palette,
        warnings,
    })
}

fn apply_overrides(mut profile: SizeProfile, spec: &SpriteSpec) -> SizeProfile {
    if let Some(max) = spec.profile.max_colors {
        profile.max_colors = max.clamp(2, pxl_core::MAX_TEXT_COLORS);
    }
    if let Some(levels) = spec.profile.ramp_levels {
        profile.ramp_levels = levels.clamp(2, 4);
    }
    profile
}

fn ascii_frames(
    spec: &SpriteSpec,
    w: u16,
    h: u16,
    profile: &SizeProfile,
    palette_ref: &str,
    res: &dyn Resolver,
    warnings: &mut Vec<String>,
) -> PxlResult<(Vec<(String, IndexFrame)>, Palette)> {
    let (text, native) = match (&spec.source.file, &spec.source.rows) {
        (Some(file), _) => {
            let text = res
                .read_text(file, crate::resolver::ResourceKind::Palette)?
                .ok_or_else(|| {
                    PxlError::io(format!("cannot read ascii source '{file}'"))
                        .with_hint("the path is relative to the sprite spec")
                })?;
            (text, spec.source.native)
        }
        (None, Some(rows)) => {
            let inline = serde_json::json!({
                "v": 1,
                "size": spec.source.native.unwrap_or(w),
                "palette": palette_ref,
                "frames": { "base": { "rows": rows } },
            });
            (
                serde_json::to_string(&inline)
                    .map_err(|e| PxlError::io(format!("sprite.inline_rows: {e}")))?,
                spec.source.native,
            )
        }
        (None, None) => {
            return Err(
                PxlError::schema("sprite.no_input: ascii source needs file or rows")
                    .with_path("/source"),
            )
        }
    };
    let spec_ascii = pxl_format::parse_ascii_spec(&text)?;
    let (doc, mut palette, _, more) = build_parts(&spec_ascii, res, Some(palette_ref), 1, false)?;
    warnings.extend(more);
    let mut out = Vec::new();
    for name in doc.frame_names() {
        let frame = doc.frame(&name)?;
        let frame = match native {
            Some(n) => upscale_to(frame, &mut palette, n, w, h, profile, warnings)?,
            None => {
                if (frame.w, frame.h) != (w, h) {
                    return Err(PxlError::schema(format!(
                        "sprite.size_mismatch: the layout is {}x{} but the spec asks for {w}x{h}",
                        frame.w, frame.h
                    ))
                    .with_path("/size")
                    .with_hint("set size to the layout's canvas, or source.native to upscale it"));
                }
                frame
            }
        };
        out.push((name, frame));
    }
    Ok((out, palette))
}

fn upscale_to(
    frame: IndexFrame,
    palette: &mut Palette,
    native: u16,
    w: u16,
    _h: u16,
    profile: &SizeProfile,
    warnings: &mut Vec<String>,
) -> PxlResult<IndexFrame> {
    if (frame.w, frame.h) != (native, native) {
        return Err(PxlError::schema(format!(
            "sprite.bad_native: source.native is {native} but the layout is {}x{}",
            frame.w, frame.h
        ))
        .with_path("/source/native"));
    }
    let factor = w / native.max(1);
    if factor <= 1 {
        return Ok(frame);
    }
    let opts = enhance_options(profile);
    let done = pxl_art::enhance_to(&frame, palette, factor, &opts)?;
    *palette = done.palette;
    for w in &done.report.warnings {
        warnings.push(w.clone());
    }
    Ok(done.frame)
}

fn scale_to(
    frame: IndexFrame,
    palette: &Palette,
    w: u16,
    h: u16,
    profile: &SizeProfile,
    warnings: &mut Vec<String>,
) -> PxlResult<IndexFrame> {
    if (frame.w, frame.h) == (w, h) {
        return Ok(frame);
    }
    let fx = w / frame.w.max(1);
    let fy = h / frame.h.max(1);
    if fx != fy || fx <= 1 {
        return Err(PxlError::schema(format!(
            "sprite.size_mismatch: a {:?} shapes spec is {}x{} and must be scaled by one integer to {w}x{h}",
            profile.tier, frame.w, frame.h
        ))
        .with_path("/size")
        .with_hint("write the shapes spec at the target size"));
    }
    let opts = enhance_options(profile);
    let done = pxl_art::enhance_to(&frame, palette, fx, &opts)?;
    warnings.extend(done.report.warnings.clone());
    Ok(done.frame)
}

fn shapes_frame(
    spec: &SpriteSpec,
    palette_ref: &str,
    res: &dyn Resolver,
) -> PxlResult<(IndexFrame, Palette)> {
    let file = spec.source.file.as_deref().ok_or_else(|| {
        PxlError::schema("sprite.no_input: shapes source needs file").with_path("/source")
    })?;
    let text = res
        .read_text(file, crate::resolver::ResourceKind::Palette)?
        .ok_or_else(|| PxlError::io(format!("cannot read shapes source '{file}'")))?;
    let shapes = pxl_format::parse_shapes(&text)?;
    let palette = crate::load_palette(shapes.palette.as_deref().unwrap_or(palette_ref), res)?;
    let frame = pxl_gen::generate_shapes(&shapes, &palette)?;
    Ok((frame, palette))
}

fn svg_frames(
    spec: &SpriteSpec,
    w: u16,
    h: u16,
    profile: &SizeProfile,
    palette_ref: &str,
    res: &dyn Resolver,
    warnings: &mut Vec<String>,
) -> PxlResult<(Vec<(String, IndexFrame)>, Palette)> {
    let text = match (&spec.source.svg, &spec.source.file) {
        (Some(svg), _) => svg.clone(),
        (None, Some(file)) => res
            .read_text(file, crate::resolver::ResourceKind::Palette)?
            .ok_or_else(|| PxlError::io(format!("cannot read svg source '{file}'")))?,
        (None, None) => {
            return Err(
                PxlError::schema("sprite.no_input: svg source needs file or svg")
                    .with_path("/source"),
            )
        }
    };
    let palette = crate::load_palette(palette_ref, res)?;
    let color_mode = spec
        .raster
        .color_mode
        .as_deref()
        .and_then(pxl_svg::ColorMode::parse)
        .unwrap_or_default();
    let finish = if spec.finish.skip {
        None
    } else {
        Some(finish_options(&spec.finish, profile))
    };
    let opts = pxl_svg::SvgOptions {
        w,
        h,
        samples: spec.raster.samples.unwrap_or(0),
        color_mode,
        gradient_bands: spec.raster.gradient_bands.unwrap_or(4),
        strict: false,
        max_colors: profile.max_colors,
        view_box: None,
        snap: spec.raster.snap,
        finish,
    };
    let out = pxl_svg::rasterize(&text, &opts, &palette)?;
    warnings.extend(out.report.warnings());
    let frames = out
        .frames
        .into_iter()
        .map(|(name, f)| (name, f.frame))
        .collect();
    Ok((frames, out.palette))
}

/// Upscale options for a tier: hybrid geometry, tier ramps, no shading (the
/// finishing pass owns colour).
fn enhance_options(profile: &SizeProfile) -> pxl_art::EnhanceOptions {
    pxl_art::EnhanceOptions {
        upscale: pxl_art::Upscale::Hybrid,
        shade: pxl_art::ShadeMode::None,
        ramps: pxl_art::palette::RampOptions {
            levels: profile.ramp_levels.clamp(2, 4),
            hue_shift: 40,
            max_colors: profile.max_colors.min(pxl_core::MAX_TEXT_COLORS),
            generate: true,
        },
        ..Default::default()
    }
}

/// Finishing options from a spec, over the tier defaults.
pub fn finish_options(spec: &FinishSpec, profile: &SizeProfile) -> pxl_art::FinishOptions {
    let mut out = pxl_art::FinishOptions::for_profile(profile);
    if let Some(v) = spec
        .outline
        .as_deref()
        .and_then(pxl_art::OutlineMode::parse)
    {
        out.outline = v;
    }
    if let Some(v) = spec.shade.as_deref().and_then(pxl_art::ShadeMode::parse) {
        out.shade = v;
    }
    if let Some(v) = spec.aa.as_deref() {
        out.aa = match v {
            "off" | "none" => pxl_core::AaPolicy::Off,
            "corners" => pxl_core::AaPolicy::Corners,
            "on" | "all" => pxl_core::AaPolicy::On,
            _ => out.aa,
        };
    }
    if let Some(v) = spec.dither {
        out.dither = v;
    }
    if let Some(v) = spec.inner_lines {
        out.inner_lines = v;
    }
    if let Some(v) = spec.light.as_deref().and_then(pxl_art::Light::parse) {
        out.light = v;
    }
    if let Some(v) = spec.cleanup.as_deref() {
        out.cleanup = parse_cleanup(v);
    }
    out
}

fn parse_cleanup(text: &str) -> pxl_art::CleanOptions {
    match text {
        "none" => pxl_art::CleanOptions::NONE,
        "all" => pxl_art::CleanOptions::ALL,
        list => {
            let mut out = pxl_art::CleanOptions::NONE;
            for item in list.split(',').map(str::trim) {
                match item {
                    "orphan" | "orphans" => out.orphans = true,
                    "elbow" | "elbows" | "pixel_perfect" => out.elbows = true,
                    "notch" | "notches" => out.notches = true,
                    "rhythm" | "jaggy" => out.rhythm = true,
                    "symmetry" | "sym" => out.symmetry = true,
                    _ => {}
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::MemoryResolver;

    fn plain(res: &MemoryResolver, text: &str) -> PxlResult<BuiltSprite> {
        build_sprite(text, None, res)
    }

    #[test]
    fn inline_rows_build_a_document_at_the_spec_size() {
        let res = MemoryResolver::new();
        let spec = r#"{"v":3,"size":32,"palette":"p16","source":{"kind":"ascii",
            "rows":["001100","011110","111111","111111","011110","001100"]}}"#;
        let built = plain(&res, spec).unwrap();
        assert_eq!((built.doc.w, built.doc.h), (32, 32));
        let frame = built.doc.frame("base").unwrap();
        assert!(frame.count_opaque() > 0);
    }

    #[test]
    fn an_svg_source_rasterises_at_the_target_size() {
        let res = MemoryResolver::new();
        let spec = r##"{"v":3,"size":64,"palette":"p16","source":{"kind":"svg",
            "svg":"<svg viewBox=\"0 0 8 8\"><rect x=\"1\" y=\"1\" width=\"6\" height=\"6\" fill=\"#d04648\"/></svg>"}}"##;
        let built = plain(&res, spec).unwrap();
        let frame = built.doc.frame("base").unwrap();
        assert_eq!((frame.w, frame.h), (64, 64));
        assert!(frame.get(32, 32) != 0);
        assert_eq!(frame.get(1, 1), 0);
    }

    #[test]
    fn a_smaller_layout_can_be_upscaled_to_the_tier() {
        let res = MemoryResolver::new();
        let spec = r#"{"v":3,"size":128,"palette":"p16","source":{"kind":"ascii","native":32,
            "rows":["001100","011110","111111","111111","011110","001100"]}}"#;
        let built = plain(&res, spec).unwrap();
        let frame = built.doc.frame("base").unwrap();
        assert_eq!((frame.w, frame.h), (128, 128));
        assert!(built.palette.validate().is_ok());
    }
}
