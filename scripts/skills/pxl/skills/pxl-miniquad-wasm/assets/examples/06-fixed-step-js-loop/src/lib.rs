use miniquad::*;
extern "C" { fn loop_js_update(dt: f64); fn loop_js_draw(); }
static mut COMMANDS: *const u32 = std::ptr::null();
static mut COLOR: u32 = 0xff101010;
struct Stage { ctx: Box<dyn RenderingBackend> }
impl EventHandler for Stage {
    fn update(&mut self) { unsafe { loop_js_update(1.0 / 60.0); } }
    fn draw(&mut self) {
        unsafe { loop_js_draw(); }
        let rgba = unsafe { COLOR };
        let channel = |shift: u32| ((rgba >> shift) & 0xffu32) as f32 / 255.0;
        self.ctx.begin_default_pass(PassAction::clear_color(channel(24), channel(16), channel(8), channel(0)));
        self.ctx.end_render_pass(); self.ctx.commit_frame();
    }
}
#[no_mangle] pub extern "C" fn loop_crate_version() -> u32 { 1 }
#[no_mangle] pub extern "C" fn loop_alloc(len: usize) -> *mut u8 {
    let mut data = vec![0u8; len]; let ptr = data.as_mut_ptr(); std::mem::forget(data); ptr
}
#[no_mangle] pub extern "C" fn loop_set_commands(ptr: *const u32) { unsafe { COMMANDS = ptr; } }
#[no_mangle] pub extern "C" fn loop_submit() { unsafe { if !COMMANDS.is_null() { COLOR = *COMMANDS; } } }
#[no_mangle]
pub fn main() { miniquad::start(conf::Conf::default(), || Box::new(Stage { ctx: window::new_rendering_backend() })); }
