---
name: pxl-engine
description: step-500. Build browser games with miniquad wasm and JavaScript, play packaged WAV with Web Audio, run deterministic headless/browser playtests, validate engine/v4, or use the legacy pure-Rust runtime.
---

# pxl-engine

step-500 is deterministic and has no model. Call it as `scripts/run.sh step-500 ...` from the `pxl` skill directory.

## Choose and record the mode first

Before implementing game logic, tell the user: “默认使用 JS 实现，浏览器可直接打开；如需纯 Rust 实现请说明” and ask whether to use pure Rust. If there is no answer, use JS. Record `runtime.mode` as `js` or `rust` in `pipeline.json` or `engine.json`; do not silently switch an existing workspace. To change modes, create a new workspace.

- **JS (default):** gameplay lives in `games/<name>/web/*.js`; `pxl_web.wasm` owns rendering, input, and the frame loop. Read [references/runtime-api.md](references/runtime-api.md) and [references/game-template.md](references/game-template.md).
- **Rust:** keep the existing `games/<name>/runtime` macroquad path. Read [references/rust-mode.md](references/rust-mode.md) and `libs/pxl-agent/skills/macroquad/SKILL.md`. Do not produce `work/500-engine/web/`.

## JS build and playtest

```bash
scripts/run.sh step-500 build --in <400-package> --game games/<name>/web --out work/500-engine
scripts/run.sh step-500 build --in <400-package> --out work/500-engine   # no game: generic visual showcase
scripts/run.sh step-500 playtest --in work/500-engine --mode headless
scripts/run.sh step-500 playtest --in work/500-engine --mode visual
scripts/run.sh step-500 playtest --in work/500-engine --mode all
scripts/run.sh serve work/500-engine/web --port 8000
```

`build` validates the package layout, copies prebuilt runtime files and game JS, creates a self-contained `assets/`, and writes `engine-build.json` with every file's sha256. It never calls cargo. Existing output requires `--overwrite`; failed output stays in `.rejected`.

`playtest headless` needs Node and runs the replay twice; `state_hash` must match across JS runs. `playtest visual` needs Python Playwright and Chromium, captures fixed-tick logical and presentation frames at DPR 1/2, and exercises L0/L1/L2 scene rendering and capability negotiation. Missing Node, browser, or wasm runtime is `NOT_ASSESSED`, never `PASS`. `all` also returns `NOT_ASSESSED` when one layer cannot run.

## Intake and slice

```bash
scripts/run.sh step-500 check --in <400-package> [--out <dir>] [--scope all|slice] [--engine engine.json]
scripts/run.sh step-500 slice --work <workspace> --out <dir>
scripts/run.sh step-500 contract
```

`check` runs seven checks: the six visual/contract checks plus `audio_capability`. The default contract is `engine/v4` with command ABI v2; older engines and unsupported required capabilities are rejected. JS mode plays packaged WAV through Web Audio. Rust mode rejects required audio and warns on optional audio instead of silently ignoring it.

`build` copies audio to `assets/audio/` and records WAV and decoded-byte summaries. Games consume only stable audio intentions; call `ctx.audio.preload`, unlock with the page's sound button, then use `consume`/`playSfx`/`playBgm`. Node headless uses the recording null player and compares `audio_events_hash` as well as `state_hash`. Browser playback and human listening are separate verdicts; an unrun browser check or listening review is `NOT_ASSESSED`.

`slice` requires a passing playtest whose `build_digest` matches the current `engine-build.json`. Missing or stale evidence is `NOT_ASSESSED`. On `PROCEED`, `next` is `step-200 run --scope all`; do not start the full run on any other verdict.
