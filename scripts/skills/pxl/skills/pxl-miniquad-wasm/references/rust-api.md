# Rust API

## Contents

- Entry and configuration
- EventHandler
- RenderingBackend

Use `miniquad::start(conf, || Box::new(handler))`. `Conf` contains `window_title`, `window_width`, `window_height`, `high_dpi`, `fullscreen`, `sample_count`, `window_resizable`, `icon`, and `platform`.

## EventHandler

- `fn update(&mut self)`
- `fn draw(&mut self)`
- `fn resize_event(&mut self, _width: f32, _height: f32)`
- `fn mouse_motion_event(&mut self, _x: f32, _y: f32)`
- `fn mouse_wheel_event(&mut self, _x: f32, _y: f32)`
- `fn mouse_button_down_event(&mut self, _button: MouseButton, _x: f32, _y: f32)`
- `fn mouse_button_up_event(&mut self, _button: MouseButton, _x: f32, _y: f32)`
- `fn char_event(&mut self, _character: char, _keymods: KeyMods, _repeat: bool)`
- `fn key_down_event(&mut self, _keycode: KeyCode, _keymods: KeyMods, _repeat: bool)`
- `fn key_up_event(&mut self, _keycode: KeyCode, _keymods: KeyMods)`
- `fn touch_event(&mut self, phase: TouchPhase, _id: u64, x: f32, y: f32)`
- `fn raw_mouse_motion(&mut self, _dx: f32, _dy: f32)`
- `fn window_minimized_event(&mut self)`
- `fn window_restored_event(&mut self)`
- `fn quit_requested_event(&mut self)`
- `fn files_dropped_event(&mut self)`

## RenderingBackend

- `fn info(&self) -> ContextInfo`
- `fn new_shader( &mut self, shader: ShaderSource, meta: ShaderMeta, ) -> Result<ShaderId, ShaderError>`
- `fn new_texture( &mut self, access: TextureAccess, data: TextureSource, params: TextureParams, ) -> TextureId`
- `fn new_render_texture(&mut self, params: TextureParams) -> TextureId`
- `fn new_texture_from_data_and_format( &mut self, bytes: &[u8], params: TextureParams, ) -> TextureId`
- `fn new_texture_from_rgba8(&mut self, width: u16, height: u16, bytes: &[u8]) -> TextureId`
- `fn texture_params(&self, texture: TextureId) -> TextureParams`
- `fn texture_size(&self, texture: TextureId) -> (u32, u32)`
- `fn texture_raw_id(&self, texture: TextureId) -> RawId`
- `fn texture_update(&mut self, texture: TextureId, bytes: &[u8])`
- `fn texture_set_filter( &mut self, texture: TextureId, filter: FilterMode, mipmap_filter: MipmapFilterMode, )`
- `fn texture_set_min_filter( &mut self, texture: TextureId, filter: FilterMode, mipmap_filter: MipmapFilterMode, )`
- `fn texture_set_mag_filter(&mut self, texture: TextureId, filter: FilterMode)`
- `fn texture_set_wrap(&mut self, texture: TextureId, wrap_x: TextureWrap, wrap_y: TextureWrap)`
- `fn texture_generate_mipmaps(&mut self, texture: TextureId)`
- `fn texture_resize(&mut self, texture: TextureId, width: u32, height: u32, bytes: Option<&[u8]>)`
- `fn texture_read_pixels(&mut self, texture: TextureId, bytes: &mut [u8])`
- `fn texture_update_part( &mut self, texture: TextureId, x_offset: i32, y_offset: i32, width: i32, height: i32, bytes: &[u8], )`
- `fn new_render_pass( &mut self, color_img: TextureId, depth_img: Option<TextureId>, ) -> RenderPass`
- `fn new_render_pass_mrt( &mut self, color_img: &[TextureId], resolve_img: Option<&[TextureId]>, depth_img: Option<TextureId>, ) -> RenderPass`
- `fn render_pass_texture(&self, render_pass: RenderPass) -> TextureId`
- `fn render_pass_color_attachments(&self, render_pass: RenderPass) -> &[TextureId]`
- `fn delete_render_pass(&mut self, render_pass: RenderPass)`
- `fn new_pipeline( &mut self, buffer_layout: &[BufferLayout], attributes: &[VertexAttribute], shader: ShaderId, params: PipelineParams, ) -> Pipeline`
- `fn apply_pipeline(&mut self, pipeline: &Pipeline)`
- `fn delete_pipeline(&mut self, pipeline: Pipeline)`
- `fn new_buffer(&mut self, type_: BufferType, usage: BufferUsage, data: BufferSource) -> BufferId`
- `fn buffer_update(&mut self, buffer: BufferId, data: BufferSource)`
- `fn buffer_size(&mut self, buffer: BufferId) -> usize`
- `fn delete_buffer(&mut self, buffer: BufferId)`
- `fn delete_texture(&mut self, texture: TextureId)`
- `fn delete_shader(&mut self, program: ShaderId)`
- `fn apply_viewport(&mut self, x: i32, y: i32, w: i32, h: i32)`
- `fn apply_scissor_rect(&mut self, x: i32, y: i32, w: i32, h: i32)`
- `fn apply_bindings_from_slice( &mut self, vertex_buffers: &[BufferId], index_buffer: BufferId, textures: &[TextureId], )`
- `fn apply_bindings(&mut self, bindings: &Bindings)`
- `fn apply_uniforms(&mut self, uniforms: UniformsSource)`
- `fn apply_uniforms_from_bytes(&mut self, uniform_ptr: *const u8, size: usize)`
- `fn clear( &mut self, color: Option<(f32, f32, f32, f32)>, depth: Option<f32>, stencil: Option<i32>, )`
- `fn begin_default_pass(&mut self, action: PassAction)`
- `fn begin_pass(&mut self, pass: Option<RenderPass>, action: PassAction)`
- `fn end_render_pass(&mut self)`
- `fn commit_frame(&mut self)`
- `fn draw(&self, base_element: i32, num_elements: i32, num_instances: i32)`

Window functions are `screen_size`, `dpi_scale`, `high_dpi`, `blocking_event_loop`, `order_quit`, `quit`, `request_quit`, `cancel_quit`, `set_cursor_grab`, `schedule_update`, `show_mouse`, `set_mouse_cursor`, `set_window_size`, `set_window_position`, `set_fullscreen`, `clipboard_get/set`, `dropped_file_count/bytes/path`, `show_keyboard`, and `new_rendering_backend`. Apple-specific functions are unavailable on wasm.
