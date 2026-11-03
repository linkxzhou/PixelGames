//! Per-frame rasterisation and detail protection.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use pxl_art::Material;
use pxl_core::{IndexFrame, PxlResult, SizeProfile};

use crate::raster::{self, Canvas, FillRule};

use super::ctx::{Ctx, Shape};

pub(crate) fn centroid_pixel(s: &Shape, w: usize, h: usize) -> Option<(usize, usize)> {
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

type Rendered = (
    IndexFrame,
    Vec<bool>,
    Option<Vec<Material>>,
    Vec<String>,
    u32,
);

pub(crate) fn render_frame(
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
