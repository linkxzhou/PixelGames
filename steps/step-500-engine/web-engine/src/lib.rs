mod batch;
mod input;
mod shader;
mod texture;

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use batch::{color, Command, COMMAND_WORDS};
use input::InputQueue;
use miniquad::*;
use texture::PendingTexture;

const MAX_QUADS: usize = 16_384;

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn pxl_js_update(dt: f64);
    fn pxl_js_draw();
}

#[cfg(not(target_arch = "wasm32"))]
extern "C" fn pxl_js_update(_dt: f64) {}

#[cfg(not(target_arch = "wasm32"))]
extern "C" fn pxl_js_draw() {}

struct Config {
    width: f32,
    height: f32,
    scale: f32,
}

struct Shared {
    config: Config,
    commands: *const u32,
    command_capacity: usize,
    command_count: usize,
    pending_textures: Vec<PendingTexture>,
    /// Texture ids the runtime released since the last frame; the renderer
    /// drops them so repeated scene loads cannot leak GPU memory.
    freed_textures: Vec<u32>,
    input: InputQueue,
}

unsafe impl Send for Shared {}

impl Default for Shared {
    fn default() -> Self {
        Self {
            config: Config {
                width: 480.0,
                height: 416.0,
                scale: 0.0,
            },
            commands: std::ptr::null(),
            command_capacity: 0,
            command_count: 0,
            pending_textures: Vec::new(),
            freed_textures: Vec::new(),
            input: InputQueue::default(),
        }
    }
}

static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();

fn shared() -> &'static Mutex<Shared> {
    SHARED.get_or_init(|| Mutex::new(Shared::default()))
}

#[no_mangle]
pub extern "C" fn pxl_runtime_version() -> u32 {
    2
}

#[no_mangle]
pub extern "C" fn pxl_crate_version() -> u32 {
    2
}

#[no_mangle]
pub extern "C" fn pxl_configure(width: f32, height: f32, scale: f32) {
    let mut shared = shared().lock().unwrap();
    if width > 0.0 && height > 0.0 {
        shared.config.width = width;
        shared.config.height = height;
        shared.config.scale = if scale >= 0.0 { scale } else { 0.0 };
    }
}

#[no_mangle]
pub extern "C" fn pxl_alloc(len: usize) -> *mut u8 {
    let mut bytes = vec![0u8; len];
    let ptr = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    ptr
}

#[no_mangle]
pub extern "C" fn pxl_set_commands(ptr: *mut u32, capacity: usize) {
    let mut shared = shared().lock().unwrap();
    shared.commands = ptr;
    shared.command_capacity = capacity;
    shared.command_count = 0;
}

#[no_mangle]
pub extern "C" fn pxl_submit(count: usize) {
    let mut shared = shared().lock().unwrap();
    shared.command_count = count.min(shared.command_capacity);
}

#[no_mangle]
pub extern "C" fn pxl_upload_texture(
    id: u32,
    width: u32,
    height: u32,
    ptr: *const u8,
    len: usize,
) -> i32 {
    let expected = width as usize * height as usize * 4;
    if id == 0
        || width == 0
        || height == 0
        || width > u16::MAX as u32
        || height > u16::MAX as u32
        || ptr.is_null()
        || len != expected
    {
        return -1;
    }
    let rgba = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
    shared()
        .lock()
        .unwrap()
        .pending_textures
        .push(PendingTexture {
            id,
            width: width as u16,
            height: height as u16,
            rgba,
        });
    0
}

#[no_mangle]
pub extern "C" fn pxl_free_texture(id: u32) -> i32 {
    if id == 0 {
        return -1;
    }
    let mut shared = shared().lock().unwrap();
    // A texture that never uploaded is not an error: releasing an absent
    // id is a no-op so callers need not track upload state.
    shared.pending_textures.retain(|item| item.id != id);
    if !shared.freed_textures.contains(&id) {
        shared.freed_textures.push(id);
    }
    0
}

#[no_mangle]
pub extern "C" fn pxl_input_ptr() -> *const input::InputEvent {
    shared().lock().unwrap().input.ptr()
}

#[no_mangle]
pub extern "C" fn pxl_input_len() -> usize {
    shared().lock().unwrap().input.len()
}

#[no_mangle]
pub extern "C" fn pxl_input_clear() {
    shared().lock().unwrap().input.clear();
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

struct DrawCall {
    texture: TextureId,
    base: i32,
    count: i32,
}

struct Renderer {
    ctx: Box<dyn RenderingBackend>,
    pipeline: Pipeline,
    vertex_buffer: BufferId,
    index_buffer: BufferId,
    white: TextureId,
    textures: BTreeMap<u32, (TextureId, f32, f32)>,
    vertices: Vec<Vertex>,
    calls: Vec<DrawCall>,
    camera: (f32, f32),
}

impl Renderer {
    fn new() -> Self {
        let mut ctx = window::new_rendering_backend();
        let white = ctx.new_texture_from_rgba8(1, 1, &[255, 255, 255, 255]);
        ctx.texture_set_filter(white, FilterMode::Nearest, MipmapFilterMode::None);
        let shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: shader::VERTEX,
                    fragment: shader::FRAGMENT,
                },
                ShaderMeta {
                    images: vec!["tex".to_string()],
                    uniforms: UniformBlockLayout { uniforms: vec![] },
                },
            )
            .expect("pxl shader");
        let pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("pos", VertexFormat::Float2),
                VertexAttribute::new("uv", VertexFormat::Float2),
                VertexAttribute::new("color0", VertexFormat::Float4),
            ],
            shader,
            PipelineParams {
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                alpha_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::One,
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                ..Default::default()
            },
        );
        let vertex_buffer = ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Stream,
            BufferSource::slice(&vec![Vertex::default(); MAX_QUADS * 4]),
        );
        let mut indices = Vec::with_capacity(MAX_QUADS * 6);
        for quad in 0..MAX_QUADS as u16 {
            let at = quad * 4;
            indices.extend_from_slice(&[at, at + 1, at + 2, at, at + 2, at + 3]);
        }
        let index_buffer = ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );
        Self {
            ctx,
            pipeline,
            vertex_buffer,
            index_buffer,
            white,
            textures: BTreeMap::new(),
            vertices: Vec::with_capacity(MAX_QUADS * 4),
            calls: Vec::new(),
            camera: (0.0, 0.0),
        }
    }

    fn upload_pending(&mut self) {
        let mut shared = shared().lock().unwrap();
        let pending = std::mem::take(&mut shared.pending_textures);
        let freed = std::mem::take(&mut shared.freed_textures);
        drop(shared);
        for item in pending {
            let dimensions = (item.width as f32, item.height as f32);
            let texture = texture::upload(self.ctx.as_mut(), &item);
            self.textures
                .insert(item.id, (texture, dimensions.0, dimensions.1));
        }
        for id in freed {
            if let Some((texture, _, _)) = self.textures.remove(&id) {
                self.ctx.as_mut().delete_texture(texture);
            }
        }
    }

    fn push_quad(
        &mut self,
        texture: TextureId,
        src: [f32; 4],
        dst: [f32; 4],
        flip_x: bool,
        flip_y: bool,
        tint: u32,
        projection: Projection,
    ) {
        if self.vertices.len() + 4 > MAX_QUADS * 4 {
            return;
        }
        let mut u0 = src[0];
        let mut u1 = src[0] + src[2];
        let mut v0 = src[1];
        let mut v1 = src[1] + src[3];
        if flip_x {
            std::mem::swap(&mut u0, &mut u1);
        }
        if flip_y {
            std::mem::swap(&mut v0, &mut v1);
        }
        let uv = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        let points = [
            [dst[0], dst[1]],
            [dst[0] + dst[2], dst[1]],
            [dst[0] + dst[2], dst[1] + dst[3]],
            [dst[0], dst[1] + dst[3]],
        ];
        let base = self.vertices.len() as i32 / 4 * 6;
        for index in 0..4 {
            self.vertices.push(Vertex {
                pos: projection.point(points[index][0], points[index][1], self.camera),
                uv: uv[index],
                color: color(tint),
            });
        }
        if let Some(call) = self.calls.last_mut() {
            if call.texture == texture {
                call.count += 6;
                return;
            }
        }
        self.calls.push(DrawCall {
            texture,
            base,
            count: 6,
        });
    }

    fn push_light(
        &mut self,
        x: f32,
        y: f32,
        radius: f32,
        rgba: u32,
        levels: u32,
        projection: Projection,
    ) {
        if radius <= 0.0 || rgba & 0xff == 0 {
            return;
        }
        let levels = levels.clamp(1, 8);
        for band in 0..levels {
            let extent = radius * (levels - band) as f32 / levels as f32;
            let alpha = (rgba & 0xff) * (band + 1) / levels;
            let tint = (rgba & 0xffff_ff00) | alpha;
            self.push_quad(
                self.white,
                [0.0, 0.0, 1.0, 1.0],
                [x - extent, y - extent, extent * 2.0, extent * 2.0],
                false,
                false,
                tint,
                projection,
            );
        }
    }

    fn draw(&mut self) {
        self.upload_pending();
        let (words, config) = {
            let shared = shared().lock().unwrap();
            let words = if shared.commands.is_null() || shared.command_count == 0 {
                &[][..]
            } else {
                unsafe {
                    std::slice::from_raw_parts(
                        shared.commands,
                        shared.command_count * COMMAND_WORDS,
                    )
                }
            };
            (
                words.to_vec(),
                (
                    shared.config.width,
                    shared.config.height,
                    shared.config.scale,
                ),
            )
        };
        let clear = words
            .chunks_exact(COMMAND_WORDS)
            .find_map(|words| match batch::decode(words) {
                Some(Command::Clear(rgba)) => Some(color(rgba)),
                _ => None,
            })
            .unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let projection = Projection::new(config);
        self.vertices.clear();
        self.calls.clear();
        self.camera = (0.0, 0.0);
        for words in words.chunks_exact(COMMAND_WORDS) {
            match batch::decode(words) {
                Some(Command::Rect { x, y, w, h, rgba }) => self.push_quad(
                    self.white,
                    [0.0, 0.0, 1.0, 1.0],
                    [x, y, w, h],
                    false,
                    false,
                    rgba,
                    projection,
                ),
                Some(Command::Sprite {
                    texture,
                    mut src,
                    dst,
                    flip_x,
                    flip_y,
                    tint,
                }) => {
                    if let Some(&(id, width, height)) = self.textures.get(&texture) {
                        src[0] /= width;
                        src[1] /= height;
                        src[2] /= width;
                        src[3] /= height;
                        self.push_quad(id, src, dst, flip_x, flip_y, tint, projection);
                    }
                }
                Some(Command::Light {
                    x,
                    y,
                    radius,
                    rgba,
                    levels,
                }) => self.push_light(x, y, radius, rgba, levels, projection),
                Some(Command::Camera(x, y)) => self.camera = (x, y),
                Some(Command::Clear(_)) | None => {}
            }
        }
        self.ctx.begin_default_pass(PassAction::clear_color(
            clear[0], clear[1], clear[2], clear[3],
        ));
        if !self.vertices.is_empty() {
            self.ctx.apply_pipeline(&self.pipeline);
            self.ctx
                .buffer_update(self.vertex_buffer, BufferSource::slice(&self.vertices));
            for call in &self.calls {
                self.ctx.apply_bindings_from_slice(
                    &[self.vertex_buffer],
                    self.index_buffer,
                    &[call.texture],
                );
                self.ctx.draw(call.base, call.count, 1);
            }
        }
        self.ctx.end_render_pass();
        self.ctx.commit_frame();
    }
}

#[derive(Clone, Copy)]
struct Projection {
    scale: f32,
    ox: f32,
    oy: f32,
    width: f32,
    height: f32,
}

impl Projection {
    fn new(config: (f32, f32, f32)) -> Self {
        let (width, height) = window::screen_size();
        let scale = if config.2 > 0.0 {
            config.2
        } else {
            (width / config.0).min(height / config.1).floor().max(1.0)
        };
        Self {
            scale,
            ox: ((width - config.0 * scale) * 0.5).floor(),
            oy: ((height - config.1 * scale) * 0.5).floor(),
            width,
            height,
        }
    }

    fn point(&self, x: f32, y: f32, camera: (f32, f32)) -> [f32; 2] {
        let sx = self.ox + (x - camera.0) * self.scale;
        let sy = self.oy + (y - camera.1) * self.scale;
        [sx / self.width * 2.0 - 1.0, 1.0 - sy / self.height * 2.0]
    }
}

struct Handler {
    renderer: Renderer,
    last_frame: f64,
}

impl EventHandler for Handler {
    fn update(&mut self) {
        let now = date::now();
        let dt = (now - self.last_frame).clamp(0.0, 0.25);
        self.last_frame = now;
        unsafe { pxl_js_update(dt) };
    }

    fn draw(&mut self) {
        unsafe { pxl_js_draw() };
        self.renderer.draw();
    }

    fn key_down_event(&mut self, key: KeyCode, mods: KeyMods, repeat: bool) {
        shared().lock().unwrap().input.key_down(key, mods, repeat);
    }

    fn key_up_event(&mut self, key: KeyCode, _mods: KeyMods) {
        shared().lock().unwrap().input.key_up(key);
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        shared().lock().unwrap().input.mouse_move(x, y);
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        shared()
            .lock()
            .unwrap()
            .input
            .mouse_button(button, x, y, true);
    }

    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        shared()
            .lock()
            .unwrap()
            .input
            .mouse_button(button, x, y, false);
    }

    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        shared().lock().unwrap().input.wheel(x, y);
    }

    fn touch_event(&mut self, phase: TouchPhase, id: u64, x: f32, y: f32) {
        shared().lock().unwrap().input.touch(phase, id, x, y);
    }
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub fn main() {
    let (width, height, scale) = {
        let shared = shared().lock().unwrap();
        (
            shared.config.width,
            shared.config.height,
            shared.config.scale,
        )
    };
    let scale = if scale > 0.0 { scale } else { 2.0 };
    miniquad::start(
        conf::Conf {
            window_title: "PixelGames".to_string(),
            window_width: (width * scale) as i32,
            window_height: (height * scale) as i32,
            window_resizable: true,
            high_dpi: true,
            ..Default::default()
        },
        || {
            Box::new(Handler {
                renderer: Renderer::new(),
                last_frame: date::now(),
            })
        },
    );
}
