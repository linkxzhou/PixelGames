//! Native entry: `--headless` runs the deterministic replay. Otherwise the
//! macroquad window presents the same simulation at a fixed 60 Hz step.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let headless = args.iter().any(|a| a == "--headless") || !cfg!(feature = "window");
    if headless || cfg!(not(feature = "window")) {
        if let Err(err) = battle_city::cli(args) {
            eprintln!("battle-city: {err}");
            std::process::exit(1);
        }
        return;
    }
    #[cfg(feature = "window")]
    window::run();
}

#[cfg(feature = "window")]
mod window {
    use macroquad::prelude::*;

    use battle_city::draw::{self, Frame, LOGIC_H, LOGIC_W};
    use battle_city::level::Level;
    use battle_city::palette::Palette;
    use battle_city::sim::{Dir, Input, Outcome, Sim};

    const DT: f32 = 1.0 / 60.0;

    pub fn run() {
        let mut conf = Conf::default();
        conf.window_title = "Battle City".to_owned();
        conf.window_width = (LOGIC_W * 3) as i32;
        conf.window_height = (LOGIC_H * 3) as i32;
        conf.window_resizable = false;
        macroquad::Window::from_config(conf, present());
    }

    async fn present() {
        let mut level_index = 1u32;
        let mut sim = Sim::new(level_index, Level::bundled(level_index).expect("level"));
        let palette = Palette::battle_city();
        let mut frame = Frame::new();
        let mut image = Image::gen_image_color(LOGIC_W as u16, LOGIC_H as u16, BLACK);
        let texture = Texture2D::from_image(&image);
        texture.set_filter(FilterMode::Nearest);
        let mut acc = 0.0f32;
        loop {
            if is_key_pressed(KeyCode::Key1) {
                level_index = 1;
                sim = fresh(level_index);
            } else if is_key_pressed(KeyCode::Key2) {
                level_index = 2;
                sim = fresh(level_index);
            } else if is_key_pressed(KeyCode::Key3) {
                level_index = 3;
                sim = fresh(level_index);
            } else if is_key_pressed(KeyCode::R) {
                sim = fresh(level_index);
            }
            let input = read_input();
            acc = (acc + get_frame_time()).min(0.25);
            while acc >= DT {
                if sim.outcome == Outcome::Playing {
                    sim.step(input);
                } else {
                    sim.step(Input::default());
                }
                acc -= DT;
            }
            draw::render(&sim, &mut frame);
            paint(&frame, &palette, &mut image);
            texture.update(&image);
            texture.set_filter(FilterMode::Nearest);
            clear_background(BLACK);
            draw_texture_ex(
                &texture,
                0.0,
                0.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(screen_width(), screen_height())),
                    ..Default::default()
                },
            );
            next_frame().await;
        }
    }

    fn fresh(index: u32) -> Sim {
        Sim::new(index, Level::bundled(index).expect("level"))
    }

    fn read_input() -> Input {
        let dir = if down(KeyCode::Up) || down(KeyCode::W) {
            Some(Dir::N)
        } else if down(KeyCode::Right) || down(KeyCode::D) {
            Some(Dir::E)
        } else if down(KeyCode::Down) || down(KeyCode::S) {
            Some(Dir::S)
        } else if down(KeyCode::Left) || down(KeyCode::A) {
            Some(Dir::W)
        } else {
            None
        };
        Input {
            dir,
            shoot: down(KeyCode::Space),
        }
    }

    fn down(key: KeyCode) -> bool {
        is_key_down(key)
    }

    fn paint(frame: &Frame, palette: &Palette, image: &mut Image) {
        let rgba = frame.to_rgba(palette);
        image.bytes.copy_from_slice(&rgba);
    }
}
