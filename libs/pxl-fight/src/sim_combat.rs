//! A compact, deterministic 2D versus fighter.
//!
//! Two fighters stand on one arena floor. Attacks are frame-windowed (startup,
//! active, recovery), blocking chips damage, throws ignore block and a motion
//! command fires a projectile. Rounds are best of three and a ladder of
//! opponents advances the match.

use pxl_arcade_kit::input::{Dir4, InputState};
use pxl_arcade_kit::replay::StateHash;
use pxl_arcade_kit::rng::Rng;


use super::types::*;

impl FightSim {
    pub(crate) fn update_player(&mut self, input: InputState, dt_ms: u32) {
        let opponent_x = self.cpu.x;
        let speed = self.player_def.walk_speed;
        let mut fighter = self.player;
        if fighter.stun_ms > 0 {
            fighter.stun_ms = fighter.stun_ms.saturating_sub(dt_ms);
            fighter.pose = Pose::Hit;
            fighter.pose_ms = 0;
            self.player = fighter;
            self.apply_gravity(dt_ms);
            return;
        }
        if let Some((kind, elapsed)) = fighter.attack {
            let def = move_def(kind);
            let total = def.startup_ms + def.active_ms + def.recovery_ms;
            let next = elapsed + dt_ms;
            if def.projectile && elapsed < def.startup_ms && next >= def.startup_ms {
                self.spawn_projectile(0);
            }
            if next >= total {
                fighter.attack = None;
                fighter.hit_done = false;
                fighter.pose = Pose::Idle;
            } else {
                fighter.attack = Some((kind, next));
                fighter.pose = match kind {
                    MoveKind::Punch => Pose::Punch,
                    MoveKind::Kick => Pose::Kick,
                    MoveKind::Special => Pose::Special,
                    MoveKind::Throw => Pose::Punch,
                };
            }
            self.player = fighter;
            self.apply_gravity(dt_ms);
            return;
        }

        fighter.facing = if opponent_x >= fighter.x { 1 } else { -1 };
        let blocking = self.can_block(&self.player, &self.cpu, input);
        if blocking {
            fighter.pose = Pose::Block;
            self.player = fighter;
            self.apply_gravity(dt_ms);
            return;
        }

        if input.primary_pressed() {
            let close = (opponent_x - fighter.x).abs() < 24.0;
            let forward = (opponent_x - fighter.x).signum() as i32 == fighter.facing;
            let kind = if close && forward {
                MoveKind::Throw
            } else if self.wants_special() {
                MoveKind::Special
            } else {
                MoveKind::Punch
            };
            fighter.attack = Some((kind, 0));
            fighter.hit_done = false;
            fighter.pose = match kind {
                MoveKind::Kick => Pose::Kick,
                MoveKind::Special => Pose::Special,
                _ => Pose::Punch,
            };
            self.player = fighter;
            self.apply_gravity(dt_ms);
            return;
        }
        if input.secondary_pressed() {
            fighter.attack = Some((MoveKind::Kick, 0));
            fighter.hit_done = false;
            fighter.pose = Pose::Kick;
            self.player = fighter;
            self.apply_gravity(dt_ms);
            return;
        }

        fighter.crouch = input.held.down;
        if fighter.on_ground && input.pressed.up {
            fighter.vy = -170.0;
            fighter.on_ground = false;
            fighter.pose = Pose::Jump;
        }
        let mut moving = false;
        if !fighter.crouch {
            if input.held.left {
                fighter.x -= speed * dt_ms as f32 / 1000.0;
                moving = true;
            }
            if input.held.right {
                fighter.x += speed * dt_ms as f32 / 1000.0;
                moving = true;
            }
        }
        fighter.x = fighter.x.clamp(STAGE_LEFT, STAGE_RIGHT);
        if fighter.on_ground {
            fighter.pose = if fighter.crouch {
                Pose::Crouch
            } else if moving {
                Pose::Walk
            } else {
                Pose::Idle
            };
        }
        self.player = fighter;
        self.apply_gravity(dt_ms);
    }

    pub(crate) fn update_cpu(&mut self, dt_ms: u32) {
        let mut fighter = self.cpu;
        let target = self.player.x;
        if fighter.stun_ms > 0 {
            fighter.stun_ms = fighter.stun_ms.saturating_sub(dt_ms);
            fighter.pose = Pose::Hit;
            self.cpu = fighter;
            self.apply_gravity(dt_ms);
            return;
        }
        if let Some((kind, elapsed)) = fighter.attack {
            let def = move_def(kind);
            let total = def.startup_ms + def.active_ms + def.recovery_ms;
            let next = elapsed + dt_ms;
            if def.projectile && elapsed < def.startup_ms && next >= def.startup_ms {
                self.spawn_projectile(1);
            }
            if next >= total {
                fighter.attack = None;
                fighter.hit_done = false;
                fighter.pose = Pose::Idle;
            } else {
                fighter.attack = Some((kind, next));
                fighter.pose = match kind {
                    MoveKind::Kick => Pose::Kick,
                    MoveKind::Special => Pose::Special,
                    _ => Pose::Punch,
                };
            }
            self.cpu = fighter;
            self.apply_gravity(dt_ms);
            return;
        }

        fighter.facing = if target >= fighter.x { 1 } else { -1 };
        let distance = (target - fighter.x).abs();
        let skill = self.cpu_def.ai_level.min(3) as u32;
        self.ai_timer_ms = self.ai_timer_ms.saturating_sub(dt_ms);
        if self.ai_timer_ms == 0 {
            self.ai_timer_ms = 180 + self.rng.below(160);
            let roll = self.rng.below(100);
            if distance < 34.0 {
                if roll < 30 + skill * 8 {
                    fighter.attack = Some((MoveKind::Throw, 0));
                } else if roll < 55 {
                    fighter.attack = Some((MoveKind::Punch, 0));
                } else if roll < 75 {
                    fighter.attack = Some((MoveKind::Kick, 0));
                }
                fighter.hit_done = false;
                fighter.pose = Pose::Punch;
            } else if distance < 74.0 {
                if roll < 25 + skill * 10 {
                    fighter.attack = Some((MoveKind::Kick, 0));
                } else if roll < 45 {
                    fighter.attack = Some((MoveKind::Punch, 0));
                }
                fighter.hit_done = false;
                fighter.pose = Pose::Kick;
            } else if distance > 130.0 && roll < 12 + skill * 6 {
                fighter.attack = Some((MoveKind::Special, 0));
                fighter.hit_done = false;
                fighter.pose = Pose::Special;
            }
        }

        if fighter.attack.is_none() {
            if distance > 42.0 {
                fighter.x += fighter.facing as f32 * self.cpu_def.walk_speed * dt_ms as f32 / 1000.0;
                fighter.pose = Pose::Walk;
            } else if distance < 20.0 {
                fighter.x -= fighter.facing as f32 * self.cpu_def.walk_speed * 0.5 * dt_ms as f32 / 1000.0;
                fighter.pose = Pose::Walk;
            } else {
                fighter.pose = Pose::Idle;
            }
            fighter.x = fighter.x.clamp(STAGE_LEFT, STAGE_RIGHT);
        }
        self.cpu = fighter;
        self.apply_gravity(dt_ms);
    }

    pub(crate) fn apply_gravity(&mut self, dt_ms: u32) {
        for fighter in [&mut self.player, &mut self.cpu] {
            if !fighter.on_ground {
                fighter.vy += 640.0 * dt_ms as f32 / 1000.0;
                fighter.y += fighter.vy * dt_ms as f32 / 1000.0;
                if fighter.y >= GROUND_Y {
                    fighter.y = GROUND_Y;
                    fighter.vy = 0.0;
                    fighter.on_ground = true;
                    fighter.pose = Pose::Idle;
                }
            }
        }
    }

    pub(crate) fn spawn_projectile(&mut self, owner: usize) {
        let (x, facing, damage) = if owner == 0 {
            (self.player.x + self.player.facing as f32 * 16.0, self.player.facing, self.player_def.special_damage.max(SPECIAL.damage))
        } else {
            (self.cpu.x + self.cpu.facing as f32 * 16.0, self.cpu.facing, self.cpu_def.special_damage.max(SPECIAL.damage))
        };
        self.projectiles.push(Projectile {
            x,
            y: GROUND_Y - 26.0,
            dir: facing as f32,
            damage,
            owner,
            alive: true,
            age_ms: 0,
        });
    }

    pub(crate) fn update_projectiles(&mut self, dt_ms: u32) {
        for projectile in &mut self.projectiles {
            projectile.x += projectile.dir * 150.0 * dt_ms as f32 / 1000.0;
            projectile.age_ms += dt_ms;
            if projectile.x < STAGE_LEFT - 16.0 || projectile.x > STAGE_RIGHT + 16.0 {
                projectile.alive = false;
            }
        }
        self.projectiles.retain(|projectile| projectile.alive);
    }

    pub(crate) fn resolve_hits(&mut self) {
        // Melee: the attacker's active window against the defender's box.
        for attacker in 0..2 {
            let (attacker_x, attacker_y, facing) = if attacker == 0 {
                (self.player.x, self.player.y, self.player.facing)
            } else {
                (self.cpu.x, self.cpu.y, self.cpu.facing)
            };
            let (kind, elapsed, hit_done) = if attacker == 0 {
                match self.player.attack {
                    Some((kind, elapsed)) => (kind, elapsed, self.player.hit_done),
                    None => continue,
                }
            } else {
                match self.cpu.attack {
                    Some((kind, elapsed)) => (kind, elapsed, self.cpu.hit_done),
                    None => continue,
                }
            };
            let def = move_def(kind);
            if def.projectile || hit_done {
                continue;
            }
            if elapsed < def.startup_ms || elapsed > def.startup_ms + def.active_ms {
                continue;
            }
            let (defender_x, defender_y) = if attacker == 0 {
                (self.cpu.x, self.cpu.y)
            } else {
                (self.player.x, self.player.y)
            };
            let dx = (defender_x - attacker_x) * facing as f32;
            let dy = (defender_y - attacker_y).abs();
            if dx > 0.0 && dx < def.reach && dy < 34.0 {
                self.apply_hit(attacker, kind, def, defender_x, defender_y);
            }
        }

        // Projectiles.
        for projectile in &mut self.projectiles {
            let (target_x, target_y) = if projectile.owner == 0 {
                (self.cpu.x, self.cpu.y)
            } else {
                (self.player.x, self.player.y)
            };
            if (projectile.x - target_x).abs() < 14.0 && (projectile.y - target_y + 20.0).abs() < 30.0 {
                projectile.alive = false;
                let damage = projectile.damage;
                let owner = projectile.owner;
                if owner == 0 {
                    self.cpu.health = self.cpu.health.saturating_sub(damage);
                    self.cpu.stun_ms = 260;
                    self.cpu.flash_ms = 160;
                } else {
                    self.player.health = self.player.health.saturating_sub(damage);
                    self.player.stun_ms = 260;
                    self.player.flash_ms = 160;
                }
                self.sparks.push(Spark {
                    x: target_x,
                    y: target_y - 26.0,
                    age_ms: 0,
                });
            }
        }
        self.projectiles.retain(|projectile| projectile.alive);
    }

    pub(crate) fn apply_hit(&mut self, attacker: usize, kind: MoveKind, def: MoveDef, x: f32, y: f32) {
        let blocked = if attacker == 0 {
            self.cpu.pose == Pose::Block && !def.ignores_block
        } else {
            self.player.pose == Pose::Block && !def.ignores_block
        };
        let damage = if blocked { (def.damage / 4).max(1) } else { def.damage };
        let stun = if blocked { 120 } else { 240 };
        if attacker == 0 {
            self.cpu.health = self.cpu.health.saturating_sub(damage);
            self.cpu.stun_ms = stun;
            self.cpu.flash_ms = 140;
            self.cpu.attack = None;
            self.player.hit_done = true;
        } else {
            self.player.health = self.player.health.saturating_sub(damage);
            self.player.stun_ms = stun;
            self.player.flash_ms = 140;
            self.player.attack = None;
            self.cpu.hit_done = true;
        }
        if def.ignores_block {
            if attacker == 0 {
                self.cpu.down = true;
                self.cpu.pose = Pose::Down;
            } else {
                self.player.down = true;
                self.player.pose = Pose::Down;
            }
        }
        self.sparks.push(Spark {
            x,
            y: y - 26.0,
            age_ms: 0,
        });
        let _ = kind;
    }

    pub(crate) fn update_sparks(&mut self, dt_ms: u32) {
        for spark in &mut self.sparks {
            spark.age_ms += dt_ms;
        }
        self.sparks.retain(|spark| spark.age_ms < 240);
        self.player.flash_ms = self.player.flash_ms.saturating_sub(dt_ms);
        self.cpu.flash_ms = self.cpu.flash_ms.saturating_sub(dt_ms);
        if self.player.down && self.player.health > 0 {
            self.player.down = false;
        }
        if self.cpu.down && self.cpu.health > 0 {
            self.cpu.down = false;
        }
    }

    pub fn hash(&self) -> u64 {
        let mut hasher = StateHash::new(self.seed);
        hasher.mix_u64(self.player.health as u64);
        hasher.mix_u64(self.cpu.health as u64);
        hasher.mix_u64(self.round as u64);
        hasher.mix_u64(self.wins[0] as u64);
        hasher.mix_u64(self.wins[1] as u64);
        hasher.mix_u64(self.ladder_index as u64);
        hasher.mix_u64(self.timer_ms as u64);
        hasher.mix_i64((self.player.x * 100.0) as i64);
        hasher.mix_i64((self.cpu.x * 100.0) as i64);
        hasher.mix_u64(self.phase.as_u8() as u64);
        hasher.finish()
    }
}
