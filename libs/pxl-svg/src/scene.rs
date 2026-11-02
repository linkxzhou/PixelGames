//! Walk the SVG tree and paint it: styles, transforms, `use`, clip paths,
//! gradients, frames, size filtering and feature protection.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use pxl_art::{FinishOptions, Material};
use pxl_core::{IndexFrame, Palette, PxlError, PxlResult, Rgba, SizeProfile};

use crate::geom::{self, Affine, Cap, Join, PathBuilder, Poly, Pt};
use crate::num;
use crate::palette_map::{ColorMode, ColorReport, ColorResolver};
use crate::raster::{self, Canvas, FillRule};
use crate::xml::Node;

/// How many samples per pixel when the caller does not say.
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

#[derive(Clone, Debug)]
enum Paint {
    Solid(u8),
    /// Hard bands along `from` → `to`; sample projected on the axis picks one.
    Bands {
        from: Pt,
        to: Pt,
        bands: Vec<u8>,
    },
}

impl Paint {
    fn at(&self, p: Pt) -> u8 {
        match self {
            Paint::Solid(c) => *c,
            Paint::Bands { from, to, bands } => {
                let (dx, dy) = (to.0 - from.0, to.1 - from.1);
                let len2 = dx * dx + dy * dy;
                if len2 <= 0.0 || bands.is_empty() {
                    return bands.first().copied().unwrap_or(0);
                }
                let t = ((p.0 - from.0) * dx + (p.1 - from.1) * dy) / len2;
                let i = (t * bands.len() as f64).floor();
                let i = if i.is_finite() { i as i64 } else { 0 };
                bands[i.clamp(0, bands.len() as i64 - 1) as usize]
            }
        }
    }
}

#[derive(Clone)]
struct Shape {
    /// Shapes of one element (its fill and its stroke) share this.
    logical: u32,
    frame: Option<String>,
    material: Option<Material>,
    /// `eye`, `highlight`, `shadow`, `detail`, `outline`.
    role: Option<String>,
    min: Option<u32>,
    name: String,
    poly_groups: Vec<Vec<Pt>>,
    rule: FillRule,
    union: bool,
    paint: Paint,
    clip: Option<Vec<Vec<Pt>>>,
}

struct Ctx<'a> {
    opts: &'a SvgOptions,
    colors: ColorResolver,
    warnings: Vec<String>,
    unsupported: Vec<String>,
    ids: BTreeMap<String, &'a Node>,
    using: Vec<String>,
    shapes: Vec<Shape>,
    order: u32,
    logical: u32,
    frame: Option<String>,
    outline_role: bool,
    size_filtered: usize,
}

#[derive(Clone, Default)]
struct Style {
    fill: Option<String>,
    fill_opacity: Option<f64>,
    stroke: Option<String>,
    stroke_width: Option<f64>,
    linejoin: Join,
    linecap: Cap,
    miter: f64,
    rule: FillRule,
    hidden: bool,
    clip: Option<String>,
    material: Option<String>,
    role: Option<String>,
    ramp: Option<String>,
    step: i32,
    min: Option<u32>,
    sizes: Option<Vec<u16>>,
    gradient: Option<String>,
}

impl Style {
    fn base() -> Self {
        Self {
            fill: Some("#000000".into()),
            linejoin: Join::Miter,
            linecap: Cap::Butt,
            miter: 4.0,
            ..Default::default()
        }
    }

    fn inherit(parent: &Style) -> Self {
        let mut s = parent.clone();
        s.gradient = None;
        s
    }
}

/// Inline `style="a:b;c:d"` beats presentation attributes.
fn style_pairs(node: &Node) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = node
        .attrs
        .iter()
        .filter(|(k, _)| k.as_str() != "style")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if let Some(s) = node.attr("style") {
        for decl in s.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                let (k, v) = (k.trim().to_string(), v.trim().to_string());
                if !k.is_empty() {
                    if let Some(e) = out.iter_mut().find(|(a, _)| *a == k) {
                        e.1 = v;
                    } else {
                        out.push((k, v));
                    }
                }
            }
        }
    }
    out
}

fn parse_style(node: &Node, parent: &Style) -> Style {
    let mut s = Style::inherit(parent);
    for (k, v) in style_pairs(node) {
        match k.as_str() {
            "fill" => s.fill = Some(v.clone()),
            "stroke" => s.stroke = Some(v.clone()),
            "stroke-width" => s.stroke_width = num::number(&v),
            "fill-opacity" | "opacity" => {
                if let Some(o) = num::number(&v) {
                    s.fill_opacity = Some(o);
                }
            }
            "stroke-linejoin" => {
                s.linejoin = match v.as_str() {
                    "round" => Join::Round,
                    "bevel" => Join::Bevel,
                    _ => Join::Miter,
                }
            }
            "stroke-linecap" => {
                s.linecap = match v.as_str() {
                    "round" => Cap::Round,
                    "square" => Cap::Square,
                    _ => Cap::Butt,
                }
            }
            "stroke-miterlimit" => s.miter = num::number(&v).unwrap_or(4.0),
            "fill-rule" | "clip-rule" => {
                s.rule = if v == "evenodd" {
                    FillRule::EvenOdd
                } else {
                    FillRule::NonZero
                }
            }
            "display" | "visibility" if v == "none" || v == "hidden" => s.hidden = true,
            "clip-path" => {
                s.clip = Some(
                    v.trim_matches(|c| {
                        c == 'u' || c == 'r' || c == 'l' || c == '(' || c == ')' || c == '#'
                    })
                    .to_string(),
                )
            }
            // Model-facing hints.
            "data-material" => s.material = Some(v.clone()),
            "data-role" => s.role = Some(v.to_ascii_lowercase()),
            "data-ramp" => s.ramp = Some(v.clone()),
            "data-step" => s.step = num::number(&v).unwrap_or(0.0) as i32,
            "data-min" => s.min = num::number(&v).map(|n| n.max(1.0) as u32),
            "data-sizes" => {
                s.sizes = Some(
                    v.split(',')
                        .filter_map(|p| num::number(p.trim()).map(|n| n as u16))
                        .collect(),
                )
            }
            _ => {}
        }
    }
    s
}

fn paint_url(v: &str) -> Option<String> {
    let t = v.trim();
    let inner = t.strip_prefix("url(")?.strip_suffix(')')?;
    Some(inner.trim().trim_start_matches('#').to_string())
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

/// Output transform and the user-space `viewBox`.
fn view(root: &Node, opts: &SvgOptions) -> PxlResult<(Vec<f64>, Affine)> {
    if let Some(v) = opts.view_box {
        let m = Affine::scale(opts.w as f64 / v[2], opts.h as f64 / v[3])
            .then(&Affine::translate(-v[0], -v[1]));
        return Ok((alloc::vec![v[0], v[1], v[2], v[3]], m));
    }
    if let Some(text) = root.attr("viewBox") {
        let v = num::number_list(text);
        if v.len() == 4 && v[2] != 0.0 && v[3] != 0.0 {
            // Meet, not stretch: one scale for both axes keeps circles round.
            let s = (opts.w as f64 / v[2]).min(opts.h as f64 / v[3]);
            let dx = (opts.w as f64 - v[2] * s) / 2.0;
            let dy = (opts.h as f64 - v[3] * s) / 2.0;
            let m = Affine::translate(dx, dy)
                .then(&Affine::scale(s, s))
                .then(&Affine::translate(-v[0], -v[1]));
            return Ok((v, m));
        }
    }
    let (w, h) = match (root.attr("width"), root.attr("height")) {
        (Some(w), Some(h)) => (num::number(w), num::number(h)),
        _ => (None, None),
    };
    match (w, h) {
        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => {
            let s = (opts.w as f64 / w).min(opts.h as f64 / h);
            Ok((alloc::vec![0.0, 0.0, w, h], Affine::scale(s, s)))
        }
        _ => Ok((
            alloc::vec![0.0, 0.0, opts.w as f64, opts.h as f64],
            Affine::IDENTITY,
        )),
    }
}

impl<'a> Ctx<'a> {
    #[allow(clippy::too_many_arguments)]
    fn walk(
        &mut self,
        node: &'a Node,
        parent_m: Affine,
        inherited: &Style,
        _vb: Vec<f64>,
    ) -> PxlResult<()> {
        let m = match node.attr("transform") {
            Some(t) => match geom::parse_transform(t) {
                Ok(t) => parent_m.then(&t),
                Err(e) => {
                    self.unsupported.push(format!("{e} on <{}>", node.name));
                    parent_m
                }
            },
            None => parent_m,
        };
        let mut style = parse_style(node, inherited);
        if let Some(url) = style.fill.as_deref().and_then(paint_url) {
            style.gradient = Some(url);
        }
        if let Some(list) = node.attr("data-sizes") {
            if !self.size_matches(list) {
                self.size_filtered += 1;
                return Ok(());
            }
        }
        if node
            .attr("data-role")
            .map_or(false, |r| r.eq_ignore_ascii_case("outline"))
        {
            self.outline_role = true;
        }
        let saved_frame = self.frame.clone();
        if let Some(name) = node.attr("data-frame") {
            let m = if self.opts.snap {
                Affine::translate(m.e.round(), m.f.round())
            } else {
                m
            };
            self.frame = Some(name.to_string());
            self.draw_children(node, m, &style)?;
            self.frame = saved_frame;
            return Ok(());
        }
        match node.name.as_str() {
            "g" | "svg" | "symbol" | "a" => self.draw_children(node, m, &style)?,
            "defs" => {}
            "use" => self.use_node(node, m, &style, 0)?,
            "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "path" => {
                self.shape(node, m, &style)?
            }
            "linearGradient" | "radialGradient" | "clipPath" | "title" | "desc" | "metadata"
            | "style" => {}
            other => self.unsupported.push(format!(
                "svg.unsupported: <{other}> is not in the subset and was ignored"
            )),
        }
        Ok(())
    }

    fn size_matches(&self, list: &str) -> bool {
        let side = self.opts.w.max(self.opts.h);
        list.split(',')
            .filter_map(|p| num::number(p.trim()).map(|n| n as u16))
            .any(|n| n == side)
    }

    fn draw_children(&mut self, node: &'a Node, m: Affine, style: &Style) -> PxlResult<()> {
        for child in &node.children {
            self.walk(child, m, style, Vec::new())?;
        }
        Ok(())
    }

    fn use_node(
        &mut self,
        node: &'a Node,
        m: Affine,
        style: &Style,
        depth: usize,
    ) -> PxlResult<()> {
        if depth > 8 {
            self.unsupported
                .push("svg.use_depth: <use> nested deeper than 8".into());
            return Ok(());
        }
        let Some(href) = node.attr("href").or_else(|| node.attr("xlink:href")) else {
            self.unsupported
                .push("svg.use_href: <use> without href".into());
            return Ok(());
        };
        let id = href.trim_start_matches('#').to_string();
        if self.using.contains(&id) {
            return Err(PxlError::schema(format!(
                "svg.use_cycle: <use> #{id} refers back to itself"
            )));
        }
        let Some(target) = self.ids.get(&id).copied() else {
            self.unsupported
                .push(format!("svg.use_missing: no element with id '{id}'"));
            return Ok(());
        };
        let x = node.attr("x").and_then(num::number).unwrap_or(0.0);
        let y = node.attr("y").and_then(num::number).unwrap_or(0.0);
        let m = m.then(&Affine::translate(x, y));
        let mut inner = parse_style(node, style);
        inner.fill = style.fill.clone();
        inner.gradient = style.gradient.clone();
        self.using.push(id);
        let r = if target.name == "symbol" {
            self.draw_children(target, m, &inner)
        } else {
            self.walk(target, m, &inner, Vec::new())
        };
        self.using.pop();
        r
    }

    fn shape(&mut self, node: &'a Node, m: Affine, style: &Style) -> PxlResult<()> {
        if style.hidden {
            return Ok(());
        }
        let mut b = PathBuilder::new(m);
        let a = |k: &str, d: f64| node.attr(k).and_then(num::number).unwrap_or(d);
        match node.name.as_str() {
            "rect" => geom::rect(
                &mut b,
                a("x", 0.0),
                a("y", 0.0),
                a("width", 0.0),
                a("height", 0.0),
                a("rx", a("ry", 0.0)),
                a("ry", a("rx", 0.0)),
            ),
            "circle" => {
                let r = a("r", 0.0);
                geom::ellipse(&mut b, a("cx", 0.0), a("cy", 0.0), r, r)
            }
            "ellipse" => geom::ellipse(
                &mut b,
                a("cx", 0.0),
                a("cy", 0.0),
                a("rx", 0.0),
                a("ry", 0.0),
            ),
            "line" => {
                b.move_to((a("x1", 0.0), a("y1", 0.0)));
                b.line_to((a("x2", 0.0), a("y2", 0.0)));
            }
            "polyline" => geom::points(&mut b, node.attr("points").unwrap_or(""), false),
            "polygon" => geom::points(&mut b, node.attr("points").unwrap_or(""), true),
            "path" => {
                let d = node.attr("d").unwrap_or("");
                if let Err(e) = geom::path(d, &mut b) {
                    return Err(PxlError::schema(format!("svg.path: {e}"))
                        .with_path("/d")
                        .with_hint("path data is a single string of commands and numbers"));
                }
            }
            _ => return Ok(()),
        }
        let polys = b.finish();
        if polys.is_empty() {
            return Ok(());
        }
        let clip = self.clip_of(style, m)?;
        let name = shape_name(node);
        let material = style.material.as_deref().and_then(Material::parse);
        self.logical += 1;
        let logical = self.logical;

        // Fill.
        if let Some(fill) = style.fill.as_deref() {
            if fill != "none" {
                let opacity = style.fill_opacity.unwrap_or(1.0);
                if opacity < 0.5 {
                    self.warnings.push(format!(
                        "svg.opacity: {name} is drawn at {} opacity; treated as clear",
                        opacity
                    ));
                } else {
                    let paint = self.paint_of(fill, style, &polys)?;
                    self.push_shape(
                        logical,
                        name.clone(),
                        &polys,
                        style,
                        paint,
                        clip.clone(),
                        material,
                    );
                }
            }
        }
        // Stroke.
        if let Some(stroke) = style.stroke.as_deref() {
            if stroke != "none" && style.stroke_width.unwrap_or(1.0) > 0.0 {
                let width = style.stroke_width.unwrap_or(1.0) * m.mean_scale();
                let pieces =
                    geom::stroke(&polys, width, style.linejoin, style.linecap, style.miter);
                let paint = self.paint_of(stroke, style, &polys)?;
                let as_polys: Vec<Poly> = pieces
                    .into_iter()
                    .map(|pts| Poly { pts, closed: true })
                    .collect();
                self.push_shape(
                    logical,
                    format!("{name} stroke"),
                    &as_polys,
                    style,
                    paint,
                    clip,
                    material,
                );
            }
        }
        Ok(())
    }

    fn push_shape(
        &mut self,
        logical: u32,
        name: String,
        polys: &[Poly],
        style: &Style,
        paint: Paint,
        clip: Option<Vec<Vec<Pt>>>,
        material: Option<Material>,
    ) {
        let groups: Vec<Vec<Pt>> = polys.iter().map(|p| p.pts.clone()).collect();
        let union = style.stroke.is_some() && name.ends_with("stroke");
        self.order += 1;
        self.shapes.push(Shape {
            logical,
            frame: self.frame.clone(),
            material,
            role: style.role.clone(),
            min: style.min,
            name,
            poly_groups: groups,
            rule: style.rule,
            union,
            paint,
            clip,
        });
    }

    fn paint_of(&mut self, spec: &str, style: &Style, polys: &[Poly]) -> PxlResult<Paint> {
        if let Some(id) = paint_url(spec) {
            return self.gradient(&id, polys);
        }
        if let Some(ramp) = &style.ramp {
            return Ok(Paint::Solid(self.colors.ramp_index(ramp, style.step)?));
        }
        let color = match spec.trim() {
            "none" => Rgba::TRANSPARENT,
            other => pxl_core::parse_hex(other).map_err(|_| {
                PxlError::schema(format!(
                    "svg.bad_color: '{other}' is not #rgb, #rrggbb or url(#id)"
                ))
            })?,
        };
        Ok(Paint::Solid(self.colors.index_of(color)?))
    }

    fn gradient(&mut self, id: &str, polys: &[Poly]) -> PxlResult<Paint> {
        let Some(node) = self.ids.get(id).copied() else {
            return Err(PxlError::schema(format!(
                "svg.missing_gradient: no element with id '{id}'"
            )));
        };
        let stops: Vec<(f64, u8)> = node
            .children
            .iter()
            .filter(|c| c.name == "stop")
            .filter_map(|c| {
                let color = c
                    .attr("stop-color")
                    .or_else(|| {
                        c.attr("style").and_then(|s| {
                            s.split(';')
                                .find_map(|d| d.trim().strip_prefix("stop-color:").map(str::trim))
                        })
                    })
                    .unwrap_or("#000000");
                let off = c.attr("offset").and_then(num::offset).unwrap_or(0.0);
                let rgba = pxl_core::parse_hex(color).ok()?;
                let idx = self.colors.index_of(rgba).ok()?;
                Some((off, idx))
            })
            .collect();
        if stops.len() < 2 {
            return Err(PxlError::schema(format!(
                "svg.gradient_stops: gradient '{id}' needs at least two stop elements"
            ))
            .with_hint("give each stop a stop-color and an offset"));
        }
        let bands = self.opts.gradient_bands.max(2);
        let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in polys.iter().flat_map(|p| p.pts.iter()) {
            minx = minx.min(p.0);
            maxx = maxx.max(p.0);
            miny = miny.min(p.1);
            maxy = maxy.max(p.1);
        }
        if maxx <= minx || maxy <= miny {
            return Ok(Paint::Solid(stops[0].1));
        }
        let radial = node.name.starts_with("radial");
        let (from, to) = if radial {
            let c = ((minx + maxx) / 2.0, (miny + maxy) / 2.0);
            let r = ((maxx - minx).max(maxy - miny)) / 2.0;
            (c, (c.0 + r, c.1))
        } else if node.attr("gradientUnits") == Some("userSpaceOnUse") {
            let g = |k: &str, d: f64| node.attr(k).and_then(num::number).unwrap_or(d);
            (
                (g("x1", minx), g("y1", miny)),
                (g("x2", maxx), g("y2", maxy)),
            )
        } else {
            // objectBoundingBox: top to bottom of the shape.
            ((0.0, miny), (0.0, maxy))
        };
        // Two stops are an explicit hard ramp of `bands` entries; more stops
        // are sampled per band, so a flat colour band never splits a stop.
        let all = if stops.len() == 2 {
            self.colors.bands(stops[0].1, stops[1].1, bands)?
        } else {
            let mut v = Vec::with_capacity(bands);
            for i in 0..bands {
                v.push(stop_at(&stops, (i as f64 + 0.5) / bands as f64));
            }
            v
        };
        Ok(Paint::Bands {
            from,
            to,
            bands: all,
        })
    }

    fn clip_of(&mut self, style: &Style, m: Affine) -> PxlResult<Option<Vec<Vec<Pt>>>> {
        let Some(text) = &style.clip else {
            return Ok(None);
        };
        let id = text.trim().trim_start_matches('#').to_string();
        let Some(node) = self.ids.get(&id).copied() else {
            self.unsupported
                .push(format!("svg.clip_missing: no element with id '{id}'"));
            return Ok(None);
        };
        let mut out: Vec<Vec<Pt>> = Vec::new();
        collect_clip(node, m, &mut out);
        Ok(if out.is_empty() { None } else { Some(out) })
    }
}

fn collect_clip(node: &Node, m: Affine, out: &mut Vec<Vec<Pt>>) {
    let m = match node.attr("transform") {
        Some(t) => match geom::parse_transform(t) {
            Ok(t) => m.then(&t),
            Err(_) => m,
        },
        None => m,
    };
    let mut b = PathBuilder::new(m);
    let a = |k: &str, d: f64| node.attr(k).and_then(num::number).unwrap_or(d);
    match node.name.as_str() {
        "rect" => geom::rect(
            &mut b,
            a("x", 0.0),
            a("y", 0.0),
            a("width", 0.0),
            a("height", 0.0),
            a("rx", a("ry", 0.0)),
            a("ry", a("rx", 0.0)),
        ),
        "circle" => {
            let r = a("r", 0.0);
            geom::ellipse(&mut b, a("cx", 0.0), a("cy", 0.0), r, r)
        }
        "ellipse" => geom::ellipse(
            &mut b,
            a("cx", 0.0),
            a("cy", 0.0),
            a("rx", 0.0),
            a("ry", 0.0),
        ),
        "polygon" => geom::points(&mut b, node.attr("points").unwrap_or(""), true),
        "path" => {
            let _ = geom::path(node.attr("d").unwrap_or(""), &mut b);
        }
        _ => {}
    }
    for p in b.finish() {
        out.push(p.pts);
    }
    for c in &node.children {
        collect_clip(c, m, out);
    }
}

/// Pixel holding the centre of a shape's bounding box.
fn centroid_pixel(s: &Shape, w: usize, h: usize) -> Option<(usize, usize)> {
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in s.poly_groups.iter().flat_map(|g| g.iter()) {
        minx = minx.min(p.0);
        maxx = maxx.max(p.0);
        miny = miny.min(p.1);
        maxy = maxy.max(p.1);
    }
    if maxx < minx || maxy < miny {
        return None;
    }
    let x = (((minx + maxx) / 2.0).floor().max(0.0) as usize).min(w - 1);
    let y = (((miny + maxy) / 2.0).floor().max(0.0) as usize).min(h - 1);
    Some((x, y))
}

fn stop_at(stops: &[(f64, u8)], t: f64) -> u8 {
    let mut prev = stops[0];
    for s in stops {
        if t <= s.0 {
            return if s == &stops[0] { s.1 } else { prev.1 };
        }
        prev = *s;
    }
    stops[stops.len() - 1].1
}

fn shape_name(node: &Node) -> String {
    match node.attr("id") {
        Some(id) => format!("<{} #{}>", node.name, id),
        None => format!("<{}>", node.name),
    }
}

type Rendered = (
    IndexFrame,
    Vec<bool>,
    Option<Vec<Material>>,
    Vec<String>,
    u32,
);

fn render_frame(
    ctx: &mut Ctx<'_>,
    frame: Option<&str>,
    profile: &SizeProfile,
    _side: u16,
) -> PxlResult<Rendered> {
    let (w, h) = (ctx.opts.w as usize, ctx.opts.h as usize);
    let n = ctx.opts.samples_per_pixel();
    let mut canvas = Canvas::new(w, h, n);
    let (gw, gh) = (canvas.gw(), canvas.gh());
    let mut lost = Vec::new();
    let mut empty_details: Vec<&Shape> = Vec::new();
    let mut materials: Vec<Option<Material>> = vec![None; w * h];
    let mut roles: Vec<Option<String>> = vec![None; w * h];
    for shape in &ctx.shapes {
        if shape.frame.as_deref() != frame {
            continue;
        }
        if !shape.poly_groups.iter().any(|g| g.len() >= 3) {
            continue;
        }
        let mask = raster::fill(&shape.poly_groups, shape.rule, shape.union, n, gw, gh);
        if mask.count() == 0 {
            // Too small for the sample grid: a marked detail still gets its one
            // pixel after the downsample, anything else is reported.
            if shape.min.is_some() || shape.role.is_some() {
                empty_details.push(shape);
            } else {
                lost.push(shape.name.clone());
            }
            continue;
        }
        let clip = match &shape.clip {
            Some(clip) => {
                let m = raster::fill(clip, FillRule::NonZero, true, n, gw, gh);
                Some(m.full(gw, gh))
            }
            None => None,
        };
        let paint = shape.paint.clone();
        let mut closure = |sx: usize, sy: usize| {
            // Sample centre, back in pixel space.
            let p = ((sx as f64 + 0.5) / n as f64, (sy as f64 + 0.5) / n as f64);
            paint.at(p)
        };
        canvas.paint(&mask, clip.as_deref(), shape.logical, &mut closure);
    }
    let resolved = raster::resolve(&canvas);
    let mut frame_out = IndexFrame::new(w as u16, h as u16)?;
    frame_out.pixels = resolved.pixels;
    // Materials and roles follow the pixel's winning shape, not a guess from
    // the sample grid: a pixel belongs to whatever was drawn on top of it.
    let mut by_logical: BTreeMap<u32, (Option<Material>, Option<String>)> = BTreeMap::new();
    for s in &ctx.shapes {
        if s.frame.as_deref() == frame {
            let e = by_logical.entry(s.logical).or_default();
            e.0 = s.material.or(e.0.take());
            e.1 = s.role.clone().or(e.1.take());
        }
    }
    for (i, id) in resolved.owner.iter().enumerate() {
        if let Some((m, r)) = by_logical.get(id) {
            if m.is_some() {
                materials[i] = *m;
            }
            roles[i] = r.clone();
        }
    }
    let mut protect = vec![false; w * h];
    let mut forced = 0u32;
    for shape in &empty_details {
        if let Some((x, y)) = centroid_pixel(shape, w, h) {
            let col = shape.paint.at((x as f64 + 0.5, y as f64 + 0.5));
            if col != 0 {
                frame_out.pixels[y * w + x] = col;
                protect[y * w + x] = true;
                forced += 1;
            }
        }
        lost.push(shape.name.clone());
    }
    let forced_detail = !empty_details.is_empty();
    if ctx.opts.finish.is_some()
        || ctx
            .shapes
            .iter()
            .any(|s| s.role.is_some() || s.min.is_some())
    {
        let vis = raster::visibility(&canvas);
        let mut by_logical: BTreeMap<u32, (u32, Option<String>, Option<u32>, String)> =
            BTreeMap::new();
        for s in &ctx.shapes {
            if s.frame.as_deref() != frame || (s.role.is_none() && s.min.is_none()) {
                continue;
            }
            let e = by_logical
                .entry(s.logical)
                .or_insert((0, None, None, s.name.clone()));
            e.1 = s.role.clone().or(e.1.take());
            e.2 = s.min.or(e.2);
        }
        for (id, (_, role, min, name)) in by_logical {
            let pixels = vis.get(&id).cloned().unwrap_or_default();
            let keep = min.unwrap_or(if role.is_some() {
                profile.min_feature.max(1) as u32
            } else {
                0
            });
            if keep == 0 {
                continue;
            }
            for (i, _, _) in &pixels {
                protect[*i] = true;
            }
            if pixels.len() as u32 >= keep {
                continue;
            }
            // Keep the detail: force the best-covered pixels, or the pixel under
            // the shape's centre when the sample grid missed it entirely.
            let mut ranked = pixels.clone();
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            if ranked.is_empty() {
                if let Some(shape) = ctx
                    .shapes
                    .iter()
                    .find(|s| s.logical == id && s.frame.as_deref() == frame && s.min == min)
                {
                    if let Some(fallback) = centroid_pixel(shape, w, h) {
                        let col = shape
                            .paint
                            .at((fallback.0 as f64 + 0.5, fallback.1 as f64 + 0.5));
                        frame_out.pixels[fallback.1 * w + fallback.0] = col;
                        protect[fallback.1 * w + fallback.0] = true;
                        forced += 1;
                    }
                }
            }
            for k in pixels.len() as u32..keep {
                let Some((i, _, col)) = ranked.get(k as usize).or_else(|| ranked.first()) else {
                    break;
                };
                frame_out.pixels[*i] = *col;
                protect[*i] = true;
                forced += 1;
            }
            if pixels.is_empty() && !forced_detail {
                lost.push(name);
            }
        }
    }
    let materials = if materials.iter().any(|m| m.is_some()) {
        Some(
            materials
                .iter()
                .map(|m| m.unwrap_or(Material::Organic))
                .collect(),
        )
    } else {
        None
    };
    let _ = roles;
    Ok((frame_out, protect, materials, lost, forced))
}
