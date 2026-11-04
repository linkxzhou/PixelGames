//! Battle City window. Fixed 1/60 s simulation steps, variable-rate rendering.

use std::path::{Path, PathBuf};

use macroquad::prelude::*;

use battle_city::assets::load_package;
use battle_city::draw::{Overlay, Renderer, Viewport, LOGIC_H, LOGIC_W};
use battle_city::save::{self, Save};
use battle_city::sim::{Input, Outcome, Sim};

const DT: f32 = 1.0 / 60.0;
const CLEAR_HOLD: f32 = 2.2;
const INTRO_HOLD: f32 = 1.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Title,
    Intro,
    Playing,
    Paused,
    StageClear,
    GameOver,
    Victory,
}

fn window_conf() -> Conf {
    Conf {
        window_title: "坦克大战 Battle City".to_owned(),
        window_width: (LOGIC_W * 2.0) as i32,
        window_height: (LOGIC_H * 2.0) as i32,
        window_resizable: true,
        high_dpi: true,
        ..Default::default()
    }
}

/// `--package <dir>`, else the first candidate that holds an asset-package.
fn package_dir() -> Result<PathBuf, String> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(at) = args.iter().position(|a| a == "--package") {
        let dir = args
            .get(at + 1)
            .ok_or_else(|| "--package needs a directory".to_string())?;
        return Ok(PathBuf::from(dir));
    }
    let candidates = [
        "../work/400-package",
        "work/400-package",
        "../../work/400-package",
        "../../../games/battle-city/work/400-package",
        "games/battle-city/work/400-package",
    ];
    for candidate in candidates {
        let dir = Path::new(candidate);
        if dir.join("asset-package.json").is_file() {
            return Ok(dir.to_path_buf());
        }
    }
    Err(format!(
        "no asset-package.json found; pass --package <400-package dir>. tried: {}",
        candidates.join(", ")
    ))
}

/// Optional CJK font: a bundled file first, then the macOS system font. The
/// game stays playable with English strings when neither is available.
fn load_ui_font() -> Option<Font> {
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts/game.ttf");
    let system = Path::new("/System/Library/Fonts/Supplemental/Arial Unicode.ttf");
    for path in [bundled, system.to_path_buf()] {
        if !path.is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        match load_ttf_font_from_bytes(&bytes) {
            Ok(font) => {
                eprintln!("battle-city: UI font {}", path.display());
                return Some(font);
            }
            Err(err) => eprintln!("battle-city: font {}: {err}", path.display()),
        }
    }
    eprintln!("battle-city: no CJK font, falling back to English UI");
    None
}

struct Game {
    renderer: Renderer,
    package: battle_city::Package,
    sim: Sim,
    screen: Screen,
    stage: usize,
    hold: f32,
    clock: f32,
    save: Save,
    save_path: PathBuf,
    high_score: u32,
    /// Lives, score and star tier carried between stages of one run.
    carry_lives: u8,
    carry_score: u32,
    carry_star: u8,
}

impl Game {
    fn new(package: battle_city::Package, font: Option<Font>, zh: bool) -> Self {
        let renderer = Renderer::new(&package, font, zh);
        let save_path = save::default_path(&package.root);
        let save = save::read(&save_path);
        let stage = (save.best_stage as usize)
            .saturating_sub(1)
            .min(package.levels.len() - 1);
        let mut sim = Sim::new(stage, package.levels[stage].clone());
        sim.high_score = save.high_score;
        Self {
            renderer,
            package,
            sim,
            screen: Screen::Title,
            stage,
            hold: 0.0,
            clock: 0.0,
            save,
            save_path,
            high_score: save.high_score,
            carry_lives: 3,
            carry_score: 0,
            carry_star: 0,
        }
    }

    /// `fresh` starts a new run (stage 1 / retry); otherwise lives, score and
    /// star tier carry over from the stage that was just cleared.
    fn start_stage(&mut self, stage: usize, fresh: bool) {
        self.stage = stage.min(self.package.levels.len() - 1);
        self.sim = Sim::new(self.stage, self.package.levels[self.stage].clone());
        if !fresh {
            self.sim.lives = self.carry_lives.max(1);
            self.sim.score = self.carry_score;
            self.sim.star = self.carry_star;
        }
        self.carry_lives = self.sim.lives;
        self.carry_score = self.sim.score;
        self.carry_star = self.sim.star;
        self.sim.high_score = self.high_score;
        self.screen = Screen::Intro;
        self.hold = INTRO_HOLD;
    }

    fn input(&self) -> Input {
        Input {
            up: is_key_down(KeyCode::Up) || is_key_down(KeyCode::W),
            down: is_key_down(KeyCode::Down) || is_key_down(KeyCode::S),
            left: is_key_down(KeyCode::Left) || is_key_down(KeyCode::A),
            right: is_key_down(KeyCode::Right) || is_key_down(KeyCode::D),
            fire: is_key_down(KeyCode::Space) || is_key_down(KeyCode::J),
        }
    }

    fn overlay(&self) -> Overlay {
        match self.screen {
            Screen::Playing => Overlay::None,
            Screen::Paused => Overlay::Paused,
            Screen::StageClear => Overlay::StageClear,
            Screen::GameOver => Overlay::GameOver,
            Screen::Victory => Overlay::Victory,
            _ => Overlay::None,
        }
    }

    fn persist(&mut self) {
        self.high_score = self.high_score.max(self.sim.score);
        self.save.record(self.sim.score, self.stage);
        self.save.high_score = self.save.high_score.max(self.high_score);
        if let Err(err) = save::write(&self.save_path, self.save) {
            eprintln!(
                "battle-city: cannot save {}: {err}",
                self.save_path.display()
            );
        }
    }
}
#[macroquad::main(window_conf)]
async fn main() {
    let dir = match package_dir() {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("battle-city: {err}");
            std::process::exit(1);
        }
    };
    let package = match load_package(&dir) {
        Ok(package) => package,
        Err(err) => {
            eprintln!("battle-city: {err}");
            std::process::exit(1);
        }
    };
    eprintln!(
        "battle-city: package {} | {} sheet(s) | {} level(s)",
        package.package_id,
        package.sheets.len(),
        package.levels.len()
    );
    if package.levels.is_empty() {
        eprintln!("battle-city: the package delivers no levels");
        std::process::exit(1);
    }

    let font = load_ui_font();
    let zh = font.is_some();
    let mut game = Game::new(package, font, zh);
    let mut accumulator = 0.0f32;

    // `--shot <path> --stage N` renders a scripted frame and exits.
    let shot = shot_path();
    if shot.is_some() {
        let stage = stage_arg().unwrap_or(1).saturating_sub(1);
        let last = game.package.levels.len() - 1;
        game.start_stage(stage.min(last), true);
        game.screen = Screen::Playing;
        let script: [Input; 4] = [
            Input {
                up: true,
                fire: true,
                ..Default::default()
            },
            Input {
                right: true,
                ..Default::default()
            },
            Input {
                left: true,
                fire: true,
                ..Default::default()
            },
            Input {
                down: true,
                ..Default::default()
            },
        ];
        for tick in 0..360 {
            let input = script[(tick / 80) % script.len()];
            game.sim.step(input, DT);
        }
        game.clock = 1.2;
        match screen_arg().as_deref() {
            Some("title") => game.screen = Screen::Title,
            Some("intro") => {
                game.screen = Screen::Intro;
                game.hold = 1.0;
            }
            Some("pause") => game.screen = Screen::Paused,
            Some("gameover") => game.screen = Screen::GameOver,
            Some("victory") => game.screen = Screen::Victory,
            _ => {}
        }
    }
    let mut shot_frames = 0;

    loop {
        let frame_time = get_frame_time().min(0.25);
        game.clock += frame_time;

        if is_key_pressed(KeyCode::Escape) {
            game.persist();
            break;
        }

        match game.screen {
            Screen::Title => {
                if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
                    let stage = (game.save.best_stage as usize).saturating_sub(1);
                    game.start_stage(stage, true);
                } else if is_key_pressed(KeyCode::R) {
                    game.start_stage(0, true);
                }
            }
            Screen::Intro => {
                game.hold -= frame_time;
                if game.hold <= 0.0 || is_key_pressed(KeyCode::Enter) {
                    game.screen = Screen::Playing;
                    accumulator = 0.0;
                }
            }
            Screen::Playing => {
                if is_key_pressed(KeyCode::P) {
                    game.screen = Screen::Paused;
                    accumulator = 0.0;
                } else {
                    accumulator = (accumulator + frame_time).min(0.25);
                    while accumulator >= DT {
                        let input = game.input();
                        game.sim.step(input, DT);
                        accumulator -= DT;
                        match game.sim.outcome {
                            Outcome::StageClear => {
                                game.carry_lives = game.sim.lives;
                                game.carry_score = game.sim.score;
                                game.carry_star = game.sim.star;
                                game.persist();
                                game.screen = Screen::StageClear;
                                game.hold = CLEAR_HOLD;
                                break;
                            }
                            Outcome::Victory => {
                                game.persist();
                                game.screen = Screen::Victory;
                                break;
                            }
                            Outcome::GameOver => {
                                game.persist();
                                game.screen = Screen::GameOver;
                                break;
                            }
                            Outcome::Playing => {}
                        }
                    }
                }
            }
            Screen::Paused => {
                if is_key_pressed(KeyCode::P) || is_key_pressed(KeyCode::Enter) {
                    game.screen = Screen::Playing;
                    accumulator = 0.0;
                }
            }
            Screen::StageClear => {
                game.hold -= frame_time;
                if game.hold <= 0.0 {
                    let next = game.stage + 1;
                    if next < game.package.levels.len() {
                        game.start_stage(next, false);
                        accumulator = 0.0;
                    } else {
                        game.screen = Screen::Victory;
                    }
                }
            }
            Screen::GameOver | Screen::Victory => {
                if is_key_pressed(KeyCode::Enter) {
                    game.screen = Screen::Title;
                } else if is_key_pressed(KeyCode::R) {
                    game.start_stage(0, true);
                }
            }
        }

        clear_background(BLACK);
        let vp = Viewport::new(screen_width(), screen_height());
        match game.screen {
            Screen::Title => game.renderer.draw_title(
                &game.package,
                &vp,
                game.clock,
                game.high_score,
                game.save.best_stage,
            ),
            Screen::Intro => {
                game.renderer
                    .draw_stage_intro(&game.package, &game.sim, &vp, game.hold)
            }
            _ => {
                game.renderer.draw_world(&game.package, &game.sim, &vp);
                game.renderer
                    .draw_hud(&game.package, &game.sim, &vp, game.high_score);
                game.renderer.draw_overlay(
                    &vp,
                    &game.overlay(),
                    &game.sim,
                    game.high_score,
                    game.clock,
                );
            }
        }

        // The back buffer is only readable before it is presented.
        if let Some(path) = &shot {
            shot_frames += 1;
            if shot_frames == 3 {
                let mut image = get_screen_data();
                for pixel in image.bytes.chunks_exact_mut(4) {
                    pixel[3] = 255;
                }
                image.export_png(&path.to_string_lossy());
                eprintln!(
                    "battle-city: screenshot {} ({}x{})",
                    path.display(),
                    image.width,
                    image.height
                );
                next_frame().await;
                break;
            }
        }
        next_frame().await;
    }
}

/// `--shot <path>`: capture the rendered frame and exit.
fn shot_path() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == "--shot")
        .and_then(|at| args.get(at + 1))
        .map(PathBuf::from)
}

/// `--stage N`: 1-based stage used by `--shot`.
fn stage_arg() -> Option<usize> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == "--stage")
        .and_then(|at| args.get(at + 1))
        .and_then(|value| value.parse().ok())
}

/// `--screen title|intro|pause|gameover|victory`: UI state used by `--shot`.
fn screen_arg() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == "--screen")
        .and_then(|at| args.get(at + 1))
        .cloned()
}
