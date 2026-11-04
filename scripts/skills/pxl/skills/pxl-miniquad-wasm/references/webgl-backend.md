# WebGL backend

The default is WebGL1 with GLSL ES 1.00 (`#version 100`). `conf::WebGLVersion::WebGL2` requests WebGL2 and permits GLES3 shaders and raw WebGL2 calls. Keep a WebGL1 shader path when WebGL1 matters.

Use `RenderingBackend` for buffers, textures, shaders, pipelines, passes, bindings, uniforms, and draw submission; see `rust-api.md` for every signature. Create textures with explicit dimensions and `TextureFormat::RGBA8`. Set min/mag filters to `FilterMode::Nearest` for pixel art and `MipmapFilterMode::None` when mipmaps are unnecessary.

`sample_count > 1` concerns render textures and MSAA; it is not a substitute for pixel scaling. Some renderbuffer/multisample imports are stubbed by `gl.js`, so feature-test instead of assuming every desktop GL path works. Check `ctx.info()` for backend and supported GLSL versions. Integer vertex attributes require suitable GL support. Read the current `webgl.rs` before using raw GL calls.

The default pass is the canvas framebuffer. Call `begin_default_pass`, apply pipeline and bindings, draw, call `end_render_pass`, then `commit_frame`. Keep alpha blending explicit in `PipelineParams` when drawing transparent sprites.
