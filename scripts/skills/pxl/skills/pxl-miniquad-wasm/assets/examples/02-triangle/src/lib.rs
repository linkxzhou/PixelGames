use miniquad::*;
#[repr(C)]
struct Vertex { pos: [f32; 2], color: [f32; 4] }
struct Stage { ctx: Box<dyn RenderingBackend>, pipeline: Pipeline, bindings: Bindings }
impl Stage {
    fn new() -> Self {
        let mut ctx = window::new_rendering_backend();
        let vertices = [
            Vertex { pos: [0.0, 0.7], color: [1.0, 0.0, 0.0, 1.0] },
            Vertex { pos: [-0.7, -0.7], color: [0.0, 1.0, 0.0, 1.0] },
            Vertex { pos: [0.7, -0.7], color: [0.0, 0.0, 1.0, 1.0] },
        ];
        let vertex = ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Immutable, BufferSource::slice(&vertices));
        let index = ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable, BufferSource::slice(&[0u16, 1, 2]));
        let shader = ctx.new_shader(ShaderSource::Glsl {
            vertex: "#version 100\nattribute vec2 pos; attribute vec4 color0; varying lowp vec4 color; void main(){gl_Position=vec4(pos,0,1);color=color0;}",
            fragment: "#version 100\nvarying lowp vec4 color; void main(){gl_FragColor=color;}",
        }, ShaderMeta { images: vec![], uniforms: UniformBlockLayout { uniforms: vec![] } }).unwrap();
        let pipeline = ctx.new_pipeline(&[BufferLayout::default()], &[
            VertexAttribute::new("pos", VertexFormat::Float2), VertexAttribute::new("color0", VertexFormat::Float4),
        ], shader, PipelineParams::default());
        Self { ctx, pipeline, bindings: Bindings { vertex_buffers: vec![vertex], index_buffer: index, images: vec![] } }
    }
}
impl EventHandler for Stage {
    fn update(&mut self) {}
    fn draw(&mut self) {
        self.ctx.begin_default_pass(PassAction::clear_color(0.08, 0.08, 0.1, 1.0));
        self.ctx.apply_pipeline(&self.pipeline);
        self.ctx.apply_bindings(&self.bindings);
        self.ctx.draw(0, 3, 1);
        self.ctx.end_render_pass();
        self.ctx.commit_frame();
    }
}
#[no_mangle]
pub fn main() { miniquad::start(conf::Conf::default(), || Box::new(Stage::new())); }
