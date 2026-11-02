use pxl_core::{IndexFrame, PxlError, PxlResult};
use pxl_format::Region;

/// A shift may move at most one full 128 wide canvas: enough to slide art off
/// the edge, small enough that a typo cannot ask for absurd work.
pub(super) fn check_delta(dx: i32, dy: i32) -> PxlResult<()> {
    let max = pxl_core::MAX_DIM as i32;
    if !(-max..=max).contains(&dx) || !(-max..=max).contains(&dy) {
        return Err(
            PxlError::schema(format!("offset ({dx},{dy}) is outside -{max}..={max}"))
                .with_path("/dx")
                .with_hint(format!(
                    "dx and dy must be in -{max}..{max}; larger moves are a mistake"
                )),
        );
    }
    Ok(())
}

pub(super) fn full_rect(frame: &IndexFrame) -> [i32; 4] {
    [0, 0, frame.w as i32, frame.h as i32]
}

pub(super) fn resolve_region(
    region: &Option<Region>,
    frame: &IndexFrame,
) -> PxlResult<Option<[i32; 4]>> {
    let Some(region) = region else {
        return Ok(None);
    };
    match region.to_rect() {
        Some(r) => {
            if r[2] <= 0 || r[3] <= 0 {
                return Err(PxlError::schema("region width and height must be positive")
                    .with_path("/region"));
            }
            if r[0] < -(frame.w as i32)
                || r[1] < -(frame.h as i32)
                || r[0] > frame.w as i32
                || r[1] > frame.h as i32
            {
                return Err(PxlError::semantic("region is outside the frame").with_path("/region"));
            }
            Ok(Some(r))
        }
        None => Err(PxlError::semantic(format!(
            "named region '{}' is unresolved",
            region.name().unwrap_or("?")
        ))
        .with_hint("pass [x,y,w,h]. auto_tread is resolved inside anim macros")),
    }
}
