//! Native-resolution finishing: the hand passes a pixel artist makes after the
//! shapes are down, applied to a frame that is already at its final size (an
//! SVG raster, a pixelised picture, an ASCII layout). Nothing is scaled.
//!
//! * `inner_lines`: a one-pixel line between two large regions whose colours
//!   are too close to read as separate parts, in the darker region's shadow.
//! * `corner_aa`: manual anti-aliasing. An inside step of the silhouette, or
//!   the corner of a stair-stepped edge between two fills, gets the
//!   intermediate ramp colour, but only where it softens a step and never on
//!   a one-pixel feature.
//! * `depillow`: pillow shading (a light ring that follows the outline on
//!   every side) is detected and the rim facing away from the light is moved
//!   to the shadow tone.
//! * `ramp_dither`: on large flat areas at the 256 tier, a two-row Bayer band
//!   where a fill meets its own shadow step, instead of a hard colour band.

use alloc::vec;
use alloc::vec::Vec;

use pxl_core::{AaPolicy, IndexFrame, OutlineStyle, Palette, PxlResult, SizeProfile};

use crate::analyze::{Material, SpriteGraph};
use crate::cleanup::{self, CleanOptions, CleanReport};
use crate::metrics::{self, Metrics};
use crate::outline::{self, OutlineMode, OutlineReport};
use crate::palette::{self, lightness, RampOptions, ShadeMap};
use crate::pipeline::Light;
use crate::quantize::BAYER4;
use crate::shade::{self, ShadeMode, ShadeReport};

const N4: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

#[derive(Clone, Debug)]
pub struct FinishOptions {
    pub cleanup: CleanOptions,
    /// Redraw the outer outline as one pixel (strip first).
    pub outline: OutlineMode,
    pub inner_lines: bool,
    pub shade: ShadeMode,
    pub aa: AaPolicy,
    pub dither: bool,
    pub depillow: bool,
    pub light: Light,
    pub ramps: RampOptions,
    pub materials: Option<Vec<Material>>,
    /// Pixels the caller marked as hand-placed detail; no pass edits them.
    pub protect: Option<Vec<bool>>,
    /// Anchor frame whose graph drives cleanup/material decisions for a sequence.
    pub decision_frame: Option<IndexFrame>,
    /// Palette and ramps generated once from the anchor frame.
    pub shared_shades: Option<(Palette, crate::palette::ShadeMap)>,
}

impl FinishOptions {
    /// Defaults for a size tier.
    pub fn for_profile(p: &SizeProfile) -> Self {
        Self {
            cleanup: CleanOptions {
                orphans: true,
                elbows: true,
                notches: p.side() >= 64,
                rhythm: p.rhythm,
                symmetry: true,
            },
            outline: match p.outline {
                OutlineStyle::Flat => OutlineMode::Flat,
                _ => OutlineMode::Selout,
            },
            inner_lines: p.outline == OutlineStyle::SeloutInner,
            shade: if p.shade {
                ShadeMode::Volume
            } else {
                ShadeMode::None
            },
            aa: p.aa,
            dither: p.dither,
            depillow: true,
            light: Light::TopLeft,
            ramps: RampOptions {
                levels: p.ramp_levels.clamp(2, 4),
                hue_shift: 40,
                max_colors: p.max_colors.min(pxl_core::MAX_TEXT_COLORS),
                generate: true,
            },
            materials: None,
            protect: None,
            decision_frame: None,
            shared_shades: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FinishReport {
    pub stages: Vec<&'static str>,
    pub clean: CleanReport,
    pub outline: OutlineReport,
    pub inner_lines: u32,
    pub shade: ShadeReport,
    pub aa: u32,
    pub pillow: u32,
    pub dither: u32,
    pub metrics: Option<Metrics>,
    pub warnings: Vec<alloc::string::String>,
}

#[derive(Clone, Debug)]
pub struct Finished {
    pub frame: IndexFrame,
    pub palette: Palette,
    pub report: FinishReport,
}

pub fn finish(frame: &IndexFrame, palette: &Palette, opts: &FinishOptions) -> PxlResult<Finished> {
    let mut report = FinishReport::default();
    let outline_index = palette.special.outline;
    let (mut work, _) = if opts.outline.strips() {
        report.stages.push("strip_outline");
        outline::strip_outer(frame, outline_index)
    } else {
        (frame.clone(), 0)
    };
    if opts.outline.strips() {
        if let Some(mask) = &opts.protect {
            for (index, protected) in mask.iter().enumerate() {
                if *protected && index < work.pixels.len() && frame.pixels[index] != 0 {
                    work.pixels[index] = frame.pixels[index];
                }
            }
        }
    }
    let decision = opts.decision_frame.as_ref().unwrap_or(&work);
    let mut graph = SpriteGraph::build(decision, palette, opts.materials.as_deref());
    if let Some(mask) = &opts.protect {
        if mask.len() != graph.protect.len() {
            return Err(pxl_core::PxlError::semantic(alloc::format!(
                "protect mask has {} pixels, expected {}",
                mask.len(),
                graph.protect.len()
            )));
        }
        graph.explicit_protect.clone_from(mask);
        for (p, m) in graph.protect.iter_mut().zip(mask) {
            *p |= *m;
        }
    }
    let mut usage = [0u32; 256];
    for &p in &work.pixels {
        usage[p as usize] += 1;
    }
    let wants_ramps = opts.outline.needs_ramps()
        || opts.shade != ShadeMode::None
        || opts.inner_lines
        || opts.aa != AaPolicy::Off
        || opts.depillow
        || opts.dither;
    let (pal, shades) = if wants_ramps {
        report.stages.push("ramps");
        if let Some((palette, shades)) = &opts.shared_shades {
            (palette.clone(), shades.clone())
        } else {
            let (p, s, r) = palette::build_shades(palette, &usage, opts.ramps);
            if !r.skipped.is_empty() {
                report.warnings.push(alloc::format!(
                    "palette cap {} reached: {} colour(s) have no ramp",
                    opts.ramps.max_colors,
                    r.skipped.len()
                ));
            }
            (p, s)
        }
    } else {
        (palette.clone(), ShadeMap::identity())
    };
    if opts.cleanup != CleanOptions::NONE {
        report.stages.push("cleanup");
        let native = work.clone();
        report.clean = cleanup::clean(&mut work, &native, 1, &graph, opts.cleanup);
    }
    let editable = |g: &SpriteGraph, x: i32, y: i32| g.editable(x, y);
    if opts.shade == ShadeMode::Volume {
        report.stages.push("shade");
        let mask: Vec<bool> = work.pixels.iter().map(|p| *p != 0).collect();
        report.shade = shade::apply(
            &mut work,
            &mask,
            1,
            &graph,
            &shades,
            pal.special.outline,
            opts.light,
        );
    }
    if opts.depillow {
        let n = depillow(&mut work, &pal, &shades, opts.light, &graph);
        if n > 0 {
            report.stages.push("depillow");
            report.pillow = n;
        }
    }
    if opts.inner_lines {
        report.stages.push("inner_lines");
        report.inner_lines = inner_lines(&mut work, &pal, &shades, &graph);
    }
    if opts.dither {
        report.stages.push("ramp_dither");
        report.dither = ramp_dither(&mut work, &shades, &graph);
    }
    if opts.outline.strips() && opts.outline != OutlineMode::None {
        report.stages.push("outline");
        report.outline = outline::apply(&mut work, &pal, &shades, opts.outline, opts.light);
    }
    if opts.aa != AaPolicy::Off {
        report.stages.push("aa");
        graph = SpriteGraph::build(&work, &pal, opts.materials.as_deref());
        if let Some(mask) = &opts.protect {
            graph.explicit_protect.clone_from(mask);
            for (p, m) in graph.protect.iter_mut().zip(mask) {
                *p |= *m;
            }
        }
        report.aa = corner_aa(&mut work, &pal, &shades, opts.aa, &|x, y| {
            editable(&graph, x, y)
        });
    }
    if let Some(mask) = &opts.protect {
        for (index, protected) in mask.iter().enumerate() {
            if *protected && frame.pixels[index] != 0 && work.pixels[index] != frame.pixels[index] {
                return Err(pxl_core::PxlError::semantic(alloc::format!(
                    "protected pixel {index} changed from {} to {} during finish",
                    frame.pixels[index],
                    work.pixels[index]
                )));
            }
        }
    }
    report.stages.push("measure");
    let g = SpriteGraph::build(frame, palette, opts.materials.as_deref());
    report.metrics = Some(metrics::measure(
        &work,
        frame,
        1,
        &g,
        &pal,
        pxl_core::Rgba::rgb(28, 26, 24),
    ));
    Ok(Finished {
        frame: work,
        palette: pal,
        report,
    })
}

/// Label same-colour 4-connected regions; returns labels and sizes.
fn regions(frame: &IndexFrame) -> (Vec<u32>, Vec<u32>) {
    let (w, h) = (frame.w as i32, frame.h as i32);
    let mut label = vec![u32::MAX; frame.pixels.len()];
    let mut sizes = Vec::new();
    let mut stack = Vec::new();
    for start in 0..frame.pixels.len() {
        if frame.pixels[start] == 0 || label[start] != u32::MAX {
            continue;
        }
        let id = sizes.len() as u32;
        let c = frame.pixels[start];
        let mut n = 0;
        label[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            n += 1;
            let (x, y) = (i as i32 % w, i as i32 / w);
            for (dx, dy) in N4 {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let j = (ny * w + nx) as usize;
                if label[j] == u32::MAX && frame.pixels[j] == c {
                    label[j] = id;
                    stack.push(j);
                }
            }
        }
        sizes.push(n);
    }
    (label, sizes)
}

/// Lightness gap (Q16) under which two touching fills need a line to read as
/// separate parts.
const INNER_GAP: i32 = 9000;
/// Regions smaller than this many pixels never get an inner line.
const INNER_MIN: u32 = 24;

fn inner_lines(
    frame: &mut IndexFrame,
    pal: &Palette,
    shades: &ShadeMap,
    graph: &SpriteGraph,
) -> u32 {
    let (w, h) = (frame.w as i32, frame.h as i32);
    let (label, sizes) = regions(frame);
    let src = frame.clone();
    let mut changed = 0;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let c = src.pixels[i];
            if c == 0 || c == pal.special.outline || !graph.editable(x, y) {
                continue;
            }
            if sizes[label[i] as usize] < INNER_MIN {
                continue;
            }
            // Only the darker side of a boundary takes the line, so it is one
            // pixel thick.
            for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
                let o = src.get(x + dx, y + dy);
                if o == 0 || o == c || o == pal.special.outline {
                    continue;
                }
                let j = ((y + dy) * w + x + dx) as usize;
                if sizes[label[j] as usize] < INNER_MIN {
                    continue;
                }
                let (lc, lo) = (lightness(pal, c), lightness(pal, o));
                if lc > lo || (lc == lo && c > o) || (lo - lc).abs() >= INNER_GAP * 2 {
                    continue;
                }
                let line = shades.shadow[c as usize];
                if line != c {
                    frame.pixels[i] = line;
                    changed += 1;
                }
                break;
            }
        }
    }
    changed
}

/// Pillow shading: the fill's light step on every side of a region. Rim
/// pixels facing away from the light take the shadow step instead.
fn depillow(
    frame: &mut IndexFrame,
    pal: &Palette,
    shades: &ShadeMap,
    light: Light,
    graph: &SpriteGraph,
) -> u32 {
    let (w, h) = (frame.w as i32, frame.h as i32);
    let (lx, ly) = light.vector();
    let src = frame.clone();
    // A colour is a "light rim" of base b when shades.light[b] == it.
    let mut base_of = [0u8; 256];
    for b in 1..pal.colors.len().min(256) {
        let l = shades.light[b];
        if l != b as u8 {
            base_of[l as usize] = b as u8;
        }
    }
    let mut rim_sides = [0u8; 256];
    for y in 0..h {
        for x in 0..w {
            let c = src.get(x, y);
            let b = base_of[c as usize];
            if c == 0 || b == 0 {
                continue;
            }
            for (k, (dx, dy)) in N4.iter().enumerate() {
                if src.get(x + dx, y + dy) == 0 || src.get(x + dx, y + dy) == pal.special.outline {
                    rim_sides[b as usize] |= 1 << k;
                }
            }
        }
    }
    let mut changed = 0;
    for y in 0..h {
        for x in 0..w {
            let c = src.get(x, y);
            let b = base_of[c as usize];
            if c == 0 || b == 0 || rim_sides[b as usize] != 0b1111 || !graph.editable(x, y) {
                continue;
            }
            // The rim faces away from the light when its open side points
            // against the light vector.
            let away = N4.iter().any(|(dx, dy)| {
                let n = src.get(x + dx, y + dy);
                let edge = n == 0 || n == pal.special.outline;
                edge && dx * lx + dy * ly < 0
            });
            if away {
                let s = shades.shadow[b as usize];
                if s != c {
                    frame.set(x, y, s);
                    changed += 1;
                }
            }
        }
    }
    changed
}

/// Manual AA on stair steps. For a clear pixel tucked into an inside corner
/// of the silhouette (two opaque 4-neighbours at right angles, both the same
/// fill, the diagonal between them opaque too), paint the fill's shadow step
/// when `On`; `Corners` limits it to steps of a run longer than one pixel.
fn corner_aa(
    frame: &mut IndexFrame,
    pal: &Palette,
    shades: &ShadeMap,
    policy: AaPolicy,
    editable: &dyn Fn(i32, i32) -> bool,
) -> u32 {
    let (w, h) = (frame.w as i32, frame.h as i32);
    let src = frame.clone();
    let outline = pal.special.outline;
    let mut changed = 0;
    for y in 0..h {
        for x in 0..w {
            let c = src.get(x, y);
            if c == 0 || c == outline {
                continue;
            }
            // Edge pixel (art side) where the outline steps diagonally.
            for (ax, ay, bx, by) in [(1, 0, 0, 1), (-1, 0, 0, 1), (1, 0, 0, -1), (-1, 0, 0, -1)] {
                let a = src.get(x + ax, y + ay);
                let b = src.get(x + bx, y + by);
                let d = src.get(x + ax + bx, y + ay + by);
                // The step: one neighbour is outline, the other fill, the
                // diagonal is outline: this pixel sits on the inside corner.
                if !(a == outline && b == c && d == outline) {
                    continue;
                }
                if !editable(x, y) {
                    continue;
                }
                if policy == AaPolicy::Corners {
                    // Only the outer silhouette: the outline pixel touches
                    // empty space.
                    let outer = N4
                        .iter()
                        .any(|(dx, dy)| src.get(x + ax + dx, y + ay + dy) == 0);
                    if !outer {
                        continue;
                    }
                }
                let s = shades.shadow[c as usize];
                if s != c && s != 0 {
                    frame.set(x, y, s);
                    changed += 1;
                }
                break;
            }
        }
    }
    changed
}

/// Where a large fill meets its own shadow step along a horizontal or
/// vertical edge, swap pixels in a two-pixel band by the Bayer pattern.
fn ramp_dither(frame: &mut IndexFrame, shades: &ShadeMap, graph: &SpriteGraph) -> u32 {
    let (w, h) = (frame.w as i32, frame.h as i32);
    let (label, sizes) = regions(frame);
    let src = frame.clone();
    let mut changed = 0;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let c = src.pixels[i];
            if c == 0 || sizes[label[i] as usize] < 64 || !graph.editable(x, y) {
                continue;
            }
            let s = shades.shadow[c as usize];
            if s == c {
                continue;
            }
            for (dx, dy) in N4 {
                // Within two pixels of the shadow region, on the base side.
                let near = (1..=2).any(|k| src.get(x + dx * k, y + dy * k) == s);
                if !near {
                    continue;
                }
                let dist = if src.get(x + dx, y + dy) == s { 1 } else { 2 };
                let t = BAYER4[(y & 3) as usize][(x & 3) as usize];
                let threshold = if dist == 1 { 8 } else { 3 };
                if t < threshold {
                    frame.pixels[i] = s;
                    changed += 1;
                }
                break;
            }
        }
    }
    changed
}

#[cfg(test)]
#[path = "finish_tests.rs"]
mod tests;
