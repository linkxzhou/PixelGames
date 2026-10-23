---
name: macroquad
description: Use when implementing or verifying the Battle City macroquad runtime, including nearest-neighbour scaling, the fixed 60 Hz step, headless replay screenshots, and native or wasm32 builds.
---

# Macroquad for the Battle City slice

The playable game is the `battle-city` crate at `games/battle-city/runtime`. Macroquad is the window runtime. The rules live in `battle_city::sim` and do not call macroquad, so a headless replay can step the same world and write PNGs without opening a display.

## Window

- Open the window with `macroquad::Window::from_config`. Logical size is 320×240. The window is an integer multiple of that (3×).
- Upload the indexed frame as an RGBA `Image`, then `Texture2D::from_image`.
- Call `texture.set_filter(FilterMode::Nearest)` after every `texture.update`. Linear filtering blurs the pixels and fails the look the style guide asks for.
- Draw with `draw_texture_ex` and `dest_size` set to the full window so the scale stays a uniform nearest-neighbour stretch.
- Keyboard: arrows or WASD move, space shoots, 1/2/3 pick a stage, R restarts.

## Fixed step

- Simulation rate is 60 Hz. `const DT: f32 = 1.0 / 60.0`.
- Accumulate `get_frame_time()`, clamp the accumulator (a stall must not dump a huge burst), and call `Sim::step` once per `DT`.
- Do not move tanks or bullets from frame time directly. Headless mode does not read the clock at all: one loop iteration is one tick.

## Headless replay

The binary always accepts a replay, including the build that links macroquad:

```bash
cargo run -p battle-city -- --headless \
  --replay games/battle-city/runtime/replay/playtest.json \
  --out playtest-out --report playtest-out/report.json --scale 3
```

`--scale` writes an extra nearest-neighbour PNG (`frame_XXXX.xN.png`). The gate reads the 320×240 file named in `report.json`.

The report is one JSON object. A passing playtest has `panicked: false`, `frames` equal to the replay, `palette_mismatches: 0`, `player_moved: true`, `shots >= 1`, and `bricks_destroyed` or `enemies_destroyed` at least 1. `state_hash` is a pure function of the tick, the grid, and the bullets. Two runs of the same replay must match.

Default playtest (`runtime/replay/playtest.json`) is stage 2 for 120 ticks: drive east, drive back west, face north, and shoot. The brick above the spawn is destroyed.

## Builds

From the workspace root:

```bash
cargo build -p battle-city
cargo build -p battle-city --target wasm32-unknown-unknown
cargo test -p battle-city
```

Macroquad is pinned to 0.4.13 with default features off (no audio device). The `window` feature is on by default. `--no-default-features` builds the headless binary only. wasm32 uses the `window` feature and does not take replay arguments; the browser entry opens the same fixed-step loop.

Rust 1.83 cannot compile `fontdue` 0.9 (`integer_sign_cast`). Do not bump macroquad past 0.4.13 without pinning an older `fontdue`.

`cargo run -p battle-city` (no `--headless`) is the windowed path. macroquad 0.4.13 links miniquad 0.4.6, whose macOS startup called `center` before `display.view` was stored. AppKit delivers `windowDidChangeScreen` from `center`, `update_dimensions` sent `-bounds` to a nil view, and Apple Silicon aborted with `null pointer dereference occurred` at `miniquad-0.4.6/src/native/macos.rs:168`. The workspace patches that crate from `vendor/miniquad` (same 0.4.6, view stored before `center`, nil view and nil GL context ignored). Leave the patch in place. Headless replay does not open a window and is unchanged.

## What the harness checks

`pxl-harness run --until playtest` runs, in order: approved design, assets, `compile.native`, `compile.wasm32`, then the playtest gates (`frames`, `palette`, `player_moves`, `shoots`, `destroys`, `screenshot`). `--from build` starts at the compile gates when asset authoring is not part of this invocation. The macroquad skill must load from `harness/skills/macroquad/SKILL.md` before those gates run.
