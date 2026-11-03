//! Tree walk: viewBox, Ctx, shape emission, clips and use-nodes.
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use pxl_art::Material;
use pxl_core::{PxlError, PxlResult, Rgba};

use crate::geom::{self, Affine, PathBuilder, Poly, Pt};
use crate::num;
use crate::palette_map::ColorResolver;
use crate::raster::{FillRule};
use crate::xml::Node;

use super::style::{parse_style, paint_url, stop_at, Paint, Style};
use super::SvgOptions;

#[derive(Clone)]
pub(crate) struct Shape {
    /// Shapes of one element (its fill and its stroke) share this.
    pub(crate) logical: u32,
    pub(crate) frame: Option<String>,
    pub(crate) material: Option<Material>,
    /// `eye`, `highlight`, `shadow`, `detail`, `outline`.
    pub(crate) role: Option<String>,
    pub(crate) min: Option<u32>,
    pub(crate) name: String,
    pub(crate) poly_groups: Vec<Vec<Pt>>,
    pub(crate) rule: FillRule,
    pub(crate) union: bool,
    pub(crate) paint: Paint,
    pub(crate) clip: Option<Vec<Vec<Pt>>>,
}

pub(crate) struct Ctx<'a> {
    pub(crate) opts: &'a SvgOptions,
    pub(crate) colors: ColorResolver,
    pub(crate) warnings: Vec<String>,
    pub(crate) unsupported: Vec<String>,
    pub(crate) ids: BTreeMap<String, &'a Node>,
    pub(crate) using: Vec<String>,
    pub(crate) shapes: Vec<Shape>,
    pub(crate) order: u32,
    pub(crate) logical: u32,
    pub(crate) frame: Option<String>,
    pub(crate) outline_role: bool,
    pub(crate) size_filtered: usize,
}

pub(crate) fn view(root: &Node, opts: &SvgOptions) -> PxlResult<(Vec<f64>, Affine)> {
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
    pub(crate) fn walk(
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


pub(crate) fn shape_name(node: &Node) -> String {
    match node.attr("id") {
        Some(id) => format!("<{} #{}>", node.name, id),
        None => format!("<{}>", node.name),
    }
}
