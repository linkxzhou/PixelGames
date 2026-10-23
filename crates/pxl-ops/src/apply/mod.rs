use std::cell::RefCell;
use std::collections::BTreeMap;

use pxl_core::{
    op_seed, paint_outline, Brush, IndexFrame, OutlineMode, Palette, PxlError, PxlResult,
};
use pxl_format::{Op, PartsFile};
use pxl_gen::RuntimeMask;

use crate::parts::{self, PartPose};

pub struct ApplyCx<'a> {
    pub palette: &'a Palette,
    pub seed: u64,
    pub frame_name: &'a str,
    pub load_template: Option<&'a dyn Fn(&str) -> PxlResult<RuntimeMask>>,
    /// Paper-doll rig used by `part_transform` and `compose`.
    pub parts: Option<&'a PartsFile>,
    /// Pose accumulated by `part_transform` inside one `apply_ops` call.
    pub pose: RefCell<BTreeMap<String, PartPose>>,
}

pub fn apply_ops(frame: &IndexFrame, ops: &[Op], cx: &ApplyCx) -> PxlResult<IndexFrame> {
    cx.pose.borrow_mut().clear();
    let mut cur = frame.clone();
    for (i, op) in ops.iter().enumerate() {
        cur = apply_op(&cur, op, cx, i as u64)?;
    }
    Ok(cur)
}

pub fn apply_op(frame: &IndexFrame, op: &Op, cx: &ApplyCx, index: u64) -> PxlResult<IndexFrame> {
    match op {
        Op::Noop {} => Ok(frame.clone()),
        Op::Shift {
            dx,
            dy,
            wrap,
            region,
        } => {
            check_delta(*dx, *dy)?;
            let rect = resolve_region(region, frame)?;
            Ok(translate(frame, *dx, *dy, *wrap, rect))
        }
        Op::RowShift {
            rows,
            dx,
            dy,
            pattern,
            phase,
            region,
            axis,
            wrap,
        } => {
            check_delta(*dx, *dy)?;
            if let Some(rect) = resolve_region(region, frame)? {
                // Region mode scrolls a rectangle along one axis. The per-row
                // parameters belong to the other mode and would be ignored.
                if rows.is_some() || pattern.is_some() || *phase != 0 {
                    return Err(PxlError::schema(
                        "row_shift with a region does not take rows, pattern or phase",
                    )
                    .with_hint("drop the region for per-row shifts, or drop rows/pattern/phase to scroll the region"));
                }
                let axis = axis.as_deref().unwrap_or("x");
                let amount = if axis == "y" || axis == "vertical" {
                    if *dy != 0 {
                        *dy
                    } else {
                        *dx
                    }
                } else {
                    *dx
                };
                let do_wrap = wrap.unwrap_or(true);
                return scroll_rect(frame, rect, axis, amount, do_wrap);
            }
            // Per-row mode shifts each row sideways by dx. A vertical amount or
            // an axis only makes sense when scrolling a region.
            if axis.is_some() || *dy != 0 {
                return Err(PxlError::schema(
                    "row_shift without a region does not take axis or dy",
                )
                .with_hint(
                    "add a region to scroll along an axis, or use shift for a vertical move",
                ));
            }
            let [y0, y1] = rows.unwrap_or([0, frame.h as i32 - 1]);
            let do_wrap = wrap.unwrap_or(false);
            Ok(shift_rows(
                frame,
                y0,
                y1,
                *dx,
                pattern.as_deref(),
                *phase,
                do_wrap,
            )?)
        }
        Op::Shear {
            axis,
            k,
            pivot,
            region,
        } => {
            if *k == 0 {
                return Err(PxlError::schema("shear k must be non-zero").with_path("/k"));
            }
            if !matches!(axis.as_str(), "x" | "horizontal" | "y" | "vertical") {
                return Err(PxlError::schema(format!("unknown shear axis '{axis}'"))
                    .with_path("/axis")
                    .with_hint("axis is x (or horizontal) or y (or vertical)"));
            }
            let rect = resolve_region(region, frame)?.unwrap_or(full_rect(frame));
            Ok(shear(frame, axis, *k, *pivot, rect))
        }
        Op::Squash {
            sx,
            sy,
            anchor,
            region,
        } => {
            check_delta(*sx, *sy)?;
            let rect = resolve_region(region, frame)?.unwrap_or(full_rect(frame));
            squash(frame, *sx, *sy, anchor, rect)
        }
        Op::Flip { axis } => Ok(flip(frame, axis)?),
        Op::Rotate90 { dir } => rotate90(frame, dir),
        Op::Rotate { .. } => Err(PxlError::semantic(
            "arbitrary-angle rotate is not implemented yet",
        )
        .with_hint("use rotate90 (cw, ccw, 180). cleanEdge angles arrive in M2")),
        Op::PaletteSwap { map, region } => {
            let rect = resolve_region(region, frame)?;
            palette_swap(frame, map, cx.palette, rect)
        }
        Op::PaletteCycle {
            cycle,
            step,
            region,
        } => {
            let rect = resolve_region(region, frame)?;
            let map = cx.palette.cycle_map(cycle, *step)?;
            Ok(remap(frame, &map, rect))
        }
        Op::Shade {
            dir,
            amount,
            region,
            ramp,
        } => {
            if !(-4..=4).contains(amount) {
                return Err(PxlError::schema("shade amount must be in -4..=4"));
            }
            if let Some(name) = ramp {
                if !cx.palette.ramps.contains_key(name) {
                    return Err(
                        PxlError::semantic(format!("shade ramp '{name}' is missing"))
                            .with_path("/ramp")
                            .with_hint("name a ramp declared on the palette"),
                    );
                }
            }
            let rect = resolve_region(region, frame)?;
            Ok(shade(
                frame,
                dir,
                *amount,
                cx.palette,
                rect,
                ramp.as_deref(),
            )?)
        }
        Op::Outline {
            color,
            mode,
            brush,
            width,
        } => {
            let mut out = frame.clone();
            let c = color_of(cx.palette, color)?;
            paint_outline(
                &mut out,
                c,
                OutlineMode::parse(mode)?,
                Brush::parse(brush)?,
                *width,
            )?;
            Ok(out)
        }
        Op::Dissolve {
            seed,
            steps,
            step,
            pattern,
        } => {
            // The frame name is left out on purpose. Each animation frame runs
            // from the same source, so every frame must draw the same pixel
            // ranks; otherwise a pixel that vanished could come back.
            let sub = op_seed(cx.seed ^ seed, index, "");
            dissolve(frame, sub, *steps, *step, pattern)
        }
        Op::MaskErase { region, mask, x, y } => Ok(mask_edit(frame, region, mask, *x, *y, None)?),
        Op::MaskFill {
            color,
            region,
            mask,
            x,
            y,
        } => {
            let c = color_of(cx.palette, color)?;
            Ok(mask_edit(frame, region, mask, *x, *y, Some(c))?)
        }
        Op::PatchPixels { x, y, rows } => patch(frame, *x, *y, rows),
        Op::Texture {
            gen,
            template,
            region,
            ramp,
            seed,
        } => {
            let rect = resolve_region(&Some(region.clone()), frame)?.ok_or_else(|| {
                PxlError::semantic("texture region must be a rectangle").with_hint("use [x,y,w,h]")
            })?;
            let sub = seed.unwrap_or_else(|| op_seed(cx.seed, index, cx.frame_name));
            texture(frame, gen, template, rect, ramp.as_deref(), sub, cx)
        }
        Op::PartTransform {
            part,
            dx,
            dy,
            rot90,
            flip,
        } => {
            check_delta(*dx, *dy)?;
            let rig = cx.parts.ok_or_else(|| {
                PxlError::semantic("part_transform needs a parts spec")
                    .with_hint("use base.gen parts, or set meta.parts to a .parts.json file")
            })?;
            rig.part(part)?;
            let (flip_x, flip_y) = parts::parse_flip(flip.as_deref())?;
            cx.pose.borrow_mut().insert(
                part.clone(),
                PartPose {
                    dx: *dx,
                    dy: *dy,
                    rot: parts::parse_rot(rot90.as_deref())?,
                    flip_x,
                    flip_y,
                },
            );
            Ok(frame.clone())
        }
        Op::Compose { parts: order, dir } => {
            let rig = cx.parts.ok_or_else(|| {
                PxlError::semantic("compose needs a parts spec")
                    .with_hint("use base.gen parts, or set meta.parts to a .parts.json file")
            })?;
            let facing = dir.as_deref().unwrap_or("down");
            let pose = cx.pose.borrow().clone();
            let drawn = parts::compose_rig(frame, rig, facing, order, &pose)?;
            cx.pose.borrow_mut().clear();
            Ok(drawn)
        }
    }
}

mod color;
mod pixels;
mod region;
mod transform;

use color::{color_of, palette_swap, remap, shade};
use pixels::{dissolve, mask_edit, patch, texture};
use region::{check_delta, full_rect, resolve_region};
pub(crate) use transform::rotate90;
use transform::{flip, scroll_rect, shear, shift_rows, squash, translate};

#[cfg(test)]
mod tests;
