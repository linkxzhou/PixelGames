# pxl-wasm (libs/pxl-wasm)

Thin layer over `pxl-engine`. Logic lives in plain Rust (`run_render`, `run_inspect`) so it can be tested natively; the `#[wasm_bindgen]` functions only convert types. Options go in as a JSON string; results come back as JS values.

## JS surface

- `render(input: string, optionsJson: string) -> RenderResult` with getters `bytes` (Uint8Array), `format` (`"png"`/`"gif"`), `metaJson` (`{ width, height, frameCount, nativeWidth, nativeHeight, scale, sampling, grid, purpose, anim, hash, info }`). Call `.free()` after reading.
- `inspect(input, optionsJson)` returns `{ kind, width, height, frames, anims, palette, warnings }`.
- `version()`, `maxInputBytes()`.
- Failures throw `{ code, message, path, hint }`.

## Options JSON (all optional, unknown keys rejected)

`kind` (`auto|ascii|ascii-json|shapes|doc`), `palette`, `charset` (`digits|printable`), `size`, `cell`, `anim`, `dir`, `scale`, `seed`, `format` (`auto|png|gif`), `grid`, `purpose` (`preview|asset`), `files` (name to text map for `legend_ref` and palettes).

## Build

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version <pin in libs/pxl-wasm/Cargo.toml> --locked
scripts/build-wasm.sh          # release; --dev for a debug build
```

Output goes to `web/src/wasm/` (`pxl_wasm.js`, `pxl_wasm_bg.wasm`, `.d.ts`). The CLI version must equal the pinned `wasm-bindgen` crate version. The web app calls it from `web/src/lib/pxl.worker.ts`; use `yarn wasm` inside `web/`.

## Rules when editing

- Keep the crate free of `js-sys`/`serde-wasm-bindgen`; add behaviour to `pxl-engine` and expose it via a plain Rust function first.
- A new option needs a field on `Options` (with `#[serde(default)]`), the mapping into `RenderRequest`, and an update to `web/src/types.ts`.

## Example: render in the browser

```ts
import init, { render } from '@/wasm/pxl_wasm.js'

await init()
const grid = '..##..\n.#$$#.\n#$$$$#\n#$$$$#\n.#$$#.\n..##..\n'
const out = render(grid, JSON.stringify({ palette: 'p16', scale: 8, purpose: 'asset' }))
const meta = JSON.parse(out.metaJson)
const url = URL.createObjectURL(new Blob([out.bytes], { type: `image/${out.format}` }))
out.free()
console.log(meta.width, meta.height, meta.hash, url)
```

The same logic checked natively (verified) via `pxl_wasm::run_render(grid, opts)`: kind `ascii`, 32x32 doc, PNG output of 9233 bytes with a `sha256:` hash.

Run tests: `cargo test -p pxl-wasm`.

## pxl-wasm vs pxl-web

`pxl-wasm` is the build-time asset tool above: it uses wasm-bindgen and writes `web/src/wasm`. `pxl-web` (`steps/step-500-engine/web-engine`) is the runtime game kernel: it uses miniquad's `gl.js`, not wasm-bindgen, and produces `pxl_web.wasm` through `scripts/build-web-engine.sh`. Never combine their JS glue or put gameplay into `pxl-wasm`.
