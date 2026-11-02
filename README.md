# pxl

The repo is a five-step game-asset pipeline. Each step owns one CLI, reads only the previous step's directory, and writes its own:

```
step-100-design -> step-200-art-spec -> step-300-asset-build -> step-400-package -> step-500-engine
  brief + design      SVG / ASCII / JSON     pixels, sheets, WAV     asset-package        engine intake
  (LLM)               (LLM)                  (deterministic)         (deterministic)      (miniquad wasm + JS / Rust)
```

Only steps 100 and 200 call a model. Step 200 writes specs and never builds; step 300 deterministically renders pixel assets and optional ZzFX SFX / BeepBox chiptune BGM from text specs. Audio uses a version-locked Node 22+ adapter offline and produces 44.1 kHz signed 16-bit WAV plus PCM/file digests; no music model or runtime synthesizer is shipped in games. See [`steps/README.md`](steps/README.md) and the plan in [`plan/v3.0/pxl-step-pipeline-plan.md`](plan/v3.0/pxl-step-pipeline-plan.md).

The v6 visual pipeline freezes `visual-intent/v1` at design time and `visual-plan/v1` at spec time. Named material/protect regions, strict palettes, real frame durations, standard previews and channel digests flow through packaging into a generic no-game showcase with L0/L1/L2 rendering. Visual approval requires current evidence and two independent reviewers; missing human review or real-model A/B remains `NOT_ASSESSED` rather than a cosmetic PASS.

The same recipe, seed, and `pxl` version produce byte-identical outputs. Core crates do not touch the filesystem, threads, or the system clock, so they can move to WASM later. There is no UI in this tree.

The design notes live in [`plan/`](plan/). This repository implements the M0 CLI, the M1 tank demo, and the M2 paper-doll farmer.

## Layout

| path | role |
| --- | --- |
| `steps/step-100-design` | brief -> design package, designer loop, gates, approval |
| `steps/step-200-art-spec` | requirements -> SVG/ASCII/shapes specs, **never builds** |
| `steps/step-300-asset-build` | the pxl crate tree; bin `step-300`, `plan`/`build`/`qa`/... |
| `steps/step-400-package` | coverage, content addressing, `asset-package.json` |
| `steps/step-500-engine` | engine/v4 visual/audio checks, miniquad wasm + JS browser builds, generic showcase, fixed-frame visual playtests, and the slice gate |
| `libs/pxl-agent` | model provider loop, tools, policy, MCP (steps 100/200 only) |
| `libs/pxl-store` | digests, revisions, event log |
| `libs/pxl-contract` | `step.json` envelope, verdicts, exit codes |
| `steps/contracts/` | JSON Schema per step |
| `examples/tank` | tank recipe. `demo/tank` points here |
| `examples/farmer` | 16×32 paper-doll farmer, four facings, two recolors |
| `assets/` | built-in palettes and mask templates |

Inside `steps/step-300-asset-build/`:

| path | role |
| --- | --- |
| `pxl-core` | frames, palette, text matrices, SplitMix64 + PCG32. `no_std` + `alloc` |
| `pxl-gen` | Bollinger masks, shape programs, periodic ground tiles |
| `pxl-ops` | integer transforms, animation macros, paper-doll compose |
| `pxl-qa` | `default`, `tank`, and `character` checks |
| `pxl-format` | JSON documents and JSON Schema |
| `pxl-export` | indexed PNG and GIF, no `image` crate |
| `pxl-audio` | text-spec validation, sandboxed Node rendering, WAV, QA and audio manifests |
| `pxl-cli` | the `step-300` binary |

## Build

Rust 1.83 is enough. Newer clap releases need edition 2024, so this repo pins `clap` 4.5.

```bash
cargo build --release -p pxl-cli
cargo test
./target/release/step-300 --help
```

The binary is `target/debug/step-300` after `cargo build -p pxl-cli`. Every
step also answers `--contract` with its JSON Schema.

## Browser game (battle-city)

```bash
./build.sh web-engine
./build.sh web-test
cargo build -p step-500-engine
./target/debug/step-500 build --in games/battle-city/work/400-package \
  --game games/battle-city/web --out work/500-engine
./target/debug/step-500 playtest --in work/500-engine --mode all
python3 -m http.server 8000 --directory work/500-engine/web
```

Open `http://localhost:8000`. The page needs no bundler or Rust toolchain after the runtime is built. Use **Enable sound** after a real user gesture to unlock Web Audio; battle-city includes three SFX and one looping chiptune BGM. Headless replay compares both simulation and audio-event hashes. The existing macroquad implementation remains available as pure Rust mode without audio playback; other games have not been migrated to JS.

Render an audio spec independently with `step-300 audio validate <spec>`, `step-300 audio render <spec> -o <dir>`, and `step-300 audio inspect <dir>`. `step-300 plan` includes audio automatically when `spec-manifest.json` declares it.

## Tank demo

```bash
step-300 build examples/tank/tank.recipe.json -o examples/tank/out/
step-300 validate examples/tank/out/tank.sheet.json --rules default
```

`out/` is gitignored. A second build into the same directory overwrites those files and keeps the same sha256. The sheet has four directions, at least four move frames and two idle frames each, at least four dirt tiles, `tank.sheet.json` (Aseprite `frames` + `meta.frameTags`), `tank_move_E.gif`, and `preview.png`.

## Farmer

```bash
step-300 build examples/farmer/farmer.recipe.json -o examples/farmer/out/
step-300 validate examples/farmer/out/farmer.sheet.json --rules default
```

The farmer is original art assembled from parts (`part_transform`, `compose`, `doll`). The canvas is 16×32. Down, up, and left have their own sprites; right is a horizontal mirror of left. Walk is 4 frames with an arm swing, alternating feet, and a 1px head bob. Idle is a 2-frame breath. Hoe is 4 frames on the down facing and does not loop. `farmer_blonde` swaps hair to blonde and the shirt to red. `farmer_raven` swaps hair to black and the shirt to green. Both stay on the same palette so they pack into one sheet.

`--json` prints one JSON object on stdout. Exit codes are `0` success, `1` I/O, `2` usage, `3` schema, `4` semantic, `5` QA, `6` output already exists. Single-file commands do not overwrite unless you pass `--overwrite`.

A miniagent skill is at `examples/tank/.miniagent/skills/pxl.md` (also linked from `.miniagent/skills/pxl.md`). It tells an agent to call `step-300` with `--json`, to read `error.hint`, and to finish on `step-300 validate`.

## Determinism

- PRNG is SplitMix64 mixed into PCG32. Seeds are explicit.
- Core generators use integer arithmetic. SVG geometry uses the shared implementation and does not yet carry a cross-platform bitwise guarantee.
- PNG encoding is indexed color, Fast compression, no filter. GIF uses a fixed palette and delay.
- Outputs do not contain timestamps or wall-clock durations.
- `step-300 plan --in <spec-tree> --out <audit-dir> --baseline` builds each recipe twice and compares indexed frames, manifests and encoded bytes. Visual review, model robustness, Linux and WASM remain `not_run` unless separately verified.

`cargo test` includes unit tests and a golden-hash file, `libs/pxl-cli/tests/golden/hashes.json`. Regenerate it only on purpose:

```bash
UPDATE_GOLDEN=1 cargo test -p pxl-cli --test golden
```

## License

MIT. See [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Bollinger behavior follows the MIT zfedoran/pixel-sprite-generator description. Outline brushes follow Pixelorama (MIT). Flip, rotate90, and shift follow Piskel (Apache-2.0). Offline audio vendors ZzFX and BeepBox (both MIT). The unlicensed MaartenGr/Sprite-Generator port was not copied.
