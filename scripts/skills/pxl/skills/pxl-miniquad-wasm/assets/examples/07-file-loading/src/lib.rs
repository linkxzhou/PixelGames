use miniquad::*;
use std::ffi::CString;
extern "C" { fn console_log(message: *const i8); }
fn log(message: String) { unsafe { console_log(CString::new(message).unwrap().as_ptr()) } }
struct Stage { ctx: Box<dyn RenderingBackend>, requested: bool }
impl EventHandler for Stage {
    fn update(&mut self) {
        if !self.requested {
            self.requested = true;
            fs::load_file("data.txt", |response| match response {
                Ok(bytes) => log(format!("loaded {} bytes", bytes.len())),
                Err(error) => log(format!("load failed: {error:?}")),
            });
        }
    }
    fn draw(&mut self) {
        self.ctx.begin_default_pass(PassAction::clear_color(0.1, 0.1, 0.15, 1.0));
        self.ctx.end_render_pass(); self.ctx.commit_frame();
    }
}
#[no_mangle]
pub fn main() { miniquad::start(conf::Conf::default(), || Box::new(Stage { ctx: window::new_rendering_backend(), requested: false })); }
