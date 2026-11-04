use miniquad::*;
extern "C" { fn demo_js_frame(tick: u32); }
static mut TICK: u32 = 0;
struct Stage { ctx: Box<dyn RenderingBackend> }
impl EventHandler for Stage {
    fn update(&mut self) {
        unsafe { TICK += 1; demo_js_frame(TICK); }
    }
    fn draw(&mut self) {
        self.ctx.begin_default_pass(PassAction::clear_color(0.1, 0.15, 0.2, 1.0));
        self.ctx.end_render_pass(); self.ctx.commit_frame();
    }
}
#[no_mangle] pub extern "C" fn demo_crate_version() -> u32 { 1 }
#[no_mangle] pub extern "C" fn demo_alloc(len: usize) -> *mut u8 {
    let mut data = vec![0u8; len]; let ptr = data.as_mut_ptr(); std::mem::forget(data); ptr
}
#[no_mangle] pub extern "C" fn demo_sum(ptr: *const u8, len: usize) -> u32 {
    unsafe { std::slice::from_raw_parts(ptr, len) }.iter().map(|v| *v as u32).sum()
}
#[no_mangle]
pub fn main() { miniquad::start(conf::Conf::default(), || Box::new(Stage { ctx: window::new_rendering_backend() })); }
