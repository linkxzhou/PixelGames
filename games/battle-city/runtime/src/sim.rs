//! Deterministic fixed-step simulation.
//!
//! No windowing, no wall clock, no I/O: the same level, seed and input
//! sequence always give the same state, so the game logic is unit testable and
//! replays are stable.
//!
//! Rules follow 策划案.md: 20 enemies per stage, four classes, three spawn
//! mouths, four enemies on screen, the 4th/11th/18th spawn carries a power-up,
//! two-hit bricks, steel that needs a tier-three player shell, ice sliding,
//! water that blocks tanks but not shells, grass drawn above tanks, and an HQ
//! that loses the run when any shell touches it.

use crate::enemy::EnemyKind;
use crate::level::Level;
use crate::powerup::{PowerKind, CLOCK_SECS, HELMET_SECS, POWERUP_LIFE, SHIELD_SECS, SHOVEL_SECS};

pub const TILE: f32 = 32.0;
pub const TANK_SIZE: f32 = 32.0;
pub const FIELD_W: f32 = 416.0;
pub const FIELD_H: f32 = 416.0;
pub const BULLET_SIZE: f32 = 8.0;
pub const PLAYER_SPEED: f32 = 92.0;
pub const PLAYER_FIRE_CD: f32 = 0.30;
pub const PLAYER_LIVES: u8 = 3;
pub const RESPAWN_DELAY: f32 = 1.3;
pub const SPAWN_FLASH_TIME: f32 = 0.75;
pub const SPAWN_GAP: f32 = 1.35;
pub const BURST_LIFE: f32 = 0.45;
pub const STAGE_ENEMIES: u32 = 20;
pub const MAX_ALIVE: usize = 4;
pub const TOTAL_STAGES: usize = 35;
pub const BONUS_SCORE: u32 = 20_000;
pub const CARRIER_SPAWNS: [usize; 3] = [3, 10, 17];

/// Enemy spawn cells: the two top corners and the top centre.
pub const SPAWN_CELLS: [[i32; 2]; 3] = [[0, 0], [6, 0], [12, 0]];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    N,
    E,
    S,
    W,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::N, Dir::E, Dir::S, Dir::W];

    pub fn delta(self) -> (f32, f32) {
        match self {
            Dir::N => (0.0, -1.0),
            Dir::E => (1.0, 0.0),
            Dir::S => (0.0, 1.0),
            Dir::W => (-1.0, 0.0),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Dir::N => "N",
            Dir::E => "E",
            Dir::S => "S",
            Dir::W => "W",
        }
    }

    pub fn is_horizontal(self) -> bool {
        matches!(self, Dir::E | Dir::W)
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::N => Dir::S,
            Dir::E => Dir::W,
            Dir::S => Dir::N,
            Dir::W => Dir::E,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Input {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub fire: bool,
}

impl Input {
    pub fn dir(&self) -> Option<Dir> {
        if self.up {
            Some(Dir::N)
        } else if self.down {
            Some(Dir::S)
        } else if self.left {
            Some(Dir::W)
        } else if self.right {
            Some(Dir::E)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Playing,
    StageClear,
    Victory,
    GameOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Player,
    Enemy,
}
#[derive(Debug, Clone)]
pub struct Tank {
    pub x: f32,
    pub y: f32,
    pub dir: Dir,
    pub moving: bool,
}

impl Tank {
    fn new(x: f32, y: f32, dir: Dir) -> Self {
        Self {
            x,
            y,
            dir,
            moving: false,
        }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + TANK_SIZE * 0.5, self.y + TANK_SIZE * 0.5)
    }
}

#[derive(Debug, Clone)]
pub struct Enemy {
    pub tank: Tank,
    pub kind: EnemyKind,
    pub hp: u8,
    pub fire_in: f32,
    pub turn_in: f32,
    /// The 4th/11th/18th spawn flashes red and drops a power-up when hit.
    pub carrier: bool,
    /// White flash timer after taking a non-lethal hit.
    pub flash_in: f32,
    /// Shells this tank currently has in the air.
    pub live_shells: u32,
}

impl Enemy {
    pub fn center(&self) -> (f32, f32) {
        self.tank.center()
    }
}

#[derive(Debug, Clone)]
pub struct Bullet {
    pub side: Side,
    pub x: f32,
    pub y: f32,
    pub dir: Dir,
    pub speed: f32,
    /// A tier-three player shell breaks steel.
    pub break_steel: bool,
    /// Index into `enemies` for enemy shells, so the per-tank limit works.
    pub owner: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Burst {
    pub x: f32,
    pub y: f32,
    pub age: f32,
    pub big: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Drop {
    pub kind: PowerKind,
    pub x: f32,
    pub y: f32,
    pub age: f32,
}

#[derive(Debug, Clone)]
pub struct SpawnFlash {
    pub cell: (i32, i32),
    pub kind: EnemyKind,
    pub carrier: bool,
    pub timer: f32,
}

/// SplitMix64: deterministic across platforms because it only uses wrapping
/// integer arithmetic.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x2545_F491_4F6C_DD1D,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / ((1u32 << 24) as f32)
    }

    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as u32
        }
    }
}

/// Which body a movement check is testing, so a tank never collides with
/// itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Body {
    Player,
    Enemy(usize),
    Static,
}

/// One stage of the game.
#[derive(Debug, Clone)]
pub struct Sim {
    pub stage: usize,
    pub level: Level,
    pub player: Tank,
    pub player_alive: bool,
    pub lives: u8,
    /// Firepower tier 0..=3: faster shell, two shells, steel breaking.
    pub star: u8,
    pub shield_in: f32,
    pub respawn_in: f32,
    pub enemies: Vec<Enemy>,
    pub bullets: Vec<Bullet>,
    pub bursts: Vec<Burst>,
    pub drops: Vec<Drop>,
    pub spawn_flashes: Vec<SpawnFlash>,
    pub score: u32,
    /// Highest score seen before this run, for the HUD.
    pub high_score: u32,
    /// Enemies not yet destroyed, including the ones on screen.
    pub remaining: u32,
    pub outcome: Outcome,
    pub time: f32,
    pub ticks: u64,
    /// Rises when the base explodes, decays every step; the renderer shakes.
    pub shake: f32,
    pub base_alive: bool,
    pub base_cell: (i32, i32),
    pub shovel_in: f32,
    pub clock_in: f32,
    rng: Rng,
    spawn_slot: usize,
    spawn_in: f32,
    spawned: u32,
    /// Kinds in spawn order, shuffled once per stage from the level mix.
    order: Vec<EnemyKind>,
    player_fire_cd: f32,
    /// Ice momentum: the last direction and how long it keeps carrying.
    slide_dir: Option<Dir>,
    slide_in: f32,
    base_walls: Vec<(i32, i32)>,
    bonus_at: u32,
}

impl Sim {
    pub fn new(stage: usize, level: Level) -> Self {
        let base_cell = level.base.unwrap_or((level.w / 2, level.h - 1));
        let spawn = level.spawn;
        let base_walls = level.wall_cells();
        let seed = level
            .seed
            .wrapping_add((stage as u64 + 1).wrapping_mul(0x9E37_79B9));
        let mut rng = Rng::new(seed);
        let mut order = Vec::with_capacity(STAGE_ENEMIES as usize);
        for (index, kind) in [
            EnemyKind::Basic,
            EnemyKind::Fast,
            EnemyKind::Power,
            EnemyKind::Armor,
        ]
        .into_iter()
        .enumerate()
        {
            for _ in 0..level.composition[index] {
                order.push(kind);
            }
        }
        while order.len() < STAGE_ENEMIES as usize {
            order.push(EnemyKind::Basic);
        }
        order.truncate(STAGE_ENEMIES as usize);
        // Fisher-Yates, so every stage mixes the classes in a stable order.
        for i in (1..order.len()).rev() {
            let j = rng.below((i + 1) as u32) as usize;
            order.swap(i, j);
        }
        const _: () = assert!(TOTAL_STAGES > 0);
        Self {
            stage,
            player: Tank::new(spawn.0 as f32 * TILE, spawn.1 as f32 * TILE, Dir::N),
            player_alive: true,
            lives: PLAYER_LIVES,
            star: 0,
            shield_in: SHIELD_SECS,
            respawn_in: 0.0,
            enemies: Vec::new(),
            bullets: Vec::new(),
            bursts: Vec::new(),
            drops: Vec::new(),
            spawn_flashes: Vec::new(),
            score: 0,
            high_score: 0,
            remaining: STAGE_ENEMIES,
            outcome: Outcome::Playing,
            time: 0.0,
            ticks: 0,
            shake: 0.0,
            base_alive: true,
            base_cell,
            shovel_in: 0.0,
            clock_in: 0.0,
            rng,
            spawn_slot: 0,
            spawn_in: 0.6,
            spawned: 0,
            order,
            player_fire_cd: 0.0,
            slide_dir: None,
            slide_in: 0.0,
            base_walls,
            bonus_at: BONUS_SCORE,
            level,
        }
    }

    pub fn total_stages() -> usize {
        TOTAL_STAGES
    }

    pub fn frozen(&self) -> bool {
        self.clock_in > 0.0
    }
    /// Advance one fixed step. `dt` is normally 1/60.
    pub fn step(&mut self, input: Input, dt: f32) {
        self.time += dt;
        self.ticks += 1;
        self.shake = (self.shake - dt * 6.0).max(0.0);
        self.age_bursts_and_drops(dt);
        if self.outcome != Outcome::Playing {
            return;
        }
        self.tick_timers(dt);
        self.update_player(input, dt);
        self.update_enemies(dt);
        self.update_bullets(dt);
        self.update_spawns(dt);
        self.update_pickups();
        self.evaluate_outcome();
    }

    fn tick_timers(&mut self, dt: f32) {
        self.shield_in = (self.shield_in - dt).max(0.0);
        self.player_fire_cd = (self.player_fire_cd - dt).max(0.0);
        self.clock_in = (self.clock_in - dt).max(0.0);
        if self.shovel_in > 0.0 {
            self.shovel_in -= dt;
            if self.shovel_in <= 0.0 {
                self.shovel_in = 0.0;
                for (x, y) in self.base_walls.clone() {
                    if self.level.is_steel_at(x, y) {
                        self.level.set_at(x, y, "b");
                    }
                }
            }
        }
    }

    // ---- player ----

    fn update_player(&mut self, input: Input, dt: f32) {
        if !self.player_alive {
            self.respawn_in -= dt;
            if self.respawn_in <= 0.0 {
                if self.lives == 0 {
                    self.outcome = Outcome::GameOver;
                } else {
                    let (sx, sy) = (
                        self.level.spawn.0 as f32 * TILE,
                        self.level.spawn.1 as f32 * TILE,
                    );
                    if self.can_stand(sx, sy, Body::Static) {
                        self.player.x = sx;
                        self.player.y = sy;
                        self.player_alive = true;
                        self.player.dir = Dir::N;
                        self.player.moving = false;
                        self.shield_in = SHIELD_SECS;
                        self.player_fire_cd = 0.2;
                        self.slide_dir = None;
                        self.slide_in = 0.0;
                    } else {
                        // The spawn cell is occupied: wait for it to clear.
                        self.respawn_in = 0.25;
                    }
                }
            }
            return;
        }

        let on_ice = self.player_on_ice();
        if let Some(dir) = input.dir() {
            if dir != self.player.dir {
                self.player.dir = dir;
                let (nx, ny) = lane_snap(self.player.x, self.player.y, dir);
                if self.can_stand(nx, ny, Body::Player) {
                    self.player.x = nx;
                    self.player.y = ny;
                }
            }
            let speed = if on_ice {
                PLAYER_SPEED * 1.1
            } else {
                PLAYER_SPEED
            };
            let (dx, dy) = dir.delta();
            self.player.moving = self.try_move_player(dx * speed * dt, dy * speed * dt);
            if on_ice {
                self.slide_dir = Some(dir);
                self.slide_in = 0.28;
            }
        } else if on_ice && self.slide_in > 0.0 {
            // Released on ice: keep gliding for a moment, then stop.
            let dir = self.slide_dir.unwrap_or(self.player.dir);
            let (dx, dy) = dir.delta();
            self.player.moving =
                self.try_move_player(dx * PLAYER_SPEED * 0.75 * dt, dy * PLAYER_SPEED * 0.75 * dt);
            if !self.player.moving {
                self.slide_in = 0.0;
            } else {
                self.slide_in -= dt;
            }
        } else {
            self.player.moving = false;
            self.slide_in = 0.0;
        }

        if input.fire && self.fire_ready(Side::Player) {
            let tank = self.player.clone();
            self.fire(Side::Player, &tank, 0);
            self.player_fire_cd = PLAYER_FIRE_CD;
        }
    }

    fn player_on_ice(&self) -> bool {
        let (x, y) = self.player.center();
        self.level
            .is_ice_at(self.level.cell_of(x, y).0, self.level.cell_of(x, y).1)
    }

    fn try_move_player(&mut self, dx: f32, dy: f32) -> bool {
        let mut moved = false;
        if dx != 0.0 {
            let nx = self.player.x + dx;
            if self.can_stand(nx, self.player.y, Body::Player) {
                self.player.x = nx;
                moved = true;
            }
        }
        if dy != 0.0 {
            let ny = self.player.y + dy;
            if self.can_stand(self.player.x, ny, Body::Player) {
                self.player.y = ny;
                moved = true;
            }
        }
        moved
    }

    // ---- enemies ----

    fn update_enemies(&mut self, dt: f32) {
        let base = self.level.base.unwrap_or(self.level.exit);
        let base_center = (
            base.0 as f32 * TILE + TILE * 0.5,
            base.1 as f32 * TILE + TILE * 0.5,
        );
        let frozen = self.frozen();
        for index in 0..self.enemies.len() {
            self.enemies[index].flash_in = (self.enemies[index].flash_in - dt).max(0.0);
            if frozen {
                self.enemies[index].tank.moving = false;
                continue;
            }
            self.enemies[index].fire_in -= dt;
            if self.enemies[index].fire_in <= 0.0 && self.enemy_shells(index) == 0 {
                let tank = self.enemies[index].tank.clone();
                self.fire(Side::Enemy, &tank, index);
                let jitter = 0.7 + self.rng.next_f32() * 0.6;
                self.enemies[index].fire_in = self.enemies[index].kind.fire_cd() * jitter;
            }
            self.enemies[index].turn_in -= dt;
            if self.enemies[index].turn_in <= 0.0 {
                self.steer_enemy(index, base_center);
            }
            let dir = self.enemies[index].tank.dir;
            let (dx, dy) = dir.delta();
            let speed = self.enemies[index].kind.speed();
            let moved = self.try_move_enemy(index, dx * speed * dt, dy * speed * dt);
            self.enemies[index].tank.moving = moved;
            if !moved {
                self.enemies[index].turn_in = 0.0;
            }
        }
    }
    fn steer_enemy(&mut self, index: usize, base_center: (f32, f32)) {
        let tank = self.enemies[index].tank.clone();
        let kind = self.enemies[index].kind;
        let (cx, cy) = tank.center();
        let dx = base_center.0 - cx;
        let dy = base_center.1 - cy;
        let horizontal_first = dx.abs() > dy.abs();
        let chase = if horizontal_first {
            if dx > 0.0 {
                Dir::E
            } else {
                Dir::W
            }
        } else if dy > 0.0 {
            Dir::S
        } else {
            Dir::N
        };
        let hunting = self.rng.next_f32() < kind.base_bias();
        let mut candidates: Vec<Dir> = Vec::with_capacity(4);
        if hunting {
            candidates.push(chase);
        }
        let start = self.rng.below(4) as usize;
        for offset in 0..4 {
            let dir = Dir::ALL[(start + offset) % 4];
            if !candidates.contains(&dir) {
                candidates.push(dir);
            }
        }
        for dir in candidates {
            let (nx, ny) = lane_snap(tank.x, tank.y, dir);
            let stand = self.can_stand(nx, ny, Body::Enemy(index));
            if stand {
                self.enemies[index].tank.dir = dir;
                self.enemies[index].tank.x = nx;
                self.enemies[index].tank.y = ny;
                break;
            }
        }
        self.enemies[index].turn_in = 0.7 + self.rng.next_f32() * 1.6;
    }

    fn try_move_enemy(&mut self, index: usize, dx: f32, dy: f32) -> bool {
        let mut moved = false;
        let body = Body::Enemy(index);
        if dx != 0.0 {
            let nx = self.enemies[index].tank.x + dx;
            if self.can_stand(nx, self.enemies[index].tank.y, body) {
                self.enemies[index].tank.x = nx;
                moved = true;
            }
        }
        if dy != 0.0 {
            let ny = self.enemies[index].tank.y + dy;
            if self.can_stand(self.enemies[index].tank.x, ny, body) {
                self.enemies[index].tank.y = ny;
                moved = true;
            }
        }
        moved
    }

    // ---- shells ----

    fn player_shell_limit(&self) -> usize {
        if self.star >= 2 {
            2
        } else {
            1
        }
    }

    fn alive_shells(&self, side: Side) -> usize {
        self.bullets.iter().filter(|b| b.side == side).count()
    }

    fn enemy_shells(&self, owner: usize) -> u32 {
        self.enemies
            .get(owner)
            .map(|enemy| enemy.live_shells)
            .unwrap_or(0)
    }

    fn fire_ready(&self, side: Side) -> bool {
        match side {
            Side::Player => self.alive_shells(Side::Player) < self.player_shell_limit(),
            Side::Enemy => self.alive_shells(Side::Enemy) < 6,
        }
    }

    fn fire(&mut self, side: Side, tank: &Tank, owner: usize) {
        let (cx, cy) = tank.center();
        let (dx, dy) = tank.dir.delta();
        let x = cx + dx * (TANK_SIZE * 0.5) - BULLET_SIZE * 0.5;
        let y = cy + dy * (TANK_SIZE * 0.5) - BULLET_SIZE * 0.5;
        let (speed, break_steel) = match side {
            Side::Player => (if self.star >= 1 { 350.0 } else { 280.0 }, self.star >= 3),
            Side::Enemy => (
                self.enemies
                    .get(owner)
                    .map(|e| e.kind.shell_speed())
                    .unwrap_or(260.0),
                false,
            ),
        };
        if let Some(enemy) = self.enemies.get_mut(owner) {
            enemy.live_shells += 1;
        }
        self.bullets.push(Bullet {
            side,
            x: x.clamp(0.0, FIELD_W - BULLET_SIZE),
            y: y.clamp(0.0, FIELD_H - BULLET_SIZE),
            dir: tank.dir,
            speed,
            break_steel,
            owner,
        });
    }

    fn update_bullets(&mut self, dt: f32) {
        let mut survivors = Vec::with_capacity(self.bullets.len());
        for mut bullet in std::mem::take(&mut self.bullets) {
            let distance = bullet.speed * dt;
            let substeps = (distance / 2.0).ceil().max(1.0) as i32;
            let stride = distance / substeps as f32;
            let mut alive = true;
            for _ in 0..substeps {
                let (dx, dy) = bullet.dir.delta();
                bullet.x += dx * stride;
                bullet.y += dy * stride;
                if !self.resolve_bullet(&mut bullet) {
                    alive = false;
                    break;
                }
            }
            if alive {
                survivors.push(bullet);
            } else if bullet.side == Side::Enemy {
                if let Some(enemy) = self.enemies.get_mut(bullet.owner) {
                    enemy.live_shells = enemy.live_shells.saturating_sub(1);
                }
            }
        }
        self.bullets = survivors;
        self.cancel_bullets();
    }

    /// Returns false when the shell is consumed.
    fn resolve_bullet(&mut self, bullet: &mut Bullet) -> bool {
        if bullet.x < 0.0
            || bullet.y < 0.0
            || bullet.x + BULLET_SIZE > FIELD_W
            || bullet.y + BULLET_SIZE > FIELD_H
        {
            return false;
        }
        // Tanks first: a tank hugging a wall is hit by the shell that overlaps it.
        match bullet.side {
            Side::Player => {
                for index in 0..self.enemies.len() {
                    let tank = self.enemies[index].tank.clone();
                    if overlaps(bullet.x, bullet.y, BULLET_SIZE, tank.x, tank.y, TANK_SIZE) {
                        if self.enemies[index].hp > 1 {
                            self.enemies[index].hp -= 1;
                            self.enemies[index].flash_in = 0.18;
                            self.bursts.push(Burst {
                                x: tank.x,
                                y: tank.y,
                                age: 0.0,
                                big: false,
                            });
                        } else {
                            self.kill_enemy(index);
                        }
                        return false;
                    }
                }
            }
            Side::Enemy => {
                if self.player_alive
                    && overlaps(
                        bullet.x,
                        bullet.y,
                        BULLET_SIZE,
                        self.player.x,
                        self.player.y,
                        TANK_SIZE,
                    )
                {
                    if self.shield_in > 0.0 {
                        self.bursts.push(Burst {
                            x: bullet.x - 12.0,
                            y: bullet.y - 12.0,
                            age: 0.0,
                            big: false,
                        });
                    } else {
                        self.kill_player();
                    }
                    return false;
                }
            }
        }
        if self.base_alive {
            let bx = self.base_cell.0 as f32 * TILE;
            let by = self.base_cell.1 as f32 * TILE;
            if overlaps(bullet.x, bullet.y, BULLET_SIZE, bx, by, TILE) {
                self.base_alive = false;
                self.shake = 1.0;
                self.bursts.push(Burst {
                    x: bx,
                    y: by,
                    age: 0.0,
                    big: true,
                });
                self.outcome = Outcome::GameOver;
                return false;
            }
        }
        // Walls: the shell tests the cells it overlaps.
        let x0 = (bullet.x / TILE).floor() as i32;
        let x1 = ((bullet.x + BULLET_SIZE - 0.01) / TILE).floor() as i32;
        let y0 = (bullet.y / TILE).floor() as i32;
        let y1 = ((bullet.y + BULLET_SIZE - 0.01) / TILE).floor() as i32;
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                if !self.level.blocks_bullet_at(cx, cy) {
                    continue;
                }
                let mut destroyed = false;
                if self.level.is_breakable_at(cx, cy) {
                    destroyed = self.level.hit_brick(cx, cy);
                } else if self.level.is_steel_at(cx, cy) && bullet.break_steel {
                    self.level.set_at(cx, cy, "f");
                    destroyed = true;
                }
                self.bursts.push(Burst {
                    x: cx as f32 * TILE,
                    y: cy as f32 * TILE,
                    age: 0.0,
                    big: destroyed,
                });
                return false;
            }
        }
        true
    }
    fn kill_enemy(&mut self, index: usize) {
        let enemy = self.enemies.remove(index);
        self.remaining = self.remaining.saturating_sub(1);
        self.score += enemy.kind.score();
        self.bursts.push(Burst {
            x: enemy.tank.x,
            y: enemy.tank.y,
            age: 0.0,
            big: true,
        });
        if enemy.carrier {
            let kind = PowerKind::from_index(self.stage as u32 * 3 + index as u32);
            // A new carrier's drop replaces whatever is on the field.
            self.drops.clear();
            self.drops.push(Drop {
                kind,
                x: enemy.tank.x,
                y: enemy.tank.y,
                age: 0.0,
            });
        }
        self.award_bonus();
        // Shell indices shift after a removal; rebase them.
        for bullet in &mut self.bullets {
            if bullet.side == Side::Enemy && bullet.owner > index {
                bullet.owner -= 1;
            }
        }
    }

    fn kill_all_enemies(&mut self) {
        let count = self.enemies.len();
        for _ in 0..count {
            let enemy = self.enemies.remove(0);
            self.remaining = self.remaining.saturating_sub(1);
            self.score += enemy.kind.score();
            self.bursts.push(Burst {
                x: enemy.tank.x,
                y: enemy.tank.y,
                age: 0.0,
                big: true,
            });
        }
        self.bullets.retain(|bullet| bullet.side != Side::Enemy);
        self.award_bonus();
    }

    fn award_bonus(&mut self) {
        while self.score >= self.bonus_at {
            self.lives = self.lives.saturating_add(1);
            self.bonus_at = self.bonus_at.saturating_add(BONUS_SCORE);
        }
    }

    fn kill_player(&mut self) {
        self.player_alive = false;
        self.lives = self.lives.saturating_sub(1);
        self.star = 0;
        self.shield_in = 0.0;
        self.slide_dir = None;
        self.slide_in = 0.0;
        self.respawn_in = RESPAWN_DELAY;
        self.bursts.push(Burst {
            x: self.player.x,
            y: self.player.y,
            age: 0.0,
            big: true,
        });
        if self.lives == 0 {
            self.outcome = Outcome::GameOver;
        }
    }

    /// Player and enemy shells cancel each other on contact.
    fn cancel_bullets(&mut self) {
        let mut remove = vec![false; self.bullets.len()];
        for i in 0..self.bullets.len() {
            if remove[i] {
                continue;
            }
            for j in (i + 1)..self.bullets.len() {
                if remove[j] || self.bullets[i].side == self.bullets[j].side {
                    continue;
                }
                let a = &self.bullets[i];
                let b = &self.bullets[j];
                if overlaps(a.x, a.y, BULLET_SIZE, b.x, b.y, BULLET_SIZE) {
                    remove[i] = true;
                    remove[j] = true;
                    break;
                }
            }
        }
        let mut index = 0;
        self.bullets.retain(|_| {
            let keep = !remove[index];
            index += 1;
            keep
        });
    }

    // ---- spawning ----

    fn update_spawns(&mut self, dt: f32) {
        for flash in &mut self.spawn_flashes {
            flash.timer -= dt;
        }
        // Resolve finished flashes first, keeping the arena clean.
        let mut index = 0;
        while index < self.spawn_flashes.len() {
            if self.spawn_flashes[index].timer > 0.0 {
                index += 1;
                continue;
            }
            let flash = self.spawn_flashes[index].clone();
            let x = flash.cell.0 as f32 * TILE;
            let y = flash.cell.1 as f32 * TILE;
            if self.level.blocks_tank_at(flash.cell.0, flash.cell.1)
                || !self.can_stand(x, y, Body::Static)
            {
                self.spawn_flashes[index].timer = 0.2;
                index += 1;
                continue;
            }
            self.spawn_flashes.remove(index);
            self.enemies.push(Enemy {
                tank: Tank::new(x, y, Dir::S),
                kind: flash.kind,
                hp: flash.kind.hp(),
                fire_in: flash.kind.fire_cd() * (0.6 + self.rng.next_f32() * 0.8),
                turn_in: 0.5 + self.rng.next_f32(),
                carrier: flash.carrier,
                flash_in: 0.0,
                live_shells: 0,
            });
            if flash.carrier {
                self.drops.clear();
            }
        }

        if self.spawned as usize >= self.order.len() {
            return;
        }
        self.spawn_in -= dt;
        if self.spawn_in > 0.0 {
            return;
        }
        if self.enemies.len() + self.spawn_flashes.len() >= MAX_ALIVE {
            self.spawn_in = 0.25;
            return;
        }
        let spawn_index = self.spawned as usize;
        let kind = self.order[spawn_index];
        let carrier = CARRIER_SPAWNS.contains(&spawn_index);
        let mut chosen = None;
        for attempt in 0..SPAWN_CELLS.len() {
            let slot = (self.spawn_slot + attempt) % SPAWN_CELLS.len();
            let cell = SPAWN_CELLS[slot];
            if !self.level.in_bounds(cell[0], cell[1]) {
                continue;
            }
            if self.level.blocks_tank_at(cell[0], cell[1]) {
                continue;
            }
            let x = cell[0] as f32 * TILE;
            let y = cell[1] as f32 * TILE;
            if !self.can_stand(x, y, Body::Static) {
                continue;
            }
            if self
                .spawn_flashes
                .iter()
                .any(|flash| flash.cell == (cell[0], cell[1]))
            {
                continue;
            }
            self.spawn_slot = (slot + 1) % SPAWN_CELLS.len();
            chosen = Some((cell[0], cell[1]));
            break;
        }
        if let Some(cell) = chosen {
            self.spawned += 1;
            self.spawn_flashes.push(SpawnFlash {
                cell,
                kind,
                carrier,
                timer: SPAWN_FLASH_TIME,
            });
            self.spawn_in = SPAWN_GAP;
        } else {
            self.spawn_in = 0.35;
        }
    }

    // ---- power-ups ----

    fn update_pickups(&mut self) {
        if !self.player_alive {
            self.drops.retain(|drop| drop.age < POWERUP_LIFE);
            return;
        }
        let mut taken = Vec::new();
        for (index, drop) in self.drops.iter().enumerate() {
            if overlaps(
                self.player.x,
                self.player.y,
                TANK_SIZE,
                drop.x,
                drop.y,
                16.0,
            ) {
                taken.push((index, drop.kind));
            }
        }
        for (index, kind) in taken.into_iter().rev() {
            self.drops.remove(index);
            self.apply_power(kind);
        }
        self.drops.retain(|drop| drop.age < POWERUP_LIFE);
    }

    fn apply_power(&mut self, kind: PowerKind) {
        self.score += kind.score();
        self.award_bonus();
        match kind {
            PowerKind::Grenade => self.kill_all_enemies(),
            PowerKind::Helmet => self.shield_in = HELMET_SECS,
            PowerKind::Shovel => {
                self.shovel_in = SHOVEL_SECS;
                for (x, y) in self.base_walls.clone() {
                    self.level.set_at(x, y, "x");
                }
            }
            PowerKind::Star => self.star = (self.star + 1).min(3),
            PowerKind::Tank => self.lives = self.lives.saturating_add(1),
            PowerKind::Clock => self.clock_in = CLOCK_SECS,
        }
    }
    // ---- geometry ----

    fn can_stand(&self, x: f32, y: f32, ignore: Body) -> bool {
        if x < 0.0 || y < 0.0 || x + TANK_SIZE > FIELD_W || y + TANK_SIZE > FIELD_H {
            return false;
        }
        let x0 = (x / TILE).floor() as i32;
        let x1 = ((x + TANK_SIZE - 0.01) / TILE).floor() as i32;
        let y0 = (y / TILE).floor() as i32;
        let y1 = ((y + TANK_SIZE - 0.01) / TILE).floor() as i32;
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                if self.level.blocks_tank_at(cx, cy) {
                    return false;
                }
                if self.base_alive && self.level.base == Some((cx, cy)) {
                    return false;
                }
            }
        }
        if ignore != Body::Player
            && self.player_alive
            && overlaps(x, y, TANK_SIZE, self.player.x, self.player.y, TANK_SIZE)
        {
            return false;
        }
        for (index, enemy) in self.enemies.iter().enumerate() {
            if ignore == Body::Enemy(index) {
                continue;
            }
            if overlaps(x, y, TANK_SIZE, enemy.tank.x, enemy.tank.y, TANK_SIZE) {
                return false;
            }
        }
        true
    }

    fn age_bursts_and_drops(&mut self, dt: f32) {
        for burst in &mut self.bursts {
            burst.age += dt;
        }
        self.bursts.retain(|b| b.age < BURST_LIFE);
        for drop in &mut self.drops {
            drop.age += dt;
        }
        self.drops.retain(|d| d.age < POWERUP_LIFE);
    }

    fn evaluate_outcome(&mut self) {
        if !self.base_alive {
            self.outcome = Outcome::GameOver;
            return;
        }
        if self.lives == 0 && !self.player_alive {
            self.outcome = Outcome::GameOver;
            return;
        }
        if self.remaining == 0 && self.enemies.is_empty() && self.spawn_flashes.is_empty() {
            self.outcome = if self.stage + 1 >= TOTAL_STAGES {
                Outcome::Victory
            } else {
                Outcome::StageClear
            };
        }
    }

    // ---- test and replay helpers ----

    /// Stable summary used by the replay test.
    pub fn fingerprint(&self) -> (u64, u32, u32, usize, usize, i64, i64) {
        (
            self.time as u64,
            self.score,
            self.remaining,
            self.enemies.len(),
            self.bullets.len(),
            (self.player.x * 4.0) as i64,
            (self.player.y * 4.0) as i64,
        )
    }

    /// Spawn one enemy immediately, bypassing the timer. Used by tests.
    pub fn force_spawn(&mut self, x: f32, y: f32, dir: Dir, kind: EnemyKind) -> usize {
        self.enemies.push(Enemy {
            tank: Tank::new(x, y, dir),
            kind,
            hp: kind.hp(),
            fire_in: 999.0,
            turn_in: 999.0,
            carrier: false,
            flash_in: 0.0,
            live_shells: 0,
        });
        self.enemies.len() - 1
    }

    /// Place a shell without going through a tank. Used by tests.
    pub fn force_bullet(&mut self, side: Side, x: f32, y: f32, dir: Dir) {
        self.bullets.push(Bullet {
            side,
            x,
            y,
            dir,
            speed: 280.0,
            break_steel: side == Side::Player && self.star >= 3,
            owner: 0,
        });
    }

    /// Place a power-up without a carrier. Used by tests.
    pub fn force_drop(&mut self, kind: PowerKind, x: f32, y: f32) {
        self.drops.push(Drop {
            kind,
            x,
            y,
            age: 0.0,
        });
    }

    pub fn set_tile(&mut self, x: i32, y: i32, tile: &str) {
        self.level.set_at(x, y, tile);
    }

    pub fn player_cell(&self) -> (i32, i32) {
        self.level.cell_of(self.player.x, self.player.y)
    }

    pub fn star(&self) -> u8 {
        self.star
    }

    pub fn thaw_all(&mut self) {
        self.clock_in = 0.0;
    }

    /// Stop the wave timer so a test can drive one system at a time.
    pub fn freeze_spawns(&mut self) {
        self.spawned = self.order.len() as u32;
        self.spawn_in = f32::MAX;
    }

    pub fn clear_shield(&mut self) {
        self.shield_in = 0.0;
    }
}

fn overlaps(ax: f32, ay: f32, asize: f32, bx: f32, by: f32, bsize: f32) -> bool {
    ax < bx + bsize && bx < ax + asize && ay < by + bsize && by < ay + asize
}

/// Tanks steer on tile lanes, so a turn snaps the cross axis to the nearest
/// tile boundary before the move is tried.
fn lane_snap(x: f32, y: f32, dir: Dir) -> (f32, f32) {
    if dir.is_horizontal() {
        (x, (y / TILE).round() * TILE)
    } else {
        ((x / TILE).round() * TILE, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_snap_aligns_the_cross_axis() {
        let (x, y) = lane_snap(37.0, 71.0, Dir::E);
        assert_eq!((x, y), (37.0, 64.0));
        let (x, y) = lane_snap(37.0, 71.0, Dir::S);
        assert_eq!((x, y), (32.0, 71.0));
    }

    #[test]
    fn rng_is_stable() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..16 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn player_shell_limit_follows_the_star_tier() {
        let level = crate::level::Level::from_json(
            r#"{"schema_version":3,"id":"t","name":"T","tile_size_px":32,
            "grid_w":13,"grid_h":13,
            "grid":["fffffffffffff","fffffffffffff","fffffffffffff","fffffffffffff",
                    "fffffffffffff","fffffffffffff","fffffffffffff","fffffffffffff",
                    "fffffffffffff","fffffffffffff","fffffffffffff","fffffffffffff",
                    "fffffffffffff"],
            "tile_semantics":[{"tile_id":"f","behaviour":"floor","blocks":false}],
            "placements":[{"entity_id":"hq_base","pos":[6,12]}],
            "spawn":[4,12],"exit":[4,12],"difficulty":1,"seed":1}"#,
            &Default::default(),
        )
        .unwrap();
        let mut sim = Sim::new(0, level);
        assert_eq!(sim.player_shell_limit(), 1);
        sim.star = 2;
        assert_eq!(sim.player_shell_limit(), 2);
    }
}
