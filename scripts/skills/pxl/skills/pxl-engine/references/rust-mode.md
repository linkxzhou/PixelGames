# Pure Rust mode

Use the existing `games/<name>/runtime` implementation and the macroquad agent skill at `libs/pxl-agent/skills/macroquad/SKILL.md`; do not migrate code to `web/` and do not generate `work/500-engine/web/`.

For battle-city:

```bash
cargo build -p battle-city-runtime
cargo test -p battle-city-runtime
cargo build --target wasm32-unknown-unknown -p battle-city-runtime
```

Keep the gates `compile.native` and `compile.wasm32`, plus the existing native headless replay gates. Rust mode does not need Node or the JS browser runtime.

Macroquad remains pinned to 0.4.13 and miniquad to the workspace `vendor/miniquad` patch. The patch preserves the macOS initialization-order fix and now also supplies the small WebGL renderbuffer hooks used by wasm ABI checking. Do not remove `[patch.crates-io]`.

`step-500 build --mode rust` deliberately points back to this cargo path; step-500's static bundler is JS-only.
