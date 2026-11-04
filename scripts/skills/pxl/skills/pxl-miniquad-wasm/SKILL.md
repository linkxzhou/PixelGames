---
name: pxl-miniquad-wasm
description: Build and audit miniquad 0.4.6 wasm applications, including gl.js loading, plugins, shared memory, input, rendering, and browser pitfalls.
---

# Miniquad wasm

Use the vendored miniquad ABI directly; do not add wasm-bindgen.

## Minimum loop

Create `<canvas id="glcanvas">`, load `gl.js`, then call `load("app.wasm")`. In Rust, export `main`, call `miniquad::start`, and implement `EventHandler::update/draw`. Serve the directory over HTTP.

Build with `cargo build --release --target wasm32-unknown-unknown`. Validate the result:

```bash
python3 scripts/check_wasm_imports.py app.wasm --interface references/wasm-interface.json --html index.html
```

## Read the interface you need

- Loading and version checks: `references/loading.md`
- Standard wasm exports: `references/wasm-exports.md`
- JavaScript `env` imports: `references/js-imports.md`
- Plugin and shared-memory ABI: `references/plugins.md`
- Events, sapp codes, and modifiers: `references/events-and-keys.md`
- `Conf`, `EventHandler`, window, and `RenderingBackend`: `references/rust-api.md`
- WebGL constraints: `references/webgl-backend.md`
- LTO, memory growth, DPI, and file loading: `references/pitfalls.md`

Run the examples in `assets/examples/01-clear` through `07-file-loading`. Each is independent of `pxl-web` and wasm-bindgen. After changing miniquad, rerun both extraction scripts with `--check`; regenerate without `--check` when the source change is intentional.
