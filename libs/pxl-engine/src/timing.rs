use pxl_core::{PxlError, PxlResult};

/// Limits every animation obeys, whether it comes from `pxl anim`, a recipe, or
/// the web page, so no entry point can describe motion another one refuses to
/// preview.
pub const MAX_ANIM_FPS: u32 = 60;

pub const MAX_ANIM_FRAMES: usize = 16;

pub fn check_anim_timing(name: &str, fps: u32, frame_count: usize) -> PxlResult<()> {
    if fps == 0 || fps > MAX_ANIM_FPS {
        return Err(PxlError::schema(format!(
            "animation '{name}' has fps {fps}; fps must be in 1..={MAX_ANIM_FPS}"
        ))
        .with_path(format!("/anims/{name}/fps")));
    }
    if frame_count == 0 || frame_count > MAX_ANIM_FRAMES {
        return Err(PxlError::schema(format!(
            "animation '{name}' has {frame_count} frames; expected 1..={MAX_ANIM_FRAMES}"
        ))
        .with_path(format!("/anims/{name}/frames")));
    }
    Ok(())
}
