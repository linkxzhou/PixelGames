use miniquad::*;
#[repr(C)]
struct Vertex { pos: [f32; 2], uv: [f32; 2] }
struct Stage { ctx: Box<dyn RenderingBackend>, pipeline: Pipeline, bindings: Bindings }
impl Stage {
    fn new() -> Self {
        let mut ctx = window::new_rendering_backend();
        let pixels = [255,0,0,255, 0,255,0,255, 0,0,255,255, 255,255,255,255];
        let texture = ctx.new_texture_from_rgba8(2, 2, &pixels);
        ctx.texture_set_filter(texture, FilterMode::Nearest, MipmapFilterMode::None);
        let vertices = [
            Vertex { pos: [-0.8, 0.8], uv: [0.0, 0.0] }, Vertex { pos: [0.8, 0.8], uv: [1.0, 0.0] },
            Vertex { pos: [0.8, -0.8], uv: [1.0, 1.0] }, Vertex { pos: [-0.8, -0.8], uv: [0.0, 1.0] },
        ];
        let vertex = ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Immutable, BufferSource::slice(&vertices));
        let index = ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable, BufferSource::slice(&[0u16,1,2,0,2,3]));
        let shader = ctx.new_shader(ShaderSource::Glsl {
            vertex: "#version 100\nattribute vec2 pos;attribute vec2 uv;varying lowp vec2 texcoord;void main(){gl_Position=vec4(pos,0,1);texcoord=uv;}",
            fragment: "#version 100\nvarying lowp vec2 texcoord;uniform sampler2D tex;void main(){gl_FragColor=texture2D(tex,texcoord);}",
        }, ShaderMeta { images: vec!["tex".into()], uniforms: UniformBlockLayout { uniforms: vec![] } }).unwrap();
        let pipeline = ctx.new_pipeline(&[BufferLayout::default()], &[
            VertexAttribute::new("pos", VertexFormat::Float2), VertexAttribute::new("uv", VertexFormat::Float2),
        ], shader, PipelineParams::default());
        Self { ctx, pipeline, bindings: Bindings { vertex_buffers: vec![vertex], index_buffer: index, images: vec![texture] } }
    }
}
impl EventHandler for Stage {
    fn update(&mut self) {}
    fn draw(&mut self) {
        self.ctx.begin_default_pass(PassAction::clear_color(0.0, 0.0, 0.0, 1.0));
        self.ctx.apply_pipeline(&self.pipeline); self.ctx.apply_bindings(&self.bindings);
        self.ctx.draw(0, 6, 1); self.ctx.end_render_pass(); self.ctx.commit_frame();
    }
}
#[no_mangle]
pub fn main() { miniquad::start(conf::Conf::default(), || Box::new(Stage::new())); }
