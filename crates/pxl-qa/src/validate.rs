use pxl_core::{PxlError, PxlResult};

use crate::animation::{anim_diff, effect_lifecycle, scoped};
use crate::character::character_checks;
use crate::common::{fail, frame_checks, is_terminal_empty, palette_checks, visible_check};
use crate::model::{AnimSeq, Report, ValidateInput};
use crate::tank::{hull_stability, tank_counts, tank_facing_checks, tread_motion};
use crate::tile::{tile_checks, tile_coverage, tile_seams};

pub fn validate(input: &ValidateInput, rules: &str) -> PxlResult<Report> {
    let mut checks = Vec::new();
    match rules {
        "default" => {
            checks.extend(palette_checks(&input.palette));
            for (name, frame) in &input.frames {
                // Full-bleed tiles are listed on the sheet. Character bbox limits
                // do not apply to them; palette indices still do.
                // A fully clear final frame of a one-shot animation (death,
                // despawn) is a valid end state, not a degenerate sprite.
                let terminal = is_terminal_empty(input, name, frame);
                let is_tile = input.tiles.iter().any(|(tile, _)| tile == name);
                // Effects, UI and overlays are judged by their declared use:
                // sparse glyphs, scattered particles and a marker on a clear
                // tile are not sprites with a broken silhouette. They still
                // must be visible unless they are a declared end state.
                let sprite_like = matches!(input.kind_of_frame(name), "sprite" | "parts");
                let structural = sprite_like && !is_tile && !terminal;
                checks.extend(frame_checks(&input.palette, name, frame, structural));
                if !sprite_like && !is_tile && !terminal {
                    checks.push(visible_check(name, frame));
                }
            }
            for anim in &input.anims {
                if input.kind_of_asset(&anim.asset) == "effect" {
                    // Bursts change most of the canvas between frames by design;
                    // the motion cap does not apply, the lifecycle rules do.
                    checks.extend(anim_diff(anim, 100, 10_000, true));
                    checks.extend(effect_lifecycle(anim));
                } else {
                    checks.extend(anim_diff(anim, 100, 3500, true));
                }
            }
            // A tile that is never expected to tile would slip through: match its
            // opposite edges on the declared ramp the same way `tank` does.
            if !input.tiles.is_empty() {
                checks.extend(tile_coverage(input));
                checks.extend(tile_seams(input));
            }
        }
        "tank" => {
            checks.extend(palette_checks(&input.palette));
            for (name, frame) in &input.frames {
                checks.extend(frame_checks(&input.palette, name, frame, false));
            }
            checks.extend(tank_facing_checks(&input.facings));
            // Every tank in the sheet is judged. The animation is found by its
            // own name, so a second tank's `tank2_move` is not skipped; its
            // check ids carry the display key to tell the tanks apart.
            let moves: Vec<&AnimSeq> = input
                .anims
                .iter()
                .filter(|a| a.base_name == "move")
                .collect();
            for anim in &moves {
                checks.extend(scoped(anim, anim_diff(anim, 200, 2500, true)));
                checks.extend(scoped(anim, tread_motion(anim, &input.palette)));
                checks.extend(scoped(anim, hull_stability(anim, &input.palette)));
            }
            if moves.is_empty() {
                checks.push(fail(
                    "tank.move_present",
                    "missing",
                    "move anim with >= 4 frames per dir",
                ));
            }
            checks.extend(tank_counts(input));
            checks.extend(tile_checks(input));
        }
        "character" => {
            checks.extend(palette_checks(&input.palette));
            for (name, frame) in &input.frames {
                checks.extend(frame_checks(&input.palette, name, frame, false));
            }
            checks.extend(character_checks(input));
        }
        other => {
            return Err(PxlError::usage(format!("unknown ruleset '{other}'"))
                .with_hint("rules are default, tank, or character"));
        }
    }
    Ok(Report {
        rules: rules.to_string(),
        checks,
    })
}
