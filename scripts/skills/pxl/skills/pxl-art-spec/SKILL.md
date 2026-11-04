---
name: pxl-art-spec
description: step-200. Turn asset-requirements.json and style-guide.json into SVG, ASCII or recipe art specs with a model (fake provider offline), then lint them against step-300 with zero pixel output. Use when running step-200, authoring or linting art specs, listing step-300 capabilities, or doing a slice run (--scope slice).
---

# pxl-art-spec

step-200 reads a design directory (`asset-requirements.json`, `style-guide.json`, and `pipeline.json` for slices) and writes specs plus `spec-manifest.json`. It never writes pixels or audio. Optional `audio/audio.spec.json` and local BeepBox songs are validated and digested without Node or rendering. Its `run` and `lint` call `step-300 plan --dry-run` to prove every spec compiles.

Call it as `scripts/run.sh step-200 ...` from the `pxl` skill directory; the launcher supplies `--pxl-bin`.

## Commands

```bash
run.sh step-200 run --in <design-dir> --out <spec-dir> [--scope all|slice] [--asset <id>] [--provider fake|openai] [--env <file>] [--max-turns 24] [--lang en|zh] [--library <dir>]... [--token-budget N] [--request-budget N] [--wall-clock-secs N]
run.sh step-200 lint --in <spec-dir>              # digests + dry-run compile, no model, no output
run.sh step-200 capabilities [list|schema] <kind> # e.g. generators, ops, macros, recipe, shapes
run.sh step-200 status --in <spec-dir>
```

- Offline: `STEP_200_FAKE_TURNS=<transcript.json>`. Fixtures: `steps/step-200-art-spec/tests/fixtures/fake_turns_{full,slice,partial}.json`, with an approved design in `.../fixtures/design`.
- `--in` must hold an approved step-100 package. Without approval step-200 refuses to run.
- `--scope slice` keeps only `pipeline.json` `slice.assets[]`. Write slice output to `<spec-dir>/.slice`.
- A passing run becomes current; failed attempts stay in `.rejected`.
- `lint` prints one JSON object. Check `ok` and every `checks[].result`.

## Choosing a spec kind

Prefer SVG for smooth shapes, ASCII for hand-placed pixels, recipe `gen` for templates. Details of the ASCII format: the `pxl-asset-build` skill, [ascii.md](../pxl-asset-build/references/ascii.md).

## Reuse before you draw (prototype library)

Pass `--library <dir>` (repeatable) to give step-200 a read-only prototype library. The library is optional: without it the tools below report an empty library and you author from scratch, saying so in the spec notes.

1. `assets.search_prototypes` with hard filters first: `view` (`top_down|side`), `kind` (`building|plant|crop|terrain|prop|fence|scene_kit`), `max_size [w,h]`, `text`. At most 5 approved candidates come back; if `truncated` is true, narrow the query instead of guessing.
2. `assets.read_prototype <id>` for the one you pick. Copy its `digest` into the instance's `prototype.digest`.
3. Write `<asset-id>/<name>.json` as an `asset-instance/v1` (schema: `step-300 schema asset-instance`): the prototype reference, a `size_tier`, `params` inside the declared ranges, `surfaces` per region, a `seed`, and only the `patches` the prototype cannot express.
4. `assets.check_instance` before finishing. It reports every problem at once.

Rules:

- Only approved entries are offered. Never ask for, or write an instance against, a `pending` or `rejected` entry; if nothing fits, record which structure, material or view is missing and author that asset by hand.
- A digest mismatch means the library changed after you read it. Re-read the entry; do not edit the digest by hand.
- Parameters choose between variants an artist drew. They do not stretch or invent geometry. Out-of-range values are errors, not clamped.
- Step-200 still writes no pixels. Expansion into pixels happens in step-300 (`step-300 prototype expand`).
