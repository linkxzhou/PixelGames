//! Headless smoke test for a built asset package.
//!
//! ```bash
//! cargo run -p battle-city-runtime --example package_check -- ../work/400-package
//! ```
//!
//! Loads the package exactly like the game does, then asserts that every
//! asset animation resolves to frames, every tile a level names is delivered,
//! and the spawn/base cells are usable. No window is opened.

use std::collections::BTreeSet;
use std::path::PathBuf;

use battle_city::assets::load_package;
use battle_city::sim::{Dir, Sim};

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "../work/400-package".to_string());
    let package = match load_package(&PathBuf::from(&dir)) {
        Ok(package) => package,
        Err(err) => {
            eprintln!("FAIL load: {err}");
            std::process::exit(1);
        }
    };
    println!(
        "package {} | {} sheets | {} assets | {} levels | palette {} ({} colours)",
        package.package_id,
        package.sheets.len(),
        package.assets.len(),
        package.levels.len(),
        package.palette.id,
        package.palette.len()
    );
    let mut failures = 0;

    for sheet in package.sheets.values() {
        println!(
            "  sheet {:<16} {}x{} {:>3} frames {:>3} tags",
            sheet.id,
            sheet.w,
            sheet.h,
            sheet.frames.len(),
            sheet.tags.len()
        );
        if sheet.frames.is_empty() {
            eprintln!("FAIL sheet {} has no frames", sheet.id);
            failures += 1;
        }
        if sheet.rgba.len() != (sheet.w * sheet.h * 4) as usize {
            eprintln!("FAIL sheet {} pixel buffer is the wrong size", sheet.id);
            failures += 1;
        }
    }

    for asset in &package.assets {
        let Some(sheet) = package.sheets.get(&asset.sheet) else {
            eprintln!(
                "FAIL asset {} names missing sheet {}",
                asset.id, asset.sheet
            );
            failures += 1;
            continue;
        };
        for (anim, info) in &asset.anims {
            let four_way =
                asset.id == "player_tank" || asset.id == "bullet" || asset.id.starts_with("enemy_");
            let dirs: Vec<&str> = if four_way {
                Dir::ALL.iter().map(|d| d.name()).collect()
            } else {
                vec!["N"]
            };
            for dir in dirs {
                let frames = sheet.anim_frames(&asset.id, anim, dir);
                if frames.is_empty() {
                    eprintln!("FAIL {} anim {anim}_{dir} has no frames", asset.id);
                    failures += 1;
                } else if frames.len() != info.frames as usize {
                    eprintln!(
                        "FAIL {} anim {anim}_{dir}: {} frames, requirement says {}",
                        asset.id,
                        frames.len(),
                        info.frames
                    );
                    failures += 1;
                }
                for frame in frames {
                    if frame.w == 0 || frame.h == 0 {
                        eprintln!("FAIL {} anim {anim}_{dir} has an empty frame", asset.id);
                        failures += 1;
                    }
                }
            }
        }
    }

    let delivered: BTreeSet<String> = package.assets.iter().map(|a| a.id.clone()).collect();
    for level in &package.levels {
        println!(
            "  level {:<8} {}x{} {} tiles  difficulty {}",
            level.id,
            level.w,
            level.h,
            level.w * level.h,
            level.difficulty
        );
        for cy in 0..level.h {
            for cx in 0..level.w {
                let tile = level.at(cx, cy);
                let is_floor = level
                    .defs
                    .get(tile)
                    .map(|d| d.behaviour == "floor")
                    .unwrap_or(false);
                if !is_floor && !delivered.contains(tile) {
                    eprintln!("FAIL {}: tile '{tile}' missing at ({cx},{cy})", level.id);
                    failures += 1;
                }
            }
        }
        let total: u32 = level.composition.iter().sum();
        if total != 20 {
            eprintln!(
                "FAIL {}: enemy composition sums to {total}, not 20",
                level.id
            );
            failures += 1;
        }
        if level.enemy_spawns.len() != 3 {
            eprintln!(
                "FAIL {}: {} enemy spawn mouths",
                level.id,
                level.enemy_spawns.len()
            );
            failures += 1;
        }
        // The simulation must be constructible and advance without panicking.
        let mut sim = Sim::new(level.difficulty.saturating_sub(1) as usize, level.clone());
        for _ in 0..180 {
            sim.step(battle_city::sim::Input::default(), 1.0 / 60.0);
        }
        println!(
            "  level {:<8} 3s headless: outcome {:?} score {} remaining {} enemy {} bullet {}",
            level.id,
            sim.outcome,
            sim.score,
            sim.remaining,
            sim.enemies.len(),
            sim.bullets.len()
        );
    }

    if failures > 0 {
        eprintln!("FAIL {failures} problem(s)");
        std::process::exit(1);
    }
    println!("OK package {} loads and plays headless", package.package_id);
}
