# pxl-engine (libs/pxl-engine)

For runtime games, do not confuse this build-time renderer with `steps/step-500-engine/web-engine` (`pxl-web`). `pxl-web` is a separate miniquad wasm kernel for JavaScript game logic, with batched drawing and browser input. It does not depend on this crate or macroquad.

Never touches the filesystem, stdin, or clock. Files a spec refers to (`legend_ref`, palette files) come through a `Resolver`, so the same code runs against a directory (CLI) or an in-memory table (web).

## API

- `render(&RenderRequest, &dyn Resolver) -> RenderOutput { format, bytes, width, height, frame_count, native_width, native_height, scale, grid, purpose, anim, info, hash }`. `hash` is `sha256:<hex>` of the bytes.
- `inspect(&RenderRequest, &Resolver) -> DocInfo` parses and assembles without encoding; `assemble`, `describe`, `detect_kind`.
- `RenderRequest { input, kind: InputKind, palette, charset, size, cell, anim, dir, scale, seed, format: FormatChoice, grid, purpose: OutputPurpose }`; defaults: `kind: Auto`, `size: 32`, `scale: 8`, `seed: 1`, `Preview`. Use `..Default::default()`.
- `InputKind`: `Auto`, `AsciiText`, `AsciiJson`, `ShapesJson` (has `draw`), `DocJson` (has `w` and array frames).
- Output choice: `Auto` gives GIF when an animation is drawn, otherwise PNG. A document without animation and an explicit GIF request is an error. `OutputPurpose::Asset` keeps transparency and draws no grid.
- `MemoryResolver::new()` / `insert(name, text)` looks up by exact name, then by final path component. `MAX_INPUT_BYTES = 256 KiB`; also `check_anim_timing`, `MAX_ANIM_FPS`.
- Defaults: palette `DEFAULT_PALETTE`; ASCII text charset is guessed (symbols like `.#$` pick `printable`).

## Rules when editing

- No `std::fs`, `std::env`, or time here; add a `Resolver` method instead.
- Keep native tests in `tests/examples.rs` in step with the web examples; they pin PNG vs GIF choice and sizes.

## Example: ASCII grid to a PNG

```rust
use pxl_engine::{render, MemoryResolver, RenderRequest};

fn main() -> pxl_core::PxlResult<()> {
    let out = render(
        &RenderRequest {
            input: ":: base\n..##..\n.#$$#.\n#$$$$#\n#$$$$#\n.#$$#.\n..##..\n",
            palette: Some("p16"),
            scale: 8,
            ..Default::default()
        },
        &MemoryResolver::new(),
    )?;
    println!("{:?} {}x{} {}", out.format, out.width, out.height, out.hash);
    Ok(())
}
```

Output (verified): `Png 256x256 sha256:d349...8175`. The 6x6 art sits on the default 32x32 canvas, scaled 8x. Write `out.bytes` to a `.png` file to view it.

Run tests: `cargo test -p pxl-engine`.
