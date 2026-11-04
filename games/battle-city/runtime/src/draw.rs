//! Rendering. Reads a [`Sim`] and the loaded package, writes nothing back.
//!
//! Everything is drawn in a 480x416 logical space and multiplied by an integer
//! scale, so pixels stay square. Text uses a bundled or system CJK font when
//! one is available and falls back to macroquad's ASCII font with English
//! strings otherwise; the UI never draws glyphs itself.

use std::collections::BTreeMap;

use macroquad::prelude::*;

use crate::assets::{Frame, Package, Sheet};
use crate::sim::{Sim, BURST_LIFE, TILE};

pub const FIELD_W: f32 = crate::sim::FIELD_W;
pub const FIELD_H: f32 = crate::sim::FIELD_H;
pub const HUD_W: f32 = 64.0;
pub const LOGIC_W: f32 = FIELD_W + HUD_W;
pub const LOGIC_H: f32 = FIELD_H;

pub const HUD_BG: Color = Color::new(0.08, 0.09, 0.07, 1.0);
pub const HUD_DARK: Color = Color::new(0.12, 0.13, 0.11, 1.0);
pub const ACCENT: Color = Color::new(0.91, 0.64, 0.23, 1.0);
pub const PAPER: Color = Color::new(0.94, 0.90, 0.76, 1.0);
pub const ALERT: Color = Color::new(0.79, 0.28, 0.18, 1.0);
pub const HUD_MUTED: Color = Color::new(0.62, 0.65, 0.55, 1.0);

/// Which extra layer a caller wants on top of the battlefield.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    None,
    Paused,
    StageClear,
    GameOver,
    Victory,
}

/// Integer scale plus centring offset for one window size.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub scale: f32,
    pub ox: f32,
    pub oy: f32,
}

impl Viewport {
    pub fn new(screen_w: f32, screen_h: f32) -> Self {
        let scale = (screen_w / LOGIC_W)
            .min(screen_h / LOGIC_H)
            .floor()
            .max(1.0);
        let ox = ((screen_w - LOGIC_W * scale) * 0.5).floor();
        let oy = ((screen_h - LOGIC_H * scale) * 0.5).floor();
        Self { scale, ox, oy }
    }

    pub fn x(&self, x: f32) -> f32 {
        self.ox + x * self.scale
    }

    pub fn y(&self, y: f32) -> f32 {
        self.oy + y * self.scale
    }
}

/// One GPU texture per sheet, uploaded once at startup.
pub struct Renderer {
    textures: BTreeMap<String, Texture2D>,
    font: Option<Font>,
    zh: bool,
}

impl Renderer {
    pub fn new(package: &Package, font: Option<Font>, zh: bool) -> Self {
        let mut textures = BTreeMap::new();
        for (id, sheet) in &package.sheets {
            let image = Image {
                width: sheet.w as u16,
                height: sheet.h as u16,
                bytes: sheet.rgba.clone(),
            };
            let texture = Texture2D::from_image(&image);
            texture.set_filter(FilterMode::Nearest);
            textures.insert(id.clone(), texture);
        }
        Self { textures, font, zh }
    }

    pub fn has_cjk(&self) -> bool {
        self.zh
    }

    fn t<'a>(&self, zh: &'a str, en: &'a str) -> &'a str {
        if self.zh {
            zh
        } else {
            en
        }
    }

    fn frame(&self, sheet: &str, frame: Frame, vp: &Viewport, x: f32, y: f32, tint: Color) {
        let Some(texture) = self.textures.get(sheet) else {
            return;
        };
        draw_texture_ex(
            texture,
            vp.x(x),
            vp.y(y),
            tint,
            DrawTextureParams {
                dest_size: Some(vec2(frame.w as f32 * vp.scale, frame.h as f32 * vp.scale)),
                source: Some(Rect::new(
                    frame.x as f32,
                    frame.y as f32,
                    frame.w as f32,
                    frame.h as f32,
                )),
                ..Default::default()
            },
        );
    }

    /// Draw a tile or sprite by asset id, choosing an animation frame by time.
    fn anim_at(
        &self,
        package: &Package,
        asset: &str,
        anim: &str,
        dir: &str,
        fps: f32,
        time: f32,
        vp: &Viewport,
        x: f32,
        y: f32,
        tint: Color,
    ) {
        let Some(sheet) = package.sheet_for(asset) else {
            return;
        };
        let Some(frame) = pick_frame(sheet, asset, anim, dir, fps, time) else {
            return;
        };
        self.frame(sheet.id.as_str(), frame, vp, x, y, tint);
    }

    fn anim(
        &self,
        package: &Package,
        asset: &str,
        anim: &str,
        dir: &str,
        fps: f32,
        time: f32,
        vp: &Viewport,
        x: f32,
        y: f32,
    ) {
        self.anim_at(package, asset, anim, dir, fps, time, vp, x, y, WHITE);
    }

    /// Text in logical coordinates; `y` is the top of the line.
    fn text(&self, vp: &Viewport, s: &str, x: f32, y: f32, size: f32, color: Color) {
        let px = (size * vp.scale).max(6.0).round();
        draw_text_ex(
            s,
            vp.x(x),
            vp.y(y) + px * 0.82,
            TextParams {
                font: self.font.as_ref(),
                font_size: px as u16,
                color,
                ..Default::default()
            },
        );
    }

    fn text_width(&self, vp: &Viewport, s: &str, size: f32) -> f32 {
        let px = (size * vp.scale).max(6.0).round();
        measure_text(s, self.font.as_ref(), px as u16, 1.0).width / vp.scale
    }

    fn text_centered(&self, vp: &Viewport, s: &str, cx: f32, y: f32, size: f32, color: Color) {
        let w = self.text_width(vp, s, size);
        self.text(vp, s, cx - w * 0.5, y, size, color);
    }
}
impl Renderer {
    /// Battlefield: terrain, tanks, shells, grass cover, bursts.
    pub fn draw_world(&self, package: &Package, sim: &Sim, vp: &Viewport) {
        let shake = sim.shake;
        let jitter = if shake > 0.0 {
            vec2(
                ((sim.time * 61.0).sin() * 3.0 * shake).round(),
                ((sim.time * 47.0).cos() * 3.0 * shake).round(),
            )
        } else {
            vec2(0.0, 0.0)
        };
        let at = |x: f32, y: f32| (x + jitter.x, y + jitter.y);

        // Terrain below tanks.
        for cy in 0..sim.level.h {
            for cx in 0..sim.level.w {
                let tile = sim.level.at(cx, cy);
                if tile == "f" || tile == "g" {
                    continue;
                }
                let (x, y) = at(cx as f32 * TILE, cy as f32 * TILE);
                if tile == "w" {
                    self.anim(package, "w", "idle", "N", 4.0, sim.time, vp, x, y);
                } else if tile == "b" && sim.level.is_cracked_at(cx, cy) {
                    self.anim(package, "brick_crack", "idle", "N", 2.0, sim.time, vp, x, y);
                } else {
                    self.anim(package, tile, "idle", "N", 2.0, sim.time, vp, x, y);
                }
            }
        }

        // Base.
        let (bx, by) = at(sim.base_cell.0 as f32 * TILE, sim.base_cell.1 as f32 * TILE);
        if sim.base_alive {
            self.anim(package, "hq_base", "idle", "N", 2.0, sim.time, vp, bx, by);
        } else {
            self.anim(
                package,
                "hq_base",
                "destroyed",
                "N",
                4.0,
                sim.time,
                vp,
                bx,
                by,
            );
        }

        // Spawn flashes announce a tank one beat before it exists.
        for flash in &sim.spawn_flashes {
            let (x, y) = at(flash.cell.0 as f32 * TILE, flash.cell.1 as f32 * TILE);
            self.anim(
                package,
                "spawn_flash",
                "spawn",
                "N",
                12.0,
                sim.time,
                vp,
                x,
                y,
            );
        }

        // Power-ups sit between terrain and tanks, like the arcade drop.
        for drop in &sim.drops {
            let (x, y) = at(drop.x + 8.0, drop.y + 8.0);
            self.anim(
                package,
                drop.kind.asset(),
                "idle",
                "N",
                2.0,
                sim.time,
                vp,
                x,
                y,
            );
        }

        // Player.
        if sim.player_alive {
            let anim = if sim.player.moving { "move" } else { "idle" };
            let fps = if sim.player.moving { 8.0 } else { 2.0 };
            let (x, y) = at(sim.player.x, sim.player.y);
            self.anim(
                package,
                "player_tank",
                anim,
                sim.player.dir.name(),
                fps,
                sim.time,
                vp,
                x,
                y,
            );
            if sim.shield_in > 0.0 {
                self.anim(package, "shield", "shield", "N", 8.0, sim.time, vp, x, y);
            }
        }

        // Enemies.
        for enemy in &sim.enemies {
            let anim = if enemy.tank.moving { "move" } else { "idle" };
            let fps = if enemy.tank.moving { 8.0 } else { 2.0 };
            let (x, y) = at(enemy.tank.x, enemy.tank.y);
            let tint = if enemy.flash_in > 0.0 {
                Color::new(1.8, 1.8, 1.8, 1.0)
            } else if enemy.carrier && (sim.ticks / 8) % 2 == 0 {
                Color::new(1.0, 0.65, 0.35, 1.0)
            } else {
                WHITE
            };
            self.anim_at(
                package,
                enemy.kind.asset(),
                anim,
                enemy.tank.dir.name(),
                fps,
                sim.time,
                vp,
                x,
                y,
                tint,
            );
        }

        // Shells.
        for bullet in &sim.bullets {
            let (x, y) = at(bullet.x, bullet.y);
            self.anim(
                package,
                "bullet",
                "fly",
                bullet.dir.name(),
                12.0,
                sim.time,
                vp,
                x,
                y,
            );
        }

        // Grass hides tanks, exactly like the arcade original.
        for cy in 0..sim.level.h {
            for cx in 0..sim.level.w {
                if sim.level.is_grass_at(cx, cy) {
                    let (x, y) = at(cx as f32 * TILE, cy as f32 * TILE);
                    self.anim(package, "g", "idle", "N", 2.0, sim.time, vp, x, y);
                }
            }
        }

        // Bursts: five frames spread over their lifetime.
        for burst in &sim.bursts {
            let frames = package
                .sheet_for("explosion")
                .map(|sheet| sheet.anim_frames("explosion", "burst", "N"))
                .unwrap_or_default();
            if frames.is_empty() {
                continue;
            }
            let t = (burst.age / BURST_LIFE).clamp(0.0, 0.999);
            let index = (t * frames.len() as f32) as usize;
            let frame = frames[index.min(frames.len() - 1)];
            let size = frame.w as f32;
            let (x, y) = at(burst.x + (TILE - size) * 0.5, burst.y + (TILE - size) * 0.5);
            let sheet = package
                .sheet_for("explosion")
                .map(|s| s.id.clone())
                .unwrap_or_default();
            self.frame(sheet.as_str(), frame, vp, x, y, WHITE);
        }
    }
}
impl Renderer {
    fn icon(&self, package: &Package, asset: &str, vp: &Viewport, x: f32, y: f32, tint: Color) {
        let Some(sheet) = package.sheet_for(asset) else {
            return;
        };
        let Some(frame) = sheet.first_frame(asset, "idle", "N") else {
            return;
        };
        self.frame(sheet.id.as_str(), frame, vp, x, y, tint);
    }

    /// HUD sidebar: enemy counter, stage, lives, score and running effects.
    pub fn draw_hud(&self, package: &Package, sim: &Sim, vp: &Viewport, high_score: u32) {
        draw_rectangle(
            vp.x(FIELD_W),
            vp.y(0.0),
            HUD_W * vp.scale,
            LOGIC_H * vp.scale,
            HUD_BG,
        );
        draw_rectangle(
            vp.x(FIELD_W),
            vp.y(0.0),
            vp.scale,
            LOGIC_H * vp.scale,
            Color::new(0.28, 0.31, 0.24, 1.0),
        );
        let x0 = FIELD_W + 6.0;

        self.text(vp, self.t("敌军", "ENEMY"), x0, 4.0, 9.0, HUD_MUTED);
        for slot in 0..sim.remaining.min(20) {
            let col = slot % 6;
            let row = slot / 6;
            self.icon(
                package,
                "hud_enemy_icon",
                vp,
                x0 + col as f32 * 9.0,
                15.0 + row as f32 * 9.0,
                WHITE,
            );
        }

        self.text(vp, self.t("关卡", "STAGE"), x0, 62.0, 9.0, HUD_MUTED);
        self.text(vp, &format!("{:02}", sim.stage + 1), x0, 73.0, 14.0, ACCENT);

        self.text(vp, self.t("生命", "LIVES"), x0, 98.0, 9.0, HUD_MUTED);
        for index in 0..sim.lives.min(5) {
            self.icon(
                package,
                "hud_life_icon",
                vp,
                x0 + index as f32 * 9.0,
                109.0,
                WHITE,
            );
        }
        if sim.lives > 5 {
            self.text(
                vp,
                &format!("+{}", sim.lives - 5),
                x0 + 46.0,
                109.0,
                8.0,
                PAPER,
            );
        }

        self.text(vp, self.t("得分", "SCORE"), x0, 136.0, 9.0, HUD_MUTED);
        self.text(vp, &format!("{}", sim.score), x0, 147.0, 10.0, PAPER);
        self.text(vp, self.t("最高", "HIGH"), x0, 170.0, 9.0, HUD_MUTED);
        self.text(
            vp,
            &format!("{}", high_score.max(sim.score)),
            x0,
            181.0,
            10.0,
            PAPER,
        );

        // Timed power-ups.
        let mut line = 206.0;
        if sim.shovel_in > 0.0 {
            self.icon(package, "power_shovel", vp, x0, line - 6.0, WHITE);
            self.text(
                vp,
                &format!("{:.0}", sim.shovel_in),
                x0 + 20.0,
                line,
                10.0,
                PAPER,
            );
            line += 16.0;
        }
        if sim.clock_in > 0.0 {
            self.icon(package, "power_clock", vp, x0, line - 6.0, WHITE);
            self.text(
                vp,
                &format!("{:.0}", sim.clock_in),
                x0 + 20.0,
                line,
                10.0,
                PAPER,
            );
            line += 16.0;
        }
        if sim.shield_in > 0.0 {
            self.icon(package, "power_helmet", vp, x0, line - 6.0, WHITE);
            self.text(
                vp,
                &format!("{:.0}", sim.shield_in),
                x0 + 20.0,
                line,
                10.0,
                PAPER,
            );
        }

        // Controls hint at the bottom.
        self.text(
            vp,
            self.t("方向键 移动", "ARROWS MOVE"),
            x0,
            318.0,
            8.0,
            HUD_MUTED,
        );
        self.text(
            vp,
            self.t("空格 开火", "SPACE FIRE"),
            x0,
            332.0,
            8.0,
            HUD_MUTED,
        );
        self.text(vp, self.t("P 暂停", "P PAUSE"), x0, 346.0, 8.0, HUD_MUTED);
    }
}
impl Renderer {
    /// Title screen: scrolling brick wall behind the logo.
    pub fn draw_title(
        &self,
        package: &Package,
        vp: &Viewport,
        time: f32,
        high_score: u32,
        best_stage: u32,
    ) {
        let offset = (time * 10.0) % TILE;
        let mut y = -offset;
        while y < LOGIC_H {
            let mut x = 0.0;
            while x < LOGIC_W {
                self.icon(package, "b", vp, x, y, WHITE);
                x += TILE;
            }
            y += TILE;
        }
        draw_rectangle(
            vp.x(0.0),
            vp.y(0.0),
            LOGIC_W * vp.scale,
            LOGIC_H * vp.scale,
            Color::new(0.0, 0.0, 0.0, 0.62),
        );
        self.text_centered(
            vp,
            self.t("坦克大战", "BATTLE CITY"),
            LOGIC_W * 0.5,
            88.0,
            26.0,
            ACCENT,
        );
        self.text_centered(
            vp,
            self.t("守卫鹰标 · 35 关完整版", "DEFEND THE EAGLE - 35 STAGES"),
            LOGIC_W * 0.5,
            130.0,
            11.0,
            PAPER,
        );
        if (time * 2.0) as u32 % 2 == 0 {
            self.text_centered(
                vp,
                self.t("按 Enter 开始", "PRESS ENTER"),
                LOGIC_W * 0.5,
                200.0,
                13.0,
                PAPER,
            );
        }
        self.text_centered(
            vp,
            &format!(
                "{} {}  {} {}",
                self.t("最高分", "HIGH"),
                high_score,
                self.t("最高关", "BEST"),
                best_stage
            ),
            LOGIC_W * 0.5,
            238.0,
            10.0,
            HUD_MUTED,
        );
        self.text_centered(
            vp,
            self.t("R：从第 1 关重新开始", "R: RESTART FROM STAGE 1"),
            LOGIC_W * 0.5,
            286.0,
            9.0,
            HUD_MUTED,
        );
        self.text_centered(
            vp,
            self.t(
                "方向键/WASD 移动 · 空格/J 开火 · P 暂停",
                "ARROWS/WASD MOVE - SPACE/J FIRE - P PAUSE",
            ),
            LOGIC_W * 0.5,
            322.0,
            9.0,
            PAPER,
        );
        self.text_centered(
            vp,
            self.t("原创像素 · pxl 管线生成", "ORIGINAL PIXELS - BUILT BY PXL"),
            LOGIC_W * 0.5,
            360.0,
            8.0,
            HUD_MUTED,
        );
    }

    /// Stage banner shown before control is handed over.
    pub fn draw_stage_intro(&self, package: &Package, sim: &Sim, vp: &Viewport, hold: f32) {
        self.draw_world(package, sim, vp);
        self.draw_hud(package, sim, vp, sim.high_score);
        draw_rectangle(
            vp.x(0.0),
            vp.y(0.0),
            FIELD_W * vp.scale,
            FIELD_H * vp.scale,
            Color::new(0.0, 0.0, 0.0, 0.72),
        );
        let title = if self.zh {
            format!("第 {} 关", sim.stage + 1)
        } else {
            format!("STAGE {}", sim.stage + 1)
        };
        self.text_centered(vp, &title, FIELD_W * 0.5, 140.0, 22.0, ACCENT);
        let theme = sim
            .level
            .name
            .rsplit(" · ")
            .next()
            .unwrap_or(&sim.level.name);
        self.text_centered(vp, theme, FIELD_W * 0.5, 182.0, 13.0, PAPER);
        let objective = if self.zh {
            sim.level.objective.as_str()
        } else {
            "CLEAR 20 ENEMIES AND PROTECT THE HQ"
        };
        self.text_centered(vp, objective, FIELD_W * 0.5, 214.0, 10.0, PAPER);
        if (hold * 2.0) as u32 % 2 == 0 {
            self.text_centered(
                vp,
                self.t("准备", "READY"),
                FIELD_W * 0.5,
                262.0,
                14.0,
                PAPER,
            );
        }
    }

    /// Modal overlay drawn on top of the battlefield.
    pub fn draw_overlay(
        &self,
        vp: &Viewport,
        overlay: &Overlay,
        sim: &Sim,
        high_score: u32,
        time: f32,
    ) {
        let (title, lines): (&str, Vec<String>) = match overlay {
            Overlay::None => return,
            Overlay::Paused => (
                self.t("暂停", "PAUSED"),
                vec![self.t("按 P 继续", "PRESS P TO RESUME").to_string()],
            ),
            Overlay::StageClear => (
                self.t("本关完成", "STAGE CLEAR"),
                vec![
                    format!("{} {}", self.t("得分", "SCORE"), sim.score),
                    self.t("即将进入下一关", "NEXT STAGE IN A MOMENT")
                        .to_string(),
                ],
            ),
            Overlay::GameOver => (
                self.t("游戏结束", "GAME OVER"),
                vec![
                    format!("{} {}", self.t("得分", "SCORE"), sim.score),
                    format!("{} {}", self.t("最高", "HIGH"), high_score.max(sim.score)),
                    self.t("Enter 重试 · Esc 退出", "ENTER RETRY - ESC QUIT")
                        .to_string(),
                ],
            ),
            Overlay::Victory => (
                self.t("全部通关", "VICTORY"),
                vec![
                    format!("{} {}", self.t("得分", "SCORE"), sim.score),
                    self.t("35 关全部清空，鹰标仍在", "ALL 35 STAGES CLEARED")
                        .to_string(),
                    self.t("Enter 返回标题", "ENTER FOR TITLE").to_string(),
                ],
            ),
        };
        draw_rectangle(
            vp.x(0.0),
            vp.y(0.0),
            LOGIC_W * vp.scale,
            LOGIC_H * vp.scale,
            Color::new(0.0, 0.0, 0.0, 0.66),
        );
        let accent = if matches!(overlay, Overlay::GameOver | Overlay::Paused) {
            ALERT
        } else {
            ACCENT
        };
        self.text_centered(vp, title, LOGIC_W * 0.5, 150.0, 22.0, accent);
        let mut y = 196.0;
        for line in &lines {
            self.text_centered(vp, line, LOGIC_W * 0.5, y, 11.0, PAPER);
            y += 20.0;
        }
        if matches!(overlay, Overlay::StageClear) {
            let dots = ".".repeat(((time * 3.0) as usize) % 4);
            self.text_centered(vp, &dots, LOGIC_W * 0.5, y + 8.0, 12.0, PAPER);
        }
    }
}

fn pick_frame(
    sheet: &Sheet,
    sprite: &str,
    anim: &str,
    dir: &str,
    fps: f32,
    time: f32,
) -> Option<Frame> {
    let frames = sheet.anim_frames(sprite, anim, dir);
    if frames.is_empty() {
        return None;
    }
    let index = ((time * fps) as usize) % frames.len();
    Some(frames[index])
}
