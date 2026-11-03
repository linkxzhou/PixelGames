---
name: web-engine
description: Use when implementing or verifying JavaScript-mode games with the pxl-web miniquad wasm kernel, fixed 60 Hz logic, asset packages, headless replay, and browser playtests.
---

# Web engine for JavaScript games

Keep gameplay in `games/<name>/web/*.js`. The Rust `pxl-web` kernel owns miniquad's requestAnimationFrame loop, input, textures, nearest-neighbor projection, and batched WebGL drawing. Do not add macroquad or wasm-bindgen to this path.

## Rules

- Run simulation at 60 Hz through `@pxl/runtime`; simulation modules do not touch the DOM or read a wall clock.
- Load only the shared asset-package. Pixels still come from step-300.
- Upload sheets once, then submit one fixed-stride command buffer per render frame. Do not make one wasm call per sprite.
- Use `build.web`, `playtest.headless`, and `playtest.browser` as JS-mode gate names. Browser may be `NOT_ASSESSED`; never report it as passed without an actual browser run.
- Compare JS state hashes only with another JS run of the same replay. Compare Rust and JS by semantic gameplay metrics, not bit-level f32 behavior.

## Commands

```bash
step-500 build --in <400-package> --game games/<name>/web --out work/500-engine
step-500 playtest --in work/500-engine --mode headless
step-500 playtest --in work/500-engine --mode browser
step-500 playtest --in work/500-engine --mode all
```

The build consumes prebuilt `pxl_web.wasm`; it does not invoke cargo. Rebuild the kernel with `scripts/build-web-engine.sh` only when the engine or runtime changes. See `games/battle-city/web` for the reference migration.
