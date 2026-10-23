//! Indexed software framebuffer. The macroquad window and the headless PNG
//! path both blit this buffer. Scaling is nearest-neighbour only.

use crate::level::GRID;
use crate::palette::Palette;
use crate::sim::{Bullet, Dir, Outcome, Sim, Tank, TILE, FIELD};

pub const LOGIC_W: u32 = 320;
pub const LOGIC_H: u32 = 240;
pub const ORIGIN_X: i32 = ((LOGIC_W as i32) - FIELD) / 2;
pub const ORIGIN_Y: i32 = 32;

const CLEAR: u8 = 0;
const OUTLINE: u8 = 1;
const FLOOR_A: u8 = 2;
const FLOOR_B: u8 = 3;
const OLIVE: u8 = 4;
const OLIVE_HI: u8 = 5;
const TREAD: u8 = 6;
const TREAD_HI: u8 = 7;
const WATER_A: u8 = 8;
const WATER_B: u8 = 9;
const BRICK: u8 = 10;
const BRICK_HI: u8 = 11;
const BONE: u8 = 12;
const GOAL: u8 = 13;
const FIRE: u8 = 14;
const SPARK: u8 = 15;

pub struct Frame {
    pub index: Vec<u8>,
}

impl Frame {
    pub fn new() -> Self {
        Self { index: vec![FLOOR_A; (LOGIC_W * LOGIC_H) as usize] }
    }

    pub fn to_rgba(&self, palette: &Palette) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.index.len() * 4);
        for &i in &self.index {
            let c = palette.rgba[i as usize];
            out.extend_from_slice(&c);
        }
        out
    }

    pub fn mismatches(&self, palette: &Palette) -> u32 {
        let rgba = self.to_rgba(palette);
        let mut bad = 0u32;
        for px in rgba.chunks_exact(4) {
            if !palette.contains_rgba([px[0], px[1], px[2], px[3]]) {
                bad += 1;
            }
        }
        bad
    }

    fn set(&mut self, x: i32, y: i32, color: u8) {
        if color == CLEAR || x < 0 || y < 0 || x >= LOGIC_W as i32 || y >= LOGIC_H as i32 {
            return;
        }
        self.index[(y as u32 * LOGIC_W + x as u32) as usize] = color;
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, color);
            }
        }
    }

    fn stroke_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8) {
        self.fill(x, y, w, 1, color);
        self.fill(x, y + h - 1, w, 1, color);
        self.fill(x, y, 1, h, color);
        self.fill(x + w - 1, y, 1, h, color);
    }
}

pub fn render(sim: &Sim, frame: &mut Frame) {
    frame.index.fill(FLOOR_A);
    frame.fill(0, 0, LOGIC_W as i32, ORIGIN_Y, OUTLINE);
    frame.fill(0, ORIGIN_Y + FIELD, LOGIC_W as i32, LOGIC_H as i32 - (ORIGIN_Y + FIELD), OUTLINE);
    for y in 0..GRID {
        for x in 0..GRID {
            paint_tile(frame, sim, x, y);
        }
    }
    if let Some((hx, hy)) = sim.level.hq {
        blit_hq(frame, ORIGIN_X + hx * TILE, ORIGIN_Y + hy * TILE, sim.hq_alive);
    }
    if sim.player.alive {
        blit_tank(frame, &sim.player, sim.tick, true);
    }
    for enemy in &sim.enemies {
        if enemy.alive {
            blit_tank(frame, enemy, sim.tick, false);
        }
    }
    for bullet in &sim.bullets {
        if bullet.alive {
            blit_bullet(frame, bullet);
        }
    }
    for boom in &sim.booms {
        blit_boom(frame, boom.x, boom.y, boom.age);
    }
    for y in 0..GRID {
        for x in 0..GRID {
            if sim.level.tile(x, y) == 't' {
                blit_grass(frame, ORIGIN_X + x * TILE, ORIGIN_Y + y * TILE, sim.tick);
            }
        }
    }
    frame.stroke_rect(ORIGIN_X - 1, ORIGIN_Y - 1, FIELD + 2, FIELD + 2, BONE);
    let stage = format!("STAGE {}", sim.level_index);
    draw_text(frame, 8, 8, &stage, BONE, 2);
    let status = status_line(sim);
    draw_text(frame, 8, 220, &status, BONE, 1);
    let mut icon_x = 250;
    if sim.player.alive {
        blit_mini_tank(frame, icon_x, 8);
        icon_x += 18;
    }
    let _ = icon_x;
}

fn status_line(sim: &Sim) -> String {
    match sim.outcome {
        Outcome::Win => "EXIT".into(),
        Outcome::Lose => "LOST".into(),
        Outcome::Playing => format!("BRICK {}", sim.bricks_destroyed),
    }
}

fn paint_tile(frame: &mut Frame, sim: &Sim, x: i32, y: i32) {
    let ox = ORIGIN_X + x * TILE;
    let oy = ORIGIN_Y + y * TILE;
    let tile = sim.level.tile(x, y);
    match tile {
        'b' => blit_brick(frame, ox, oy),
        'x' => blit_steel(frame, ox, oy),
        'w' => blit_water(frame, ox, oy, sim.tick),
        'h' => blit_hazard(frame, ox, oy),
        'g' => blit_goal(frame, ox, oy, sim.tick),
        't' => blit_floor(frame, ox, oy, x, y),
        _ => blit_floor(frame, ox, oy, x, y),
    }
}

fn blit_floor(frame: &mut Frame, x: i32, y: i32, tx: i32, ty: i32) {
    let color = if (tx + ty) % 2 == 0 { FLOOR_A } else { FLOOR_B };
    frame.fill(x, y, TILE, TILE, color);
}

fn blit_brick(frame: &mut Frame, x: i32, y: i32) {
    frame.fill(x, y, TILE, TILE, BRICK);
    for row in 0..4 {
        let yy = y + row * 4;
        frame.fill(x, yy + 3, TILE, 1, OUTLINE);
        let shift = if row % 2 == 0 { 0 } else { 8 };
        frame.fill(x + shift, yy, 1, 3, BRICK_HI);
        frame.fill(x + shift + 7, yy, 1, 3, OUTLINE);
    }
}

fn blit_steel(frame: &mut Frame, x: i32, y: i32) {
    frame.fill(x, y, TILE, TILE, TREAD);
    frame.stroke_rect(x, y, TILE, TILE, TREAD_HI);
    frame.fill(x + 2, y + 2, 2, 2, OUTLINE);
    frame.fill(x + 12, y + 2, 2, 2, OUTLINE);
    frame.fill(x + 2, y + 12, 2, 2, OUTLINE);
    frame.fill(x + 12, y + 12, 2, 2, OUTLINE);
    frame.fill(x + 7, y + 7, 2, 2, BONE);
}

fn blit_water(frame: &mut Frame, x: i32, y: i32, tick: u32) {
    let phase = (tick / 8) % 2;
    frame.fill(x, y, TILE, TILE, WATER_A);
    for i in 0..4 {
        let yy = y + 2 + i * 4 + phase as i32;
        frame.fill(x + 1, yy, 14, 1, WATER_B);
    }
}

fn blit_hazard(frame: &mut Frame, x: i32, y: i32) {
    frame.fill(x, y, TILE, TILE, FLOOR_A);
    for i in 0..TILE {
        frame.set(x + i, y + i, FIRE);
        frame.set(x + TILE - 1 - i, y + i, SPARK);
    }
    frame.stroke_rect(x + 2, y + 2, 12, 12, FIRE);
}

fn blit_goal(frame: &mut Frame, x: i32, y: i32, tick: u32) {
    let hi = if (tick / 8) % 2 == 0 { SPARK } else { BONE };
    frame.fill(x, y, TILE, TILE, GOAL);
    frame.fill(x + 7, y + 3, 2, 6, hi);
    frame.fill(x + 5, y + 7, 6, 2, hi);
    frame.fill(x + 4, y + 9, 8, 2, hi);
    frame.stroke_rect(x, y, TILE, TILE, OUTLINE);
}

fn blit_grass(frame: &mut Frame, x: i32, y: i32, tick: u32) {
    let sway = (tick / 10) % 2;
    for i in 0..5 {
        let xx = x + 1 + i * 3 + sway as i32;
        frame.fill(xx, y + 4, 1, 8, OLIVE);
        frame.set(xx, y + 3, OLIVE_HI);
        frame.set(xx + 1, y + 6, OLIVE_HI);
    }
}

fn blit_hq(frame: &mut Frame, x: i32, y: i32, alive: bool) {
    let body = if alive { BONE } else { FIRE };
    frame.fill(x + 1, y + 5, 14, 10, body);
    frame.fill(x + 4, y + 2, 8, 5, body);
    frame.fill(x + 6, y + 1, 4, 2, SPARK);
    frame.stroke_rect(x + 1, y + 5, 14, 10, OUTLINE);
    frame.fill(x + 4, y + 8, 2, 2, OUTLINE);
    frame.fill(x + 10, y + 8, 2, 2, OUTLINE);
    frame.fill(x + 7, y + 9, 2, 3, if alive { SPARK } else { OUTLINE });
}

fn blit_tank(frame: &mut Frame, tank: &Tank, tick: u32, player: bool) {
    let phase = ((tick / 4) + tank.steps) % 2;
    let ox = ORIGIN_X + tank.x * TILE;
    let oy = ORIGIN_Y + tank.y * TILE;
    for ly in 0..TILE {
        for lx in 0..TILE {
            let (sx, sy) = match tank.dir {
                Dir::N => (lx, ly),
                Dir::E => (ly, TILE - 1 - lx),
                Dir::S => (TILE - 1 - lx, TILE - 1 - ly),
                Dir::W => (TILE - 1 - ly, lx),
            };
            if let Some(color) = tank_pixel(sx, sy, phase, player) {
                frame.set(ox + lx, oy + ly, color);
            }
        }
    }
}

fn tank_pixel(x: i32, y: i32, phase: u32, player: bool) -> Option<u8> {
    let body = if player { OLIVE_HI } else { BRICK_HI };
    let turret = if player { BONE } else { SPARK };
    let cannon = if player { BONE } else { FIRE };
    if y == 0 || y == 15 || x == 0 || x == 15 {
        return None;
    }
    let tread = (1..=3).contains(&x) || (12..=14).contains(&x);
    if tread && (3..14).contains(&y) {
        let stripe = (y + phase as i32) % 2 == 0;
        return Some(if stripe { OUTLINE } else { TREAD_HI });
    }
    if (6..=9).contains(&x) && (1..=7).contains(&y) {
        if x == 6 || x == 9 || y == 1 {
            return Some(OUTLINE);
        }
        return Some(if y <= 3 { cannon } else { turret });
    }
    if (4..=11).contains(&x) && (5..=13).contains(&y) {
        if x == 4 || x == 11 || y == 5 || y == 13 {
            return Some(OUTLINE);
        }
        return Some(body);
    }
    None
}

fn blit_bullet(frame: &mut Frame, bullet: &Bullet) {
    let x = ORIGIN_X + bullet.px - 1;
    let y = ORIGIN_Y + bullet.py - 1;
    let color = if bullet.from_player { SPARK } else { FIRE };
    frame.fill(x, y, 3, 3, color);
    frame.set(x + 1, y + 1, BONE);
}

fn blit_boom(frame: &mut Frame, tx: i32, ty: i32, age: u32) {
    let radius = 2 + (age as i32) / 2;
    let cx = ORIGIN_X + tx * TILE + TILE / 2;
    let cy = ORIGIN_Y + ty * TILE + TILE / 2;
    let color = if age % 2 == 0 { SPARK } else { FIRE };
    for y in -radius..=radius {
        for x in -radius..=radius {
            if x * x + y * y <= radius * radius {
                frame.set(cx + x, cy + y, color);
            }
        }
    }
}

fn blit_mini_tank(frame: &mut Frame, x: i32, y: i32) {
    frame.fill(x, y + 2, 8, 6, OLIVE);
    frame.fill(x + 3, y, 2, 4, BONE);
    frame.stroke_rect(x, y + 2, 8, 6, OUTLINE);
}

fn draw_text(frame: &mut Frame, x: i32, y: i32, text: &str, color: u8, scale: i32) {
    let mut cx = x;
    for ch in text.chars() {
        let rows = glyph(ch);
        for (ry, bits) in rows.iter().enumerate() {
            for rx in 0..3 {
                if (bits >> (2 - rx)) & 1 == 1 {
                    frame.fill(cx + rx * scale, y + ry as i32 * scale, scale, scale, color);
                }
            }
        }
        cx += 4 * scale;
    }
}

fn glyph(ch: char) -> [u8; 5] {
    match ch {
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => [0b011, 0b100, 0b100, 0b100, 0b011],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'G' => [0b011, 0b100, 0b101, 0b101, 0b011],
        'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'K' => [0b101, 0b110, 0b100, 0b110, 0b101],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'R' => [0b110, 0b101, 0b110, 0b101, 0b101],
        'S' => [0b011, 0b100, 0b010, 0b001, 0b110],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        ' ' => [0, 0, 0, 0, 0],
        _ => [0b111, 0b101, 0b111, 0b101, 0b111],
    }
}
