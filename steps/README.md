# steps

The pipeline. The top level holds **exactly five big steps**; every smaller
action is a subcommand inside one of them. See
[`plan/v3.0/pxl-step-pipeline-plan.md`](../plan/v3.0/pxl-step-pipeline-plan.md)
for the reasoning and the source-of-inspiration map.

```
step-100-design ─→ step-200-art-spec ─→ step-300-asset-build ─→ step-400-package ─→ step-500-engine
     design             art-spec            asset-build             package            engine
      (LLM)              (LLM)             (bin from libs/pxl-cli)       (deterministic)    (miniquad wasm + JS / Rust)
```

## Structure rules

1. `steps/` admits **only big steps** at the top level: one step = one stage =
   one bin. Sub-steps, roles, templates, and internal modules live inside the
   step directory.
2. `step-300-asset-build/` is the asset-build stage directory. Reusable `pxl-*`
   crates now live under `libs/` (including `pxl-cli`, whose binary is still
   named `step-300`). This directory keeps non-crate stage adjuncts such as
   `audio-adapter/` and docs.
3. Code that two or more steps depend on and that has no pipeline meaning of
   its own lives in `libs/`. **A library is never called a step.** The former
   step-300 `pxl-*` tree belongs here.

## The five steps

| step | bin | does | LLM | subcommands | input → output |
|---|---|---|---|---|---|
| step-100-design | `step-100` | brief → design/v3 + visual intent, gates and approval | yes | `init` `run` `gate` `approve` `status` `lessons` | `brief.json` → `work/100-design/` |
| step-200-art-spec | `step-200` | requirements → SVG/ASCII/JSON specs | yes | `run` `lint` `capabilities` `status` | `work/100-design/` → `work/200-art-spec/` |
| step-300-asset-build | `step-300` | specs → PNG/GIF/sheets/WAV (the `pxl` crates) | no | `plan` `build` `render` `sheet` `audio` `gen` `op` `anim` `palette` `tile` `inspect` `profile` `schema` `list` `enhance` `pixelize` `ase` | `work/200-art-spec/` → `work/300-build/` |
| step-400-package | `step-400` | visual/audio build outputs → asset-package | no | `run` `check` `approve` `status` | `work/300-build/` → `work/400-package/` |
| step-500-engine | `step-500` | asset-package → browser bundle + playtest | no | `check` `slice` `build` `playtest` | `work/400-package/` → `work/500-engine/` |

JS mode builds a static `web/` directory from prebuilt miniquad wasm and runs deterministic Node replay plus optional Chromium acceptance. `step-500 build` without `--game` creates a generic asset showcase; `step-500 playtest --mode visual` captures fixed-tick logical and presentation frames at DPR 1/2, with L0/L1/L2 rendering selected by the frozen scene. Pre-rendered WAV uses Web Audio after a user gesture; pure Rust mode keeps the existing macroquad runtime and rejects required audio. Missing Node or browser evidence is `NOT_ASSESSED`, never a pass.

## Contracts

`step-300 ase`（v8.0）提供 `.aseprite` inspect/convert(PNG|GIF)/sheet/list-layers|tags|slices/split-slices/filter/op/import 子集（含 flood-fill、hue-saturation、sheet padding/extrude/trim、`--crop`/`--merge-duplicates`）；详见 `plan/v8.0/`。

Every step prints its contract with `--contract`:

```sh
step-100 --contract     # design.schema.json
step-200 --contract     # art-spec.schema.json
step-300 schema <kind>  # the pxl-format schemas (unchanged)
step-400 --contract     # asset-package.schema.json
step-500 --contract     # engine.schema.json
```

The shared envelope every step writes as `step.json` is
[`contracts/step.schema.json`](contracts/step.schema.json). The workspace-level
lock file `pipeline.json` (written by `step-100 init`) follows
[`contracts/pipeline.schema.json`](contracts/pipeline.schema.json).

The single production contract family is `design/v3`, `art-spec/v3`, `asset-build/v4`, `asset-package/v3`, `engine/v4` and `pipeline/v2`; older packages and migration commands are rejected rather than silently adapted. Optional audio keeps independent `audio-requirements/v1`, `audio-spec/v1` and `audio-manifest/v1` contracts. Step-100 requires an approved `visual-intent.json` before step-200. Step-200 freezes that intent and writes `visual-plan.json` plus an immutable `knowledge.lock.json`; it never exports pixels or PCM. Step-300 resolves effective styles, named material/protect regions and per-frame durations, then emits effective-style, visual reports/metadata, standard previews and optional material/emissive/shadow channels. Step-400 validates the complete digest closure and requires a current `visual-review/v1` with two independent reviewers before approval. Accepted CONCERNS remain CONCERNS.

`step-300 audio validate/render/inspect` handles named ZzFX SFX and frozen BeepBox JSON BGM. `step-300 plan` includes audio automatically, step-400 checks separate audio coverage and copies WAV into `assets/audio/`, and step-500 records decoded bytes and copies them into browser builds. All source, PCM, file, package and revision digests participate in approval invalidation.

The verdict vocabulary is four-valued — `PASS / CONCERNS / NOT_ASSESSED /
FAIL` with priority `FAIL > CONCERNS > NOT_ASSESSED > PASS`. "Could not check"
is never `PASS`.

## Running one step in isolation

Each step freezes its output into the next step's fixtures
(`steps/step-NNN-*/tests/fixtures/`), so every step runs alone. Use the checked-in fake provider transcripts and approved design fixture in the integration tests for offline runs; real model runs additionally need the normal provider configuration and a passing step-100 approval.

Deleting a downstream directory never breaks an upstream step.
