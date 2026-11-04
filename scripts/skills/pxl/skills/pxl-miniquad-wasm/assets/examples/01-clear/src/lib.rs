use miniquad::*;
struct Stage { ctx: Box<dyn RenderingBackend> }
impl EventHandler for Stage {
    fn update(&mut self) {}
    fn draw(&mut self) {
        self.ctx.begin_default_pass(PassAction::clear_color(0.1, 0.2, 0.3, 1.0));
        self.ctx.end_render_pass();
        self.ctx.commit_frame();
    }
}
#[no_mangle]
pub fn main() {
    miniquad::start(conf::Conf::default(), || Box::new(Stage { ctx: window::new_rendering_backend() }));
}
