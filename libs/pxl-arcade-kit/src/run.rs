//! Window loop, keyboard polling, fixed-step accumulator and screenshot mode.
//!
//! Games keep `update`/`draw` pure; everything platform specific lives here.

use std::path::PathBuf;

use macroquad::prelude::*;

use crate::gfx::Gfx;
use crate::input::{Buttons, InputState};
use crate::package::Package;
use crate::screen::Viewport;
use crate::shot;

pub struct Config {
    pub title: &'static str,
    pub logical: (u32, u32),
    pub clear: Color,
    /// Logical step in milliseconds; every remake runs 60 Hz.
    pub fixed_ms: u32,
}

impl Config {
    pub fn new(title: &'static str, logical: (u32, u32)) -> Config {
        Config {
            title,
            logical,
            clear: Color::new(0.02, 0.02, 0.05, 1.0),
            fixed_ms: 16,
        }
    }

    pub fn clear(mut self, color: Color) -> Config {
        self.clear = color;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    /// `--package` when the player passed one; otherwise the default is
    /// searched for, so the binary works from any working directory.
    pub explicit_package: Option<PathBuf>,
    /// Workspace-relative package path, e.g.
    /// `games/todo-01-pac-man/work/400-package`.
    pub default_package: String,
    pub shot: Option<PathBuf>,
    pub seed: Option<u64>,
    pub ticks: Option<u32>,
    pub level: Option<usize>,
}

impl Options {
    /// Parse `--package`, `--shot`, `--seed`, `--ticks`, `--level`.
    pub fn parse(default_package: &str) -> Options {
        let args: Vec<String> = std::env::args().collect();
        let value = |flag: &str| -> Option<String> {
            args.iter()
                .position(|arg| arg == flag)
                .and_then(|at| args.get(at + 1))
                .cloned()
        };
        Options {
            explicit_package: value("--package").map(PathBuf::from),
            default_package: default_package.to_string(),
            shot: value("--shot").map(PathBuf::from),
            seed: value("--seed").and_then(|text| text.parse().ok()),
            ticks: value("--ticks").and_then(|text| text.parse().ok()),
            level: value("--level").and_then(|text| text.parse().ok()),
        }
    }

    /// Find the pxl `400-package` for this run.
    pub fn resolve(&self) -> Result<PathBuf, String> {
        crate::path::resolve_package(
            self.explicit_package.as_deref(),
            &self.default_package,
            &crate::path::Candidates::from_env(),
        )
    }
}

pub trait Game {
    fn update(&mut self, input: InputState, dt_ms: u32);
    fn draw(&self, gfx: &Gfx);
    fn package(&self) -> &Package;
    /// Called once before a screenshot run so the frame shows gameplay.
    fn warm_up(&mut self, _ticks: u32) {}
    /// Rendered frames to skip before the screenshot is taken.
    fn shot_ticks(&self) -> u32 {
        30
    }
}

pub struct App<G: Game> {
    config: Config,
    options: Options,
    game: G,
    gfx: Gfx,
}

impl<G: Game> App<G> {
    /// Load the package, upload the sheets and build the game.
    pub fn new(config: Config, options: Options, make: impl FnOnce(&Package, &Options) -> G) -> Result<App<G>, String> {
        let package = Package::load(&options.resolve()?)?;
        if package.levels.is_empty() {
            eprintln!(
                "warning: {} delivers no levels; the game will use built-in tables",
                package.package_id
            );
        }
        let gfx = Gfx::new(&package)?;
        let game = make(&package, &options);
        eprintln!(
            "{}: package {} ({} sheets, {} levels)",
            config.title,
            package.package_id,
            package.sheets.len(),
            package.levels.len()
        );
        Ok(App {
            config,
            options,
            game,
            gfx,
        })
    }

    pub async fn run(mut self) {
        if self.options.shot.is_some() {
            self.game.warm_up(self.game.shot_ticks());
        }
        let mut previous_frame = Buttons::NONE;
        let mut accumulator = 0.0f32;
        let mut rendered = 0u32;
        let fixed = self.config.fixed_ms as f32;
        loop {
            let frame_time = get_frame_time().min(0.1);
            accumulator += frame_time * 1000.0;
            while accumulator >= fixed {
                accumulator -= fixed;
                let held = poll_buttons();
                let input = InputState::new(held, previous_frame);
                self.game.update(input, self.config.fixed_ms);
                previous_frame = held;
            }

            if is_key_pressed(KeyCode::Escape) && self.options.shot.is_none() {
                break;
            }

            let view = Viewport::fit(
                self.config.logical.0,
                self.config.logical.1,
                screen_width() as u32,
                screen_height() as u32,
            );
            let (x, y, w, h) = view.camera_rect(screen_width() as u32, screen_height() as u32);
            // A positive zoom on both axes keeps y growing downwards; the
            // `from_display_rect` helper flips y, which mirrors every sprite.
            let camera = Camera2D {
                target: vec2(x + w / 2.0, y + h / 2.0),
                zoom: vec2(2.0 / w, 2.0 / h),
                ..Default::default()
            };
            set_camera(&camera);
            clear_background(self.config.clear);
            self.game.draw(&self.gfx);
            set_default_camera();

            if let Some(path) = self.options.shot.clone() {
                rendered += 1;
                if rendered >= self.options.ticks.unwrap_or(3) {
                    if let Err(error) = shot::capture(&path) {
                        eprintln!("{error}");
                        std::process::exit(1);
                    }
                    eprintln!("{}: screenshot {}", self.config.title, path.display());
                    next_frame().await;
                    break;
                }
            }
            next_frame().await;
        }
    }
}

pub fn poll_buttons() -> Buttons {
    Buttons {
        up: is_key_down(KeyCode::Up) || is_key_down(KeyCode::W),
        down: is_key_down(KeyCode::Down) || is_key_down(KeyCode::S),
        left: is_key_down(KeyCode::Left) || is_key_down(KeyCode::A),
        right: is_key_down(KeyCode::Right) || is_key_down(KeyCode::D),
        primary: is_key_down(KeyCode::Space) || is_key_down(KeyCode::J) || is_key_down(KeyCode::Z),
        secondary: is_key_down(KeyCode::K) || is_key_down(KeyCode::X) || is_key_down(KeyCode::LeftShift),
        start: is_key_down(KeyCode::Enter),
        pause: is_key_down(KeyCode::Escape) || is_key_down(KeyCode::P),
        coin: is_key_down(KeyCode::C) || is_key_down(KeyCode::Key5),
    }
}

/// Convenience for a window title derived from the binary name.
pub fn window_conf(title: &'static str, logical: (u32, u32), scale: u32) -> Conf {
    Conf {
        window_title: title.to_string(),
        window_width: (logical.0 * scale) as i32,
        window_height: (logical.1 * scale) as i32,
        high_dpi: true,
        window_resizable: true,
        ..Default::default()
    }
}
