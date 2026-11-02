//! A compact, deterministic 2D versus fighter.
//!
//! Two fighters stand on one arena floor. Attacks are frame-windowed (startup,
//! active, recovery), blocking chips damage, throws ignore block and a motion
//! command fires a projectile. Rounds are best of three and a ladder of
//! opponents advances the match.

use pxl_arcade_kit::input::{Dir4, InputState};
use pxl_arcade_kit::replay::StateHash;
use pxl_arcade_kit::rng::Rng;

pub const GROUND_Y: f32 = 176.0;
pub const STAGE_LEFT: f32 = 24.0;
pub const STAGE_RIGHT: f32 = 296.0;
pub const ROUND_MS: u32 = 60_000;
pub const INTRO_MS: u32 = 1400;
pub const ROUND_END_MS: u32 = 1800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    Punch,
    Kick,
    Special,
    Throw,
}

#[derive(Debug, Clone, Copy)]
pub struct MoveDef {
    pub kind: MoveKind,
    pub startup_ms: u32,
    pub active_ms: u32,
    pub recovery_ms: u32,
    pub damage: u32,
    pub reach: f32,
    pub ignores_block: bool,
    pub projectile: bool,
}

pub const PUNCH: MoveDef = MoveDef {
    kind: MoveKind::Punch,
    startup_ms: 60,
    active_ms: 70,
    recovery_ms: 110,
    damage: 6,
    reach: 36.0,
    ignores_block: false,
    projectile: false,
};

pub const KICK: MoveDef = MoveDef {
    kind: MoveKind::Kick,
    startup_ms: 110,
    active_ms: 90,
    recovery_ms: 170,
    damage: 9,
    reach: 48.0,
    ignores_block: false,
    projectile: false,
};

pub const SPECIAL: MoveDef = MoveDef {
    kind: MoveKind::Special,
    startup_ms: 160,
    active_ms: 20,
    recovery_ms: 240,
    damage: 14,
    reach: 0.0,
    ignores_block: false,
    projectile: true,
};

pub const THROW: MoveDef = MoveDef {
    kind: MoveKind::Throw,
    startup_ms: 40,
    active_ms: 60,
    recovery_ms: 220,
    damage: 12,
    reach: 22.0,
    ignores_block: true,
    projectile: false,
};

pub fn move_def(kind: MoveKind) -> MoveDef {
    match kind {
        MoveKind::Punch => PUNCH,
        MoveKind::Kick => KICK,
        MoveKind::Special => SPECIAL,
        MoveKind::Throw => THROW,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FighterDef {
    pub name: &'static str,
    pub tint: u8,
    pub walk_speed: f32,
    pub health: u32,
    pub ai_level: u8,
    pub special_damage: u32,
}

#[derive(Debug, Clone)]
pub struct Cast {
    pub player: FighterDef,
    pub opponents: Vec<FighterDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pose {
    Idle,
    Walk,
    Crouch,
    Jump,
    Punch,
    Kick,
    Block,
    Hit,
    Down,
    Special,
}

#[derive(Debug, Clone, Copy)]
pub struct Fighter {
    pub x: f32,
    pub y: f32,
    pub vy: f32,
    pub facing: i32,
    pub health: u32,
    pub pose: Pose,
    pub pose_ms: u32,
    pub walk_ms: u32,
    pub crouch: bool,
    pub on_ground: bool,
    pub attack: Option<(MoveKind, u32)>,
    pub hit_done: bool,
    pub stun_ms: u32,
    pub down: bool,
    pub flash_ms: u32,
}

impl Fighter {
    fn new(x: f32, facing: i32, health: u32) -> Fighter {
        Fighter {
            x,
            y: GROUND_Y,
            vy: 0.0,
            facing,
            health,
            pose: Pose::Idle,
            pose_ms: 0,
            walk_ms: 0,
            crouch: false,
            on_ground: true,
            attack: None,
            hit_done: false,
            stun_ms: 0,
            down: false,
            flash_ms: 0,
        }
    }

    pub fn anim(&self) -> &'static str {
        match self.pose {
            Pose::Idle => "idle",
            Pose::Walk => "walk",
            Pose::Crouch => "crouch",
            Pose::Jump => "walk",
            Pose::Punch => "punch",
            Pose::Kick => "kick",
            Pose::Block => "block",
            Pose::Hit => "hit",
            Pose::Down => "hit",
            Pose::Special => "punch",
        }
    }

    pub fn busy(&self) -> bool {
        self.attack.is_some() || self.stun_ms > 0 || self.down
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Projectile {
    pub x: f32,
    pub y: f32,
    pub dir: f32,
    pub damage: u32,
    pub owner: usize,
    pub alive: bool,
    pub age_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Intro(u32),
    Fight,
    RoundEnd(u32),
    /// Match finished; `true` means the player took it.
    MatchEnd(bool),
}

#[derive(Debug, Clone, Copy)]
pub struct Spark {
    pub x: f32,
    pub y: f32,
    pub age_ms: u32,
}

pub struct FightSim {
    pub player: Fighter,
    pub cpu: Fighter,
    pub player_def: FighterDef,
    pub cpu_def: FighterDef,
    pub projectiles: Vec<Projectile>,
    pub sparks: Vec<Spark>,
    pub phase: Phase,
    pub round: u8,
    pub wins: [u8; 2],
    pub timer_ms: u32,
    pub clock_ms: u64,
    pub ladder_index: usize,
    opponents: Vec<FighterDef>,
    rng: Rng,
    seed: u64,
    ai_timer_ms: u32,
    motion: Vec<(i32, u32)>,
    round_banner_ms: u32,
}

impl FightSim {
    pub fn new(cast: &Cast, seed: u64) -> Result<FightSim, String> {
        if cast.opponents.is_empty() {
            return Err("the ladder is empty".into());
        }
        let mut sim = FightSim {
            player: Fighter::new(120.0, 1, cast.player.health),
            cpu: Fighter::new(200.0, -1, cast.opponents[0].health),
            player_def: cast.player,
            cpu_def: cast.opponents[0],
            projectiles: Vec::new(),
            sparks: Vec::new(),
            phase: Phase::Intro(INTRO_MS),
            round: 1,
            wins: [0, 0],
            timer_ms: ROUND_MS,
            clock_ms: 0,
            ladder_index: 0,
            opponents: cast.opponents.clone(),
            rng: Rng::new(seed),
            seed,
            ai_timer_ms: 0,
            motion: Vec::new(),
            round_banner_ms: 0,
        };
        sim.start_round();
        Ok(sim)
    }

    pub fn ladder_len(&self) -> usize {
        self.opponents.len()
    }

    pub fn opponent_name(&self) -> &'static str {
        self.cpu_def.name
    }

    fn start_round(&mut self) {
        let health = self.player_def.health;
        self.player = Fighter::new(110.0, 1, health);
        self.cpu = Fighter::new(210.0, -1, self.cpu_def.health);
        self.projectiles.clear();
        self.sparks.clear();
        self.timer_ms = ROUND_MS;
        self.phase = Phase::Intro(INTRO_MS);
        self.ai_timer_ms = 0;
        self.motion.clear();
        self.round_banner_ms = 0;
    }

    fn next_round(&mut self) {
        self.round = self.round.saturating_add(1);
        self.start_round();
    }

    fn next_opponent(&mut self) {
        self.ladder_index += 1;
        if self.ladder_index >= self.opponents.len() {
            self.phase = Phase::MatchEnd(true);
            return;
        }
        self.cpu_def = self.opponents[self.ladder_index];
        self.round = 1;
        self.wins = [0, 0];
        self.start_round();
    }

    pub fn restart(&mut self) {
        self.ladder_index = 0;
        self.cpu_def = self.opponents[0];
        self.round = 1;
        self.wins = [0, 0];
        self.rng = Rng::new(self.seed);
        self.start_round();
    }

    pub fn tick(&mut self, input: InputState, dt_ms: u32) {
        self.clock_ms += dt_ms as u64;
        match self.phase {
            Phase::Intro(timer) => {
                let left = timer.saturating_sub(dt_ms);
                if left == 0 {
                    self.phase = Phase::Fight;
                    self.round_banner_ms = 900;
                } else {
                    self.phase = Phase::Intro(left);
                }
                self.tick_poses(dt_ms);
                return;
            }
            Phase::RoundEnd(timer) => {
                let left = timer.saturating_sub(dt_ms);
                self.tick_poses(dt_ms);
                if left == 0 {
                    let player_won = self.wins[0] >= 2;
                    let cpu_won = self.wins[1] >= 2;
                    if cpu_won {
                        self.phase = Phase::MatchEnd(false);
                    } else if player_won {
                        self.next_opponent();
                    } else {
                        self.next_round();
                    }
                } else {
                    self.phase = Phase::RoundEnd(left);
                }
                return;
            }
            Phase::MatchEnd(_) => {
                if input.start_pressed() {
                    self.restart();
                }
                self.tick_poses(dt_ms);
                return;
            }
            Phase::Fight => {}
        }

        self.round_banner_ms = self.round_banner_ms.saturating_sub(dt_ms);
        self.timer_ms = self.timer_ms.saturating_sub(dt_ms);
        self.update_motion(input, dt_ms);
        self.update_player(input, dt_ms);
        self.update_cpu(dt_ms);
        self.update_projectiles(dt_ms);
        self.resolve_hits();
        self.update_sparks(dt_ms);

        if self.player.health == 0 || self.cpu.health == 0 || self.timer_ms == 0 {
            let player_won = if self.cpu.health == 0 {
                true
            } else if self.player.health == 0 {
                false
            } else {
                self.player.health >= self.cpu.health
            };
            if player_won {
                self.wins[0] += 1;
            } else {
                self.wins[1] += 1;
            }
            self.phase = Phase::RoundEnd(ROUND_END_MS);
        }
    }

    fn tick_poses(&mut self, dt_ms: u32) {
        self.player.pose_ms = self.player.pose_ms.saturating_add(dt_ms);
        self.cpu.pose_ms = self.cpu.pose_ms.saturating_add(dt_ms);
        for spark in &mut self.sparks {
            spark.age_ms += dt_ms;
        }
        self.sparks.retain(|spark| spark.age_ms < 240);
    }

    fn update_motion(&mut self, input: InputState, dt_ms: u32) {
        let now = self.clock_ms as u32;
        if let Some(dir) = input.dir_held() {
            let relative = match dir {
                Dir4::Left => -1,
                Dir4::Right => 1,
                _ => 0,
            };
            if relative != 0 {
                let last = self.motion.last().map(|entry| entry.0);
                if last != Some(relative) {
                    self.motion.push((relative, now));
                }
            }
        }
        self.motion.retain(|entry| now.saturating_sub(entry.1) < 600);
        let _ = dt_ms;
    }

    fn wants_special(&self) -> bool {
        let mut seen_away = false;
        for (dir, _) in &self.motion {
            let relative = dir * self.player.facing;
            if relative < 0 {
                seen_away = true;
            } else if relative > 0 && seen_away {
                return true;
            }
        }
        false
    }

    fn can_block(&self, fighter: &Fighter, opponent: &Fighter, input: InputState) -> bool {
        let away = match opponent.x > fighter.x {
            true => input.held.left,
            false => input.held.right,
        };
        away && fighter.on_ground && fighter.attack.is_none() && fighter.stun_ms == 0
    }

    fn update_player(&mut self, input: InputState, dt_ms: u32) {
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

    fn update_cpu(&mut self, dt_ms: u32) {
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

    fn apply_gravity(&mut self, dt_ms: u32) {
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

    fn spawn_projectile(&mut self, owner: usize) {
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

    fn update_projectiles(&mut self, dt_ms: u32) {
        for projectile in &mut self.projectiles {
            projectile.x += projectile.dir * 150.0 * dt_ms as f32 / 1000.0;
            projectile.age_ms += dt_ms;
            if projectile.x < STAGE_LEFT - 16.0 || projectile.x > STAGE_RIGHT + 16.0 {
                projectile.alive = false;
            }
        }
        self.projectiles.retain(|projectile| projectile.alive);
    }

    fn resolve_hits(&mut self) {
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

    fn apply_hit(&mut self, attacker: usize, kind: MoveKind, def: MoveDef, x: f32, y: f32) {
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

    fn update_sparks(&mut self, dt_ms: u32) {
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

impl Phase {
    pub fn as_u8(self) -> u8 {
        match self {
            Phase::Intro(_) => 0,
            Phase::Fight => 1,
            Phase::RoundEnd(_) => 2,
            Phase::MatchEnd(_) => 3,
        }
    }
}

impl std::convert::From<Phase> for u8 {
    fn from(phase: Phase) -> u8 {
        phase.as_u8()
    }
}
