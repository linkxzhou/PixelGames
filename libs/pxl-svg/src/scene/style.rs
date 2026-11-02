//! SVG presentation attributes and paints (solid / gradient bands).
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::geom::{Join, Cap, Pt};
use crate::num;
use crate::raster::FillRule;
use crate::xml::Node;

#[derive(Clone, Debug)]
pub(crate) enum Paint {
    Solid(u8),
    /// Hard bands along `from` → `to`; sample projected on the axis picks one.
    Bands {
        from: Pt,
        to: Pt,
        bands: Vec<u8>,
    },
}

impl Paint {
    pub(crate) fn at(&self, p: Pt) -> u8 {
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

#[derive(Clone, Default)]
pub(crate) struct Style {
    pub(crate) fill: Option<String>,
    pub(crate) fill_opacity: Option<f64>,
    pub(crate) stroke: Option<String>,
    pub(crate) stroke_width: Option<f64>,
    pub(crate) linejoin: Join,
    pub(crate) linecap: Cap,
    pub(crate) miter: f64,
    pub(crate) rule: FillRule,
    pub(crate) hidden: bool,
    pub(crate) clip: Option<String>,
    pub(crate) material: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) ramp: Option<String>,
    pub(crate) step: i32,
    pub(crate) min: Option<u32>,
    pub(crate) sizes: Option<Vec<u16>>,
    pub(crate) gradient: Option<String>,
}

impl Style {
    pub(crate) fn base() -> Self {
        Self {
            fill: Some("#000000".into()),
            linejoin: Join::Miter,
            linecap: Cap::Butt,
            miter: 4.0,
            ..Default::default()
        }
    }

    pub(crate) fn inherit(parent: &Style) -> Self {
        let mut s = parent.clone();
        s.gradient = None;
        s
    }
}

/// Inline `style="a:b;c:d"` beats presentation attributes.
pub(crate) fn style_pairs(node: &Node) -> Vec<(String, String)> {
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

pub(crate) fn parse_style(node: &Node, parent: &Style) -> Style {
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

pub(crate) fn paint_url(v: &str) -> Option<String> {
    let t = v.trim();
    let inner = t.strip_prefix("url(")?.strip_suffix(')')?;
    Some(inner.trim().trim_start_matches('#').to_string())
}

pub(crate) fn stop_at(stops: &[(f64, u8)], t: f64) -> u8 {
    let mut prev = stops[0];
    for s in stops {
        if t <= s.0 {
            return if s == &stops[0] { s.1 } else { prev.1 };
        }
        prev = *s;
    }
    stops[stops.len() - 1].1
}
