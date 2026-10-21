# pxl

`pxl` is a deterministic pixel-art asset CLI. An agent (or a person) feeds it a palette, a shape program or a Bollinger mask, and a recipe. It writes palette-indexed documents, transform animations, tileable ground, an Aseprite-compatible spritesheet, and upscaled previews.

The same recipe, seed, and `pxl` version produce byte-identical outputs. Core crates do not touch the filesystem, threads, or the system clock, so they can move to WASM later. There is no UI in this tree.

The design notes live in [`plan/`](plan/). This repository implements the M0 CLI, the M1 tank demo, and the M2 paper-doll farmer.

## Layout

| path | role |
| --- | --- |
| `crates/pxl-core` | frames, palette, text matrices, SplitMix64 + PCG32. `no_std` + `alloc` |
| `crates/pxl-gen` | Bollinger masks, shape programs, periodic ground tiles |
| `crates/pxl-ops` | integer transforms, animation macros, paper-doll compose |
| `crates/pxl-qa` | `default`, `tank`, and `character` checks |
| `crates/pxl-format` | JSON documents and JSON Schema |
| `crates/pxl-export` | indexed PNG and GIF, no `image` crate |
| `crates/pxl-cli` | the `pxl` binary |
| `examples/tank` | tank recipe. `demo/tank` points here |
| `examples/farmer` | 16×32 paper-doll farmer, four facings, two recolors |
| `assets/` | built-in palettes and mask templates |

## Build

Rust 1.83 is enough. Newer clap releases need edition 2024, so this repo pins `clap` 4.5.

```bash
cargo build --release -p pxl-cli
cargo test
./target/release/pxl --help
```

The binary is `target/debug/pxl` after `cargo build -p pxl-cli`.

## Tank demo

```bash
pxl build examples/tank/tank.recipe.json -o examples/tank/out/
pxl validate examples/tank/out/tank.sheet.json --rules tank
```

`out/` is gitignored. A second build into the same directory overwrites those files and keeps the same sha256. The sheet has four directions, at least four move frames and two idle frames each, at least four dirt tiles, `tank.sheet.json` (Aseprite `frames` + `meta.frameTags`), `tank_move_E.gif`, and `preview.png`.

## Farmer

```bash
pxl build examples/farmer/farmer.recipe.json -o examples/farmer/out/
pxl validate examples/farmer/out/farmer.sheet.json --rules character
```

The farmer is original art assembled from parts (`part_transform`, `compose`, `doll`). The canvas is 16×32. Down, up, and left have their own sprites; right is a horizontal mirror of left. Walk is 4 frames with an arm swing, alternating feet, and a 1px head bob. Idle is a 2-frame breath. Hoe is 4 frames on the down facing and does not loop. `farmer_blonde` swaps hair to blonde and the shirt to red. `farmer_raven` swaps hair to black and the shirt to green. Both stay on the same palette so they pack into one sheet.

`--json` prints one JSON object on stdout. Exit codes are `0` success, `1` I/O, `2` usage, `3` schema, `4` semantic, `5` QA, `6` output already exists. Single-file commands do not overwrite unless you pass `--overwrite`.

A miniagent skill is at `examples/tank/.miniagent/skills/pxl.md` (also linked from `.miniagent/skills/pxl.md`). It tells an agent to call `pxl` with `--json`, to read `error.hint`, and to finish on `pxl validate`.

## Determinism

- PRNG is SplitMix64 mixed into PCG32. Seeds are explicit.
- Generators use integer arithmetic. The sine table used for ramps is a fixed `u8` lookup.
- PNG encoding is indexed color, Fast compression, no filter. GIF uses a fixed palette and delay.
- Outputs do not contain timestamps or wall-clock durations.

`cargo test` includes unit tests and a golden-hash file, `crates/pxl-cli/tests/golden/hashes.json`. Regenerate it only on purpose:

```bash
UPDATE_GOLDEN=1 cargo test -p pxl-cli --test golden
```

## License

MIT. See [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Bollinger behavior follows the MIT zfedoran/pixel-sprite-generator description. Outline brushes follow Pixelorama (MIT). Flip, rotate90, and shift follow Piskel (Apache-2.0). The unlicensed MaartenGr/Sprite-Generator port was not copied.
