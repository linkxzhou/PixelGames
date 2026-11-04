---
name: pxl-asset-build
description: step-300, the deterministic pixel-art CLI. Generate sprites and tiles, apply ops, animate with macros, pack Aseprite-style spritesheets, render PNG/GIF previews, run QA, and build whole recipes or spec trees (plan). Includes ASCII, SVG, shapes, sprite, pixelize and enhance commands. Use when creating, transforming, previewing, packing or validating pixel-art assets with step-300 / pxl.
---

# pxl-asset-build

`step-300` turns a palette, a generator input and a seed into palette-indexed `*.pxl.json` documents, animations, sheets and PNG/GIF previews. Same inputs, seed and version give byte-identical output. Call it as `scripts/run.sh step-300 ...` from the `pxl` skill directory.

## Ground rules

1. Pass `--json` on every call. Stdout is one JSON object; logs go to stderr.
2. Never edit pixels with Python or image libraries.
3. Discover first: `list generators|ops|macros|templates|palettes`, `schema recipe|ops|op|pxl|palette|parts|sheet`. Macro arguments come from `list macros --json`.
4. Exit codes: 0 ok, 1 I/O, 2 usage, 3 schema, 4 semantic, 5 QA, 6 output exists. Read `error.code`, `error.message`, `error.hint`, fix, retry at most 3 times.
5. Outputs are not overwritten; add `--overwrite` when re-running in place.
6. Pass `--seed` explicitly. `--strict` turns warnings into errors.
7. Finish with `validate <doc-or-sheet.json> --json`.
8. Put specs and recipes in files; do not inline large JSON in shell quotes.

## Command map

| Command | Use |
| --- | --- |
| `gen bollinger\|shapes\|noise-tile\|parts` | one static document (`--template --spec --palette --ramp --mirror --dir --kind --size --variants`) |
| `ascii import\|render\|refine`, `gen ascii` | text grid to spec, document, or PNG/GIF; see [ascii.md](references/ascii.md) |
| `svg render\|lint\|matrix` | rasterise an SVG subset to indexed pixels, or export it as an ASCII layout |
| `sprite build`, `shapes refine` | compile `*.sprite.json`; double a shapes spec for hand refinement |
| `op` | `--op '<json>'` (repeatable) or `--ops file`; `--frame X --as Y` |
| `anim` | `--name N --macro 'bob(amp=1)'` or `--frames a,b,c`, `--dirs`, `--fps`, `--no-loop` |
| `palette list\|show\|apply\|swap\|cycle`, `tile gen` | palettes and tiles (`--kind dirt\|grass\|sand\|ore`) |
| `sheet <docs...>` | pack into indexed PNG plus Aseprite JSON; inputs must share one palette |
| `render` | `--anim` for GIF, `--scale`, `--grid` preview, `--asset` transparent |
| `pixelize`, `enhance`, `profile [tier]` | PNG to indexed frame; native-resolution 2x to 4x rebuild; size-tier defaults |
| `inspect`, `validate` | size, colours, bbox, text matrix; QA (`--rules default\|strict`, `--qa-config file`) |
| `build <recipe> -o dir` | generate, animate, pack, preview, validate in one go |
| `plan --in <spec-dir> --out <dir>` | build every recipe and optional audio under a spec tree; write `asset-build/v4` `index.json` |
| `prototype check\|expand\|lock --library <dir>` | validate a prototype library, expand one frozen `asset-instance` into a `*.shapes.json`, write `catalog.lock.json`; see below |
| `audio validate <spec>` | validate `audio-spec/v1` and local BeepBox dependencies without Node or output |
| `audio render <spec> -o <dir>` | render ZzFX SFX and BeepBox BGM to 44.1 kHz signed 16-bit WAV plus `audio-manifest.json` (`--only`, `--adapter`) |
| `audio inspect <wav-or-dir>` | check WAV engineering facts or re-verify all file/PCM digests and loop ranges |

`plan` writes visual and audio entries in `index.json`. Audio needs Node 22+ only for rendering, not validation/inspection. Use `--seed` and `--strict` explicitly; failed output stays in `.rejected`. Run `step-300 schema audio-spec` for the frozen named-parameter contract. Do not hand-write sparse ZzFX arrays, remote URLs, MML or arbitrary JS.

Run `step-300 <command> --help` for flags. `mcp` is a stub; use the CLI.

## Example: seeded slime

```bash
cd "$(mktemp -d)"; R=<skill-dir>/scripts/run.sh
$R step-300 gen bollinger --seed 7 --template slime_8x8 --palette p16 --ramp slime --mirror x -o slime.pxl.json --json
$R step-300 op slime.pxl.json --op '{"op":"outline","mode":"outer"}' -o slime_o.pxl.json --json
$R step-300 anim slime_o.pxl.json --name hop --macro 'bob(amp=1)' --fps 8 -o slime_a.pxl.json --json
$R step-300 render slime_a.pxl.json --anim hop --scale 8 -o hop.gif --json
$R step-300 validate slime_a.pxl.json --json
```

Expected: `gen` reports 8x8 with frame `base`; `anim` adds `hop_0..hop_3`; `render` writes a 64x64 GIF; `validate` exits 0 with `"ok":true`.

## Prototype library

`prototype check --library <dir>` validates `prototype/`, `surface/`, `kit/`, `base/` and `recolour/` (JSON only), including source digests and the dependency closure. It exits non-zero on any problem.

`prototype expand --library <dir> <instance.json> -o <out.shapes.json>` is deterministic: same library and instance give byte-identical output. It refuses unapproved entries unless `--allow-pending` is passed (draft builds only), rejects an instance pinned to a different prototype digest, and rejects any overlay or patch that would fall outside the canvas. Base files must use `pixels` draw commands so expansion is exact.

`prototype lock --library <dir> -o catalog.lock.json` writes the sorted, digested input closure. It fails while any entry is not approved, unless `--allow-pending` is passed for a draft lock. Approval is set by a human editing the entry's `review`; no command sets it.

Pass the resulting shapes to the normal recipe path so the usual finish and QA still run. If the library is missing or an id is absent, report it; do not substitute a flat colour block.

## Sheet contract check

`python3 scripts/check_sheet.py <sheet.json> [min_frames]` checks the Aseprite structure (`frames`, `frameTags`, `meta`) and the `meta.pxl` extension. Exit 0 on pass; exit 1 with `FAIL: <reason>` on the first violation.

## Gotchas

- Existing output paths fail with `E_EXISTS` (exit 6) unless `--overwrite` is set.
- `ascii import --cell` must fit the canvas: a 16x10 grid at `--cell 4` overflows 32x32 (`ascii.size_overflow`); the error hint names the largest cell that fits.
- Golden hashes live in `libs/pxl-cli/tests/golden/hashes.json`; regenerate only on purpose.
