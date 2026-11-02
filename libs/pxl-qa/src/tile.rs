use crate::common::{delta, sample, style};
use crate::{Check, QaConfig, Severity, ValidateInput};

pub(crate) fn inspect(input: &ValidateInput, config: &QaConfig) -> Vec<Check> {
    let mut out = Vec::new();
    for (name, frame) in &input.tiles {
        let mut full = Check::new(
            format!("tile.{name}.full_bleed"),
            true,
            0,
            "0 clear border pixels",
            Severity::Error,
        );
        let mut clear = 0;
        for y in 0..frame.h as i32 {
            for x in 0..frame.w as i32 {
                if (x == 0 || y == 0 || x == frame.w as i32 - 1 || y == frame.h as i32 - 1)
                    && frame.get(x, y) == 0
                {
                    clear += 1;
                    sample(&mut full, x, y, config);
                }
            }
        }
        full.pass = clear == 0;
        full.value = format!("{clear} clear edge pixels");
        if config.require_tile_full_bleed {
            out.push(full);
        }
        for (axis, enabled, length) in [
            ("x", config.tile_horizontal, frame.h),
            ("y", config.tile_vertical, frame.w),
        ] {
            if !enabled {
                continue;
            }
            let mut seam = Check::new(
                format!("tile.{name}.seam_{axis}"),
                true,
                0,
                format!(
                    "alpha match; ramp distance <= {}, otherwise RGBA delta <= {}",
                    config.max_seam_ramp_steps, config.max_seam_channel_delta
                ),
                style(config),
            );
            let mut bad = 0;
            for i in 0..length as i32 {
                let (x, y, xx, yy) = if axis == "x" {
                    (0, i, frame.w as i32 - 1, i)
                } else {
                    (i, 0, i, frame.h as i32 - 1)
                };
                let (a, b) = (frame.get(x, y), frame.get(xx, yy));
                let matches = if (a == 0) != (b == 0) {
                    false
                } else if a == b {
                    true
                } else if let Some(d) = input.palette.ramp_distance(input.ramp_for(name), a, b) {
                    d <= config.max_seam_ramp_steps
                } else {
                    input
                        .palette
                        .colors
                        .get(a as usize)
                        .zip(input.palette.colors.get(b as usize))
                        .is_some_and(|(a, b)| delta(*a, *b) <= config.max_seam_channel_delta)
                };
                if !matches {
                    bad += 1;
                    sample(&mut seam, x, y, config);
                }
            }
            seam.pass = bad == 0;
            seam.value = format!("{bad}/{length} mismatched edge pairs");
            out.push(seam);
        }
    }
    out
}
