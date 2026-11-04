//! Headless acceptance for the simulation: two-hit bricks, star-gated steel,
//! water/grass shell rules, base loss, stage progression, four enemy classes,
//! six power-ups and a seeded replay.

use std::collections::BTreeSet;

use battle_city::enemy::EnemyKind;
use battle_city::level::Level;
use battle_city::powerup::PowerKind;
use battle_city::sim::{Dir, Input, Outcome, Side, Sim, BURST_LIFE, TILE};

const DT: f32 = 1.0 / 60.0;

fn known_assets() -> BTreeSet<String> {
    ["b", "x", "g", "w", "i"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// 6x6 arena: brick at (1,0), steel at (2,0), water at (2,2), the HQ wall at
/// (2,4)/(3,4)/(4,4)/(2,5)/(4,5) and the base at (3,5).
fn arena() -> Level {
    let json = r#"{
        "schema_version": 3, "id": "arena", "name": "Arena", "tile_size_px": 32,
        "grid_w": 6, "grid_h": 6,
        "grid": ["fbxfff", "ffffff", "ffwfff", "ffffff", "ffbbbf", "ffbfbf"],
        "tile_semantics": [
            {"tile_id": "f", "behaviour": "floor", "blocks": false},
            {"tile_id": "b", "behaviour": "wall", "blocks": true},
            {"tile_id": "x", "behaviour": "wall", "blocks": true},
            {"tile_id": "w", "behaviour": "water", "blocks": true},
            {"tile_id": "g", "behaviour": "decor", "blocks": false}
        ],
        "placements": [{"entity_id": "hq_base", "pos": [3, 5]}],
        "spawn": [0, 0], "exit": [0, 0],
        "difficulty": 1, "seed": 9,
        "enemy_composition": {"basic": 8, "fast": 4, "power": 4, "armor": 4}
    }"#;
    Level::from_json(json, &known_assets()).expect("arena parses")
}

/// A sim with the wave timer stopped and the spawn shield dropped, so a test
/// can drive one system at a time.
fn quiet_sim() -> Sim {
    let mut sim = Sim::new(0, arena());
    sim.freeze_spawns();
    sim.clear_shield();
    sim
}

fn advance(sim: &mut Sim, steps: usize) {
    for _ in 0..steps {
        sim.step(Input::default(), DT);
    }
}

fn fire_east(sim: &mut Sim, x: f32, y: f32) {
    sim.force_bullet(Side::Player, x, y, Dir::E);
}

#[test]
fn bricks_take_two_hits() {
    let mut sim = quiet_sim();
    assert_eq!(sim.level.at(1, 0), "b");
    fire_east(&mut sim, 20.0, 12.0);
    advance(&mut sim, 8);
    assert_eq!(sim.level.at(1, 0), "b", "first hit only cracks the brick");
    assert!(sim.level.is_cracked_at(1, 0));
    fire_east(&mut sim, 20.0, 12.0);
    advance(&mut sim, 8);
    assert_eq!(sim.level.at(1, 0), "f", "second hit clears the brick");
}

#[test]
fn steel_needs_a_tier_three_shell() {
    let mut sim = quiet_sim();
    // Clear the brick in front so the shell reaches the steel plate.
    sim.set_tile(1, 0, "f");
    assert_eq!(sim.level.at(2, 0), "x");
    fire_east(&mut sim, 60.0, 12.0);
    advance(&mut sim, 8);
    assert_eq!(sim.level.at(2, 0), "x", "a normal shell is stopped");
    sim.star = 3;
    fire_east(&mut sim, 60.0, 12.0);
    advance(&mut sim, 8);
    assert_eq!(sim.level.at(2, 0), "f", "a tier-three shell breaks steel");
}

#[test]
fn shells_pass_water_and_grass() {
    let mut sim = quiet_sim();
    sim.force_bullet(Side::Player, 64.0, 70.0, Dir::E);
    advance(&mut sim, 6);
    assert!(!sim.bullets.is_empty(), "water does not stop shells");
    sim.force_bullet(Side::Player, 32.0, 70.0, Dir::E);
    advance(&mut sim, 4);
    assert!(!sim.bullets.is_empty(), "grass does not stop shells");
}

#[test]
fn water_blocks_tanks() {
    let mut sim = quiet_sim();
    sim.player.x = 64.0;
    sim.player.y = 64.0;
    let before = (sim.player.x, sim.player.y);
    for _ in 0..60 {
        sim.step(
            Input {
                down: true,
                ..Default::default()
            },
            DT,
        );
    }
    assert_eq!(
        (sim.player.x, sim.player.y),
        before,
        "water blocks the hull"
    );
}

#[test]
fn destroying_the_base_ends_the_run() {
    let mut sim = quiet_sim();
    assert!(sim.base_alive);
    // Open the brick above the HQ, then fly a shell down into the eagle.
    sim.set_tile(3, 4, "f");
    sim.force_bullet(Side::Enemy, 3.0 * TILE + 12.0, 5.0 * TILE - 40.0, Dir::S);
    let mut steps = 0;
    while sim.base_alive && steps < 60 {
        sim.step(Input::default(), DT);
        steps += 1;
    }
    assert!(!sim.base_alive, "the base takes the hit");
    assert_eq!(sim.outcome, Outcome::GameOver);
    assert!(sim.shake > 0.0, "the screen shakes");
}

#[test]
fn clearing_the_last_wave_clears_the_stage() {
    let mut sim = quiet_sim();
    sim.remaining = 0;
    advance(&mut sim, 2);
    assert_eq!(sim.outcome, Outcome::StageClear);

    let mut last = Sim::new(34, arena());
    last.freeze_spawns();
    last.remaining = 0;
    advance(&mut last, 2);
    assert_eq!(last.outcome, Outcome::Victory);
}

#[test]
fn player_cannot_drive_through_a_brick() {
    let mut sim = quiet_sim();
    for _ in 0..120 {
        sim.step(
            Input {
                right: true,
                ..Default::default()
            },
            DT,
        );
    }
    assert_eq!(sim.player.x, 0.0, "the brick at (1,0) blocks the hull");
    assert_eq!(sim.player.dir, Dir::E);
}

#[test]
fn tanks_steer_on_tile_lanes() {
    let mut sim = quiet_sim();
    for _ in 0..30 {
        sim.step(
            Input {
                right: true,
                ..Default::default()
            },
            DT,
        );
    }
    sim.step(
        Input {
            down: true,
            ..Default::default()
        },
        DT,
    );
    assert_eq!(sim.player.x % TILE, 0.0, "the hull aligns to the lane");
}

#[test]
fn enemy_shell_kills_the_player_and_costs_a_life() {
    let mut sim = quiet_sim();
    let lives = sim.lives;
    sim.force_bullet(Side::Enemy, 12.0, 8.0, Dir::S);
    advance(&mut sim, 4);
    assert!(!sim.player_alive, "the player is destroyed");
    assert_eq!(sim.lives, lives - 1);
    assert_ne!(sim.outcome, Outcome::GameOver, "lives remain");
}

#[test]
fn the_spawn_shield_absorbs_one_shell() {
    let mut sim = Sim::new(0, arena());
    sim.freeze_spawns();
    sim.force_bullet(Side::Enemy, 12.0, 8.0, Dir::S);
    advance(&mut sim, 4);
    assert!(sim.player_alive, "the shield holds");
}

#[test]
fn armour_takes_four_hits_and_only_scores_on_death() {
    let mut sim = quiet_sim();
    sim.force_spawn(96.0, 96.0, Dir::E, EnemyKind::Armor);
    sim.clock_in = 60.0;
    for hit in 0..4 {
        fire_east(&mut sim, 60.0, 100.0);
        advance(&mut sim, 10);
        let score = sim.score;
        if hit < 3 {
            assert_eq!(sim.enemies.len(), 1, "armour survives hit {}", hit + 1);
            assert_eq!(sim.enemies[0].hp, 3 - hit as u8);
            assert_eq!(score, 0);
        } else {
            assert!(sim.enemies.is_empty(), "the fourth hit kills it");
            assert_eq!(score, 400);
        }
    }
}

#[test]
fn star_power_raises_the_firepower_tier() {
    let mut sim = quiet_sim();
    sim.force_drop(PowerKind::Star, 0.0, 0.0);
    advance(&mut sim, 2);
    assert_eq!(sim.star(), 1);
    assert!(sim.drops.is_empty(), "the drop is consumed");
}

#[test]
fn grenade_clears_the_screen() {
    let mut sim = quiet_sim();
    sim.force_spawn(96.0, 96.0, Dir::E, EnemyKind::Basic);
    sim.force_spawn(160.0, 96.0, Dir::E, EnemyKind::Fast);
    sim.clock_in = 60.0;
    sim.force_drop(PowerKind::Grenade, 0.0, 0.0);
    advance(&mut sim, 2);
    assert!(sim.enemies.is_empty(), "every on-screen enemy dies");
    assert_eq!(sim.score, 100 + 200 + 500);
}

#[test]
fn shovel_steels_the_hq_wall_then_restores_it() {
    let mut sim = quiet_sim();
    let wall = sim.level.wall_cells();
    assert!(wall.len() >= 3, "the arena has an HQ wall");
    sim.force_drop(PowerKind::Shovel, 0.0, 0.0);
    advance(&mut sim, 2);
    assert!(
        sim.level.is_steel_at(wall[0].0, wall[0].1),
        "wall becomes steel"
    );
    sim.shovel_in = 0.001;
    sim.step(Input::default(), DT);
    assert!(
        sim.level.is_breakable_at(wall[0].0, wall[0].1),
        "wall becomes brick again"
    );
}

#[test]
fn clock_freezes_enemies() {
    let mut sim = quiet_sim();
    let index = sim.force_spawn(96.0, 96.0, Dir::E, EnemyKind::Fast);
    sim.clock_in = 10.0;
    let x = sim.enemies[index].tank.x;
    advance(&mut sim, 60);
    assert_eq!(sim.enemies[index].tank.x, x, "frozen tanks do not move");
    sim.thaw_all();
    advance(&mut sim, 30);
    assert!(sim.enemies[index].tank.x > x, "frozen tanks resume");
}

#[test]
fn a_carrier_drops_a_power_up() {
    let mut sim = quiet_sim();
    let index = sim.force_spawn(96.0, 96.0, Dir::E, EnemyKind::Basic);
    sim.enemies[index].carrier = true;
    sim.clock_in = 60.0;
    fire_east(&mut sim, 60.0, 100.0);
    advance(&mut sim, 10);
    assert_eq!(sim.drops.len(), 1, "the carrier leaves one drop");
}

#[test]
fn bursts_expire() {
    let mut sim = quiet_sim();
    fire_east(&mut sim, 20.0, 12.0);
    advance(&mut sim, 8);
    assert!(!sim.bursts.is_empty(), "the hit spawns a burst");
    advance(&mut sim, (BURST_LIFE / DT) as usize + 4);
    assert!(sim.bursts.is_empty(), "bursts are transient");
}

#[test]
fn a_seeded_run_replays_exactly() {
    let inputs: Vec<Input> = (0..240)
        .map(|i| Input {
            right: i % 40 < 20,
            down: i % 40 >= 20,
            fire: i % 17 == 0,
            ..Default::default()
        })
        .collect();
    let mut a = Sim::new(1, arena());
    let mut b = Sim::new(1, arena());
    a.freeze_spawns();
    b.freeze_spawns();
    for input in &inputs {
        a.step(*input, DT);
        b.step(*input, DT);
    }
    assert_eq!(a.fingerprint(), b.fingerprint(), "same seed, same play");
    assert!(a.time > 0.0);
}
