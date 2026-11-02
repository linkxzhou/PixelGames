//! Animation macros expand to concrete op lists. Each frame's ops are applied
//! to the source frame independently (they do not accumulate).

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult};
use pxl_format::{Op, PartsFile};

mod args;
mod catalog;
mod character;
mod doll;
mod effects;
mod tank;

pub use args::parse_macro_call;
pub use catalog::{macro_catalog, macro_info, MacroArgInfo, MacroInfo, MACRO_NAMES};

use args::check_args;
use character::{blink, bob, idle_breathe, walk_side};
use doll::doll;
use effects::{death_dissolve, hurt_flash, pulse_glow, spin_projectile};
use tank::{idle_rumble, tread_cycle};

pub fn expand_macro(
    spec: &str,
    dir: Option<&str>,
    frame: &IndexFrame,
    palette: &Palette,
    parts: Option<&PartsFile>,
) -> PxlResult<Vec<Vec<Op>>> {
    let (name, args) = parse_macro_call(spec)?;
    if !MACRO_NAMES.contains(&name.as_str()) {
        return Err(PxlError::semantic(format!("unknown macro '{name}'"))
            .with_hint("run `pxl list macros`"));
    }
    check_args(&name, &args)?;
    match name.as_str() {
        "tread_cycle" => tread_cycle(&args, dir, frame, palette),
        "idle_rumble" => idle_rumble(&args, dir),
        "idle_breathe" => idle_breathe(&args, frame),
        "bob" => bob(&args),
        "walk_side" => walk_side(&args, frame, false),
        "run_side" => walk_side(&args, frame, true),
        "hurt_flash" => hurt_flash(&args),
        "death_dissolve" => death_dissolve(&args),
        "blink" => blink(&args, frame, palette),
        "spin_projectile" => spin_projectile(&args),
        "pulse_glow" => pulse_glow(&args),
        "doll" => doll(&args, dir, parts),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests;
