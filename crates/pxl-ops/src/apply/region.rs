use pxl_core::{IndexFrame, PxlError, PxlResult};
use pxl_format::Region;

pub(super) fn check_delta(dx: i32, dy: i32) -> PxlResult<()> {
    if !(-32..=32).contains(&dx) || !(-32..=32).contains(&dy) {
        return Err(
            PxlError::schema(format!("offset ({dx},{dy}) is outside -32..=32"))
                .with_path("/dx")
                .with_hint("dx and dy must be in -32..32"),
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
