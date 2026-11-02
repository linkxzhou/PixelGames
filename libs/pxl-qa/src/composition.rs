use crate::common::delta;
use crate::{Check, QaConfig, Severity, ValidateInput};
use pxl_core::{IndexFrame, PxlResult};

pub fn compose_grid(frames: &[&IndexFrame], grid: u16) -> PxlResult<IndexFrame> {
    if frames.is_empty() || grid == 0 {
        return Err(pxl_core::PxlError::usage(
            "composition needs at least one frame and a positive grid",
        ));
    }
    let w = frames[0].w;
    let h = frames[0].h;
    if frames.iter().any(|frame| frame.w != w || frame.h != h) {
        return Err(pxl_core::PxlError::semantic(
            "composition frames must have identical dimensions",
        ));
    }
    let mut output = IndexFrame::new(w * grid, h * grid)?;
    for cy in 0..grid {
        for cx in 0..grid {
            let frame = frames[(cx as usize + cy as usize) % frames.len()];
            for y in 0..h {
                for x in 0..w {
                    output.set(
                        (cx * w + x) as i32,
                        (cy * h + y) as i32,
                        frame.get(x as i32, y as i32),
                    );
                }
            }
        }
    }
    Ok(output)
}

pub(crate) fn inspect(input: &ValidateInput, config: &QaConfig) -> Vec<Check> {
    if input.tiles.len() < 2 {
        return Vec::new();
    }
    let frames = input.tiles.iter().map(|(_, frame)| frame).collect::<Vec<_>>();
    let ramp = input.ramp_for(&input.tiles[0].0);
    let mut mismatches = 0usize;
    for (index, frame) in frames.iter().enumerate() {
        let next = frames[(index + 1) % frames.len()];
        for y in 0..frame.h as i32 {
            if !compatible(input, ramp, frame.get(frame.w as i32 - 1, y), next.get(0, y), config) {
                mismatches += 1;
            }
        }
        for x in 0..frame.w as i32 {
            if !compatible(input, ramp, frame.get(x, frame.h as i32 - 1), next.get(x, 0), config) {
                mismatches += 1;
            }
        }
    }
    let border_color = frames[0].get(0, 0);
    let bordered = frames
        .iter()
        .filter(|frame| {
            let w = frame.w as i32;
            let h = frame.h as i32;
            (0..h).all(|y| {
                frame.get(0, y) == border_color && frame.get(w - 1, y) == border_color
            }) && (0..w).all(|x| {
                frame.get(x, 0) == border_color && frame.get(x, h - 1) == border_color
            })
        })
        .count();
    vec![
        Check::new(
            "tile.composition.seams",
            mismatches == 0,
            mismatches,
            "3x3 variant adjacency respects alpha and seam thresholds",
            Severity::Warning,
        ),
        Check::new(
            "tile.composition.periodic_border",
            border_color == 0 || bordered != frames.len(),
            format!("{bordered}/{} uniformly bordered variants", frames.len()),
            "no periodic frame around every tile; declared structural borders are review evidence",
            Severity::Warning,
        ),
    ]
}

fn compatible(
    input: &ValidateInput,
    ramp: &str,
    a: u8,
    b: u8,
    config: &QaConfig,
) -> bool {
    if (a == 0) != (b == 0) {
        return false;
    }
    if a == b {
        return true;
    }
    if let Some(distance) = input.palette.ramp_distance(ramp, a, b) {
        return distance <= config.max_seam_ramp_steps;
    }
    input
        .palette
        .colors
        .get(a as usize)
        .zip(input.palette.colors.get(b as usize))
        .is_some_and(|(a, b)| delta(*a, *b) <= config.max_seam_channel_delta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_core::{Palette, Rgba, Special};

    fn palette() -> Palette {
        Palette {
            id: "test".into(),
            colors: vec![Rgba::TRANSPARENT, Rgba::rgb(20, 20, 20), Rgba::rgb(200, 200, 200)],
            ramps: Default::default(),
            cycles: Default::default(),
            special: Special {
                transparent: 0,
                outline: 1,
                flash: 2,
            },
        }
    }

    #[test]
    fn grid_composition_is_real_pixels_and_flags_periodic_borders() {
        let flat = pxl_core::decode_matrix(&["111".into(), "121".into(), "111".into()]).unwrap();
        let bordered = pxl_core::decode_matrix(&["111".into(), "111".into(), "111".into()]).unwrap();
        let grid = compose_grid(&[&flat, &bordered], 3).unwrap();
        assert_eq!((grid.w, grid.h), (9, 9));
        assert_eq!(grid.get(7, 4), 1);
        let input = ValidateInput {
            palette: palette(),
            frames: Default::default(),
            facings: Default::default(),
            anims: Vec::new(),
            tiles: vec![("flat".into(), flat), ("bordered".into(), bordered)],
            tile_ramp: "main".into(),
            tile_ramps: Default::default(),
            asset_kinds: Default::default(),
            frame_assets: Default::default(),
        };
        let checks = inspect(&input, &QaConfig::default());
        assert!(checks.iter().any(|check| check.id == "tile.composition.periodic_border" && !check.pass));
        assert!(checks.iter().any(|check| check.id == "tile.composition.seams" && check.pass));
    }
}
