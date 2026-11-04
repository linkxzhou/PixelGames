use miniquad::*;
use std::ffi::CString;
extern "C" { fn console_log(message: *const i8); }
fn log(message: String) { unsafe { console_log(CString::new(message).unwrap().as_ptr()) } }
struct Stage { ctx: Box<dyn RenderingBackend> }
impl EventHandler for Stage {
    fn update(&mut self) {}
    fn draw(&mut self) {
        self.ctx.begin_default_pass(PassAction::clear_color(0.1, 0.1, 0.1, 1.0));
        self.ctx.end_render_pass(); self.ctx.commit_frame();
    }
    fn key_down_event(&mut self, key: KeyCode, mods: KeyMods, repeat: bool) { log(format!("key down {key:?} mods={mods:?} repeat={repeat}")); }
    fn key_up_event(&mut self, key: KeyCode, _mods: KeyMods) { log(format!("key up {key:?}")); }
    fn mouse_motion_event(&mut self, x: f32, y: f32) { log(format!("mouse {x},{y}")); }
    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) { log(format!("button {button:?} {x},{y}")); }
    fn mouse_wheel_event(&mut self, x: f32, y: f32) { log(format!("wheel {x},{y}")); }
    fn touch_event(&mut self, phase: TouchPhase, id: u64, x: f32, y: f32) { log(format!("touch {phase:?} {id} {x},{y}")); }
}
#[no_mangle]
pub fn main() { miniquad::start(conf::Conf::default(), || Box::new(Stage { ctx: window::new_rendering_backend() })); }
