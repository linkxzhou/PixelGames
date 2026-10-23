//! Fixed-step Battle City rules. One call to [`Sim::step`] is one 60 Hz tick.
//! Nothing in here reads the wall clock, so a replay is a pure function of
//! the level, the seed, and the input script.

use crate::level::{Level, GRID};

pub const TILE: i32 = 16;
pub const FIELD: i32 = GRID * TILE;
pub const STEP_COOLDOWN: u32 = 8;
pub const FIRE_COOLDOWN: u32 = 12;
pub const BULLET_SPEED: i32 = 8;
pub const ENEMY_STEP: u32 = 12;
pub const ENEMY_FIRE: u32 = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    N = 0,
    E = 1,
    S = 2,
    W = 3,
}

impl Dir {
    pub fn parse(s: &str) -> Self {
        match s {
            "E" | "e" | "right" => Dir::E,
            "S" | "s" | "down" => Dir::S,
            "W" | "w" | "left" => Dir::W,
            _ => Dir::N,
        }
    }

    pub fn delta(self) -> (i32, i32) {
        match self {
            Dir::N => (0, -1),
            Dir::E => (1, 0),
            Dir::S => (0, 1),
            Dir::W => (-1, 0),
        }
    }

    pub fn clockwise(self) -> Self {
        match self {
            Dir::N => Dir::E,
            Dir::E => Dir::S,
            Dir::S => Dir::W,
            Dir::W => Dir::N,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub dir: Option<Dir>,
    pub shoot: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Playing,
    Win,
    Lose,
}

#[derive(Clone, Debug)]
pub struct Tank {
    pub x: i32,
    pub y: i32,
    pub dir: Dir,
    pub alive: bool,
    pub player: bool,
    pub cooldown: u32,
    pub fire_cooldown: u32,
    pub steps: u32,
}

#[derive(Clone, Debug)]
pub struct Bullet {
    pub px: i32,
    pub py: i32,
    pub dir: Dir,
    pub from_player: bool,
    pub alive: bool,
}

#[derive(Clone, Debug)]
pub struct Boom {
    pub x: i32,
    pub y: i32,
    pub age: u32,
}

#[derive(Clone, Debug)]
pub struct Sim {
    pub level_index: u32,
    pub level: Level,
    pub player: Tank,
    pub enemies: Vec<Tank>,
    pub bullets: Vec<Bullet>,
    pub booms: Vec<Boom>,
    pub tick: u32,
    pub shots: u32,
    pub bricks_destroyed: u32,
    pub enemies_destroyed: u32,
    pub player_moved: bool,
    pub hq_alive: bool,
    pub outcome: Outcome,
    pub player_start: (i32, i32),
}

impl Sim {
    pub fn new(level_index: u32, level: Level) -> Self {
        let start = level.spawn;
        let enemies = level
            .enemies
            .iter()
            .map(|(x, y, dir)| Tank {
                x: *x,
                y: *y,
                dir: *dir,
                alive: true,
                player: false,
                cooldown: 0,
                fire_cooldown: 0,
                steps: 0,
            })
            .collect();
        Self {
            level_index,
            player: Tank {
                x: start.0,
                y: start.1,
                dir: Dir::N,
                alive: true,
                player: true,
                cooldown: 0,
                fire_cooldown: 0,
                steps: 0,
            },
            enemies,
            bullets: Vec::new(),
            booms: Vec::new(),
            tick: 0,
            shots: 0,
            bricks_destroyed: 0,
            enemies_destroyed: 0,
            player_moved: false,
            hq_alive: level.hq.is_some() || true,
            outcome: Outcome::Playing,
            player_start: start,
            level,
        }
    }

    pub fn step(&mut self, input: Input) {
        if self.outcome == Outcome::Playing {
            self.step_player(input);
            self.step_enemies();
            self.step_bullets();
            self.resolve_overlap();
        }
        for boom in &mut self.booms {
            boom.age = boom.age.saturating_add(1);
        }
        self.booms.retain(|b| b.age < 8);
        self.tick = self.tick.saturating_add(1);
    }

    fn step_player(&mut self, input: Input) {
        if !self.player.alive {
            return;
        }
        if let Some(dir) = input.dir {
            self.player.dir = dir;
            if self.player.cooldown == 0 {
                let (dx, dy) = dir.delta();
                let nx = self.player.x + dx;
                let ny = self.player.y + dy;
                if !self.level.blocks_tank(nx, ny) && !self.occupied(nx, ny, true) {
                    self.player.x = nx;
                    self.player.y = ny;
                    self.player.steps = self.player.steps.saturating_add(1);
                    self.player_moved = true;
                    self.player.cooldown = STEP_COOLDOWN;
                    self.after_player_step();
                }
            }
        }
        if self.player.cooldown > 0 {
            self.player.cooldown -= 1;
        }
        if self.player.fire_cooldown > 0 {
            self.player.fire_cooldown -= 1;
        }
        if input.shoot && self.player.fire_cooldown == 0 && self.outcome == Outcome::Playing {
            self.spawn_bullet(&self.player.clone(), true);
        }
    }

    fn after_player_step(&mut self) {
        let tile = self.level.tile(self.player.x, self.player.y);
        if tile == 'h' {
            self.player.alive = false;
            self.boom(self.player.x, self.player.y);
            self.outcome = Outcome::Lose;
            return;
        }
        if tile == 'g' || (self.player.x, self.player.y) == self.level.exit {
            self.outcome = Outcome::Win;
        }
    }

    fn step_enemies(&mut self) {
        if self.tick % ENEMY_STEP != 0 && self.tick % ENEMY_FIRE != 0 {
            return;
        }
        let fire = self.tick % ENEMY_FIRE == 0;
        let walk = self.tick % ENEMY_STEP == 0;
        for i in 0..self.enemies.len() {
            if !self.enemies[i].alive {
                continue;
            }
            if walk {
                let (dx, dy) = self.enemies[i].dir.delta();
                let nx = self.enemies[i].x + dx;
                let ny = self.enemies[i].y + dy;
                let blocked = self.level.blocks_tank(nx, ny)
                    || self.occupied(nx, ny, false)
                    || (nx, ny) == (self.player.x, self.player.y);
                if blocked {
                    self.enemies[i].dir = self.enemies[i].dir.clockwise();
                } else {
                    self.enemies[i].x = nx;
                    self.enemies[i].y = ny;
                    self.enemies[i].steps = self.enemies[i].steps.saturating_add(1);
                }
            }
            if fire {
                let tank = self.enemies[i].clone();
                self.spawn_bullet(&tank, false);
            }
        }
    }

    fn occupied(&self, x: i32, y: i32, ignore_player: bool) -> bool {
        if !ignore_player && self.player.alive && self.player.x == x && self.player.y == y {
            return true;
        }
        self.enemies.iter().any(|e| e.alive && e.x == x && e.y == y)
    }

    fn spawn_bullet(&mut self, tank: &Tank, from_player: bool) {
        if !tank.alive {
            return;
        }
        if self.bullets.iter().any(|b| b.alive && b.from_player == from_player && same_owner(b, tank)) {
            return;
        }
        if from_player {
            self.player.fire_cooldown = FIRE_COOLDOWN;
            self.shots = self.shots.saturating_add(1);
        }
        let (dx, dy) = tank.dir.delta();
        let nx = tank.x + dx;
        let ny = tank.y + dy;
        let mut bullet = Bullet {
            px: nx * TILE + TILE / 2,
            py: ny * TILE + TILE / 2,
            dir: tank.dir,
            from_player,
            alive: true,
        };
        if !self.level.in_bounds(nx, ny) {
            return;
        }
        self.resolve_bullet_tile(&mut bullet, nx, ny);
        if bullet.alive {
            self.bullets.push(bullet);
        }
    }

    fn step_bullets(&mut self) {
        for bullet in &mut self.bullets {
            if !bullet.alive {
                continue;
            }
            let (dx, dy) = bullet.dir.delta();
            bullet.px += dx * BULLET_SPEED;
            bullet.py += dy * BULLET_SPEED;
        }
        let snapshot: Vec<Bullet> = self.bullets.clone();
        for (i, bullet) in snapshot.iter().enumerate() {
            if !bullet.alive {
                continue;
            }
            let tx = bullet.px.div_euclid(TILE);
            let ty = bullet.py.div_euclid(TILE);
            let mut current = bullet.clone();
            self.resolve_bullet_tile(&mut current, tx, ty);
            self.bullets[i] = current;
        }
        self.bullets.retain(|b| b.alive);
    }

    fn resolve_bullet_tile(&mut self, bullet: &mut Bullet, tx: i32, ty: i32) {
        if !bullet.alive {
            return;
        }
        if !self.level.in_bounds(tx, ty) {
            bullet.alive = false;
            return;
        }
        if let Some((hx, hy)) = self.level.hq {
            if (tx, ty) == (hx, hy) && !bullet.from_player {
                bullet.alive = false;
                self.hq_alive = false;
                self.boom(tx, ty);
                self.outcome = Outcome::Lose;
                return;
            }
        }
        let tile = self.level.tile(tx, ty);
        if tile == 'b' {
            self.level.set_tile(tx, ty, 'f');
            self.bricks_destroyed = self.bricks_destroyed.saturating_add(1);
            self.boom(tx, ty);
            bullet.alive = false;
            return;
        }
        if tile == 'x' {
            self.boom(tx, ty);
            bullet.alive = false;
            return;
        }
        if bullet.from_player {
            if let Some(enemy) = self.enemies.iter_mut().find(|e| e.alive && e.x == tx && e.y == ty) {
                enemy.alive = false;
                self.enemies_destroyed = self.enemies_destroyed.saturating_add(1);
                self.boom(tx, ty);
                bullet.alive = false;
            }
        } else if self.player.alive && self.player.x == tx && self.player.y == ty {
            self.player.alive = false;
            self.boom(tx, ty);
            bullet.alive = false;
            self.outcome = Outcome::Lose;
        }
    }

    fn resolve_overlap(&mut self) {
        if self.outcome != Outcome::Playing || !self.player.alive {
            return;
        }
        if self.enemies.iter().any(|e| e.alive && e.x == self.player.x && e.y == self.player.y) {
            self.player.alive = false;
            self.boom(self.player.x, self.player.y);
            self.outcome = Outcome::Lose;
        }
    }

    fn boom(&mut self, x: i32, y: i32) {
        self.booms.push(Boom { x, y, age: 0 });
    }

    pub fn state_hash(&self) -> String {
        let mut text = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            self.tick,
            self.player.x,
            self.player.y,
            self.player.dir as u8,
            self.player.alive as u8,
            self.bricks_destroyed,
            self.enemies_destroyed,
            self.outcome as u8
        );
        for row in &self.level.grid {
            text.push('|');
            text.extend(row.iter());
        }
        for b in &self.bullets {
            text.push_str(&format!(";{}:{}:{}", b.px, b.py, b.dir as u8));
        }
        format!("{:016x}", fnv64(text.as_bytes()))
    }
}

fn same_owner(bullet: &Bullet, tank: &Tank) -> bool {
    // One live bullet per faction is enough for the slice. The tank argument
    // keeps the call site honest if a later change tracks per-tank shots.
    let _ = tank;
    bullet.alive
}

fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_level_two_moves_shoots_and_breaks_a_brick() {
        let level = Level::bundled(2).unwrap();
        assert_eq!(level.tile(1, 9), 'b');
        let mut sim = Sim::new(2, level);
        let script = [
            (24, Some(Dir::E), false),
            (48, Some(Dir::W), false),
            (120, Some(Dir::N), true),
        ];
        for tick in 0..120 {
            let mut input = Input::default();
            for (until, dir, shoot) in script {
                if tick < until {
                    input.dir = dir;
                    input.shoot = shoot;
                    break;
                }
            }
            sim.step(input);
        }
        assert!(sim.player_moved, "player never left the spawn tile");
        assert!(sim.shots >= 1, "no shot");
        assert!(sim.bricks_destroyed >= 1, "brick survived");
        assert_eq!(sim.level.tile(1, 9), 'f');
        assert_ne!((sim.player.x, sim.player.y), sim.player_start);
    }

    #[test]
    fn two_replays_share_a_hash() {
        let run = || {
            let mut sim = Sim::new(2, Level::bundled(2).unwrap());
            for _ in 0..30 {
                sim.step(Input { dir: Some(Dir::E), shoot: false });
            }
            sim.state_hash()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn player_bullet_destroys_an_enemy() {
        let mut level = Level::bundled(1).unwrap();
        level.grid.iter_mut().for_each(|row| row.fill('f'));
        level.enemies.push((3, 5, Dir::W));
        level.blocks_tank(0, 0);
        let mut sim = Sim::new(1, level);
        sim.player.x = 3;
        sim.player.y = 7;
        sim.player.dir = Dir::N;
        sim.enemies.push(Tank {
            x: 3,
            y: 5,
            dir: Dir::S,
            alive: true,
            player: false,
            cooldown: 0,
            fire_cooldown: 0,
            steps: 0,
        });
        sim.step(Input { dir: None, shoot: true });
        assert!(sim.bullets.iter().any(|b| b.alive) || sim.enemies_destroyed > 0);
        for _ in 0..8 {
            sim.step(Input::default());
        }
        assert!(sim.enemies_destroyed >= 1, "enemy still alive");
    }
}
