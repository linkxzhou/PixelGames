---
name: pxl
description: Run the PixelGames pixel-art asset pipeline end to end with bundled step-100..step-500 binaries (macOS, Linux, Windows launchers) and route to a stage sub-skill. Use when creating, building, validating or packaging pixel-art sprites, tiles, animations or spritesheets; when a request mentions step-100, step-200, step-300, step-400, step-500, the pxl CLI, recipe.json, *.pxl.json, brief to asset-package, art specs, QA gates or engine intake; or when reading or changing the pxl-* Rust crates.
---

# pxl

Five-stage pipeline: brief -> design -> art-spec -> asset-build -> package -> engine (miniquad wasm + JS, optional pure Rust). Stages 100 and 200 call a model (fake provider by default, offline); 300, 400 and 500 are deterministic. Text recipes produce pixels and optional ZzFX SFX / BeepBox BGM; browsers play pre-rendered WAV, never the Node synthesizers.

## Run binaries only through the launcher

```bash
scripts/run.sh <step-100|step-200|step-300|step-400|step-500> [args...]   # macOS, Linux
scripts\run.ps1 <step-N> [args...]                                        # Windows
scripts/run.sh --platform | --verify | --dev <step-N> ...
```

Paths are relative to this skill's directory. The launcher picks `bin/<platform>/`, puts it on `PATH`, and passes `--pxl-bin` to `step-200`. Do not assume `cargo` or a system `PATH`. If it reports a missing platform, read [references/troubleshooting.md](references/troubleshooting.md).

## Route by intent

| Intent | Sub-skill (read its SKILL.md) |
| --- | --- |
| Brief to design package, visual intent, approval, or lesson analysis | [skills/pxl-design](skills/pxl-design/SKILL.md) |
| Requirements to SVG / ASCII / recipe specs, lint specs | [skills/pxl-art-spec](skills/pxl-art-spec/SKILL.md) |
| Generate, transform, animate, pack, render, validate pixels; build recipes | [skills/pxl-asset-build](skills/pxl-asset-build/SKILL.md) |
| Coverage check, content-addressed package, package approval | [skills/pxl-package](skills/pxl-package/SKILL.md) |
| Build a browser-playable game, run headless replay, or give a slice verdict | [skills/pxl-engine](skills/pxl-engine/SKILL.md) |
| Miniquad wasm interfaces, plugins, and runnable examples | [skills/pxl-miniquad-wasm](skills/pxl-miniquad-wasm/SKILL.md) |
| Read or change a `pxl-*` crate | [skills/pxl-crates-dev](skills/pxl-crates-dev/SKILL.md) |

Workspace layout, handoff files and contracts: [references/pipeline.md](references/pipeline.md).

## Rules that apply to every stage

1. All pixels come from `step-300`. Never write or edit pixels with Python or image libraries.
2. Exit 10 means a human approval is pending. Stop and ask the user to run `approve --digest <manifest_digest>` (from `revision.manifest.json`). Do not run `approve` yourself unless the user explicitly tells you to.
3. Verdicts are four-valued: `FAIL > CONCERNS > NOT_ASSESSED > PASS`. A check that could not run is `NOT_ASSESSED`, never `PASS`. Report it that way.
4. Default provider is `fake` (offline, uses `STEP_100_FAKE_TURNS` / `STEP_200_FAKE_TURNS` transcripts). A real model needs `--provider openai --env <file>` supplied by the user. Never invent, read or store credentials.
5. Existing outputs are not replaced without `--overwrite`. Failed model output lands in `.rejected`; do not move it into the current directory.
6. Print a step's contract with `step-100|200|400|500 --contract`; for step-300 use `schema <kind>`. Trust `--help` over prose docs.
7. Exit codes: step-300 uses 0 ok, 1 I/O, 2 usage, 3 schema, 4 semantic, 5 QA, 6 output exists. The pipeline steps use 10 waiting approval, 11 blocked/policy, 12 gates failed, 13 budget, 14 lock held, 15 integrity, 16 tool/provider. Retry a failing call at most 3 times, then report.
8. Before implementing game logic, say “默认使用 JS 实现，浏览器可直接打开；如需纯 Rust 实现请说明” and ask for the mode. If the user does not choose, continue in JS; never silently switch a workspace's recorded `runtime.mode`.
9. In JS mode write gameplay only in `games/<name>/web/*.js`; pixels still come only from step-300.
10. Never claim browser verification passed unless `playtest browser` actually ran and passed.

## Prototype library status

The reuse workflow (search, pin, expand, lock, package) is implemented in the Rust sources of steps 200, 300 and 400 and in `steps/step-500-engine/runtime-js/scene-select.js`. The bundled binaries under `bin/` are **not rebuilt** by this change (the manifest still lists `darwin-arm64` only), so `--library`, `prototype` and `--catalog-lock` are available from a source build, not from the shipped launcher, until the binaries are regenerated. Check with `scripts/run.sh step-300 prototype --help` and trust that over this page.

## Smoke test (offline, about 10 seconds)

```bash
scripts/run.sh step-300 gen bollinger --seed 7 --template slime_8x8 --palette p16 --ramp slime --mirror x -o /tmp/slime.pxl.json --json
scripts/run.sh step-300 validate /tmp/slime.pxl.json --json     # exit 0, "ok":true
```
