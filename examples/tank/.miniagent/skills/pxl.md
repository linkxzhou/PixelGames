---
name: pxl
description: Generate deterministic pixel-art assets (sprites, animations, tiles, spritesheets) with the pxl CLI. Use when the task involves creating or repairing game art via pxl commands.
temperature: 0.2
---

# pxl skill

You drive the `pxl` CLI to produce pixel-art assets. pxl is deterministic: the same
inputs (spec + palette + seed) always produce the same pixels. Your job is to write
specs as JSON files, run pxl commands, read structured feedback, and converge on a
valid, validated spritesheet.

## Ground rules

1. **Always pass `--json`.** Every invocation must include `--json` so stdout is one
   JSON object. Logs go to stderr and can be ignored.
2. **Never hand-write PNG output.** Never use Python or any image library to
   manipulate pixels. All pixels come from pxl commands.
3. **Discover before you write.** Run `pxl list ...` or `pxl schema ...` first when
   unsure about a generator, op, macro, or file format.
4. **Check every exit code.** Exit 0 = success. Non-zero = read the JSON error:
   - `error.code` — machine code (`E_USAGE`, `E_SCHEMA`, `E_QA`, ...)
   - `error.message` — what went wrong
   - `error.hint` — how to fix it (e.g. the exact macro signature)
   - `error.path` — the offending file, when applicable
   Fix according to `hint` and retry. **At most 3 retries per failure**, then move
   on and report the blocker.
5. **Finish with `pxl validate`.** A task is only complete when
   `pxl validate <sheet.json> --rules <rules> --json` exits 0.

## Command workflow

Typical order for one asset (see `examples/tank/` for a complete reference):

```bash
pxl palette show <palette.json> --json            # inspect palette ramps/cycles
pxl gen shapes --spec <body.shapes.json> --seed 1 -o <id>.pxl.json --json
pxl op <id>.pxl.json --op '<op JSON>' -o <id>.pxl.json --overwrite --json
pxl op <id>.pxl.json --frame N --as E --op '{"op":"rotate90","dir":"cw"}' -o <id>.pxl.json --overwrite --json
pxl anim <id>.pxl.json --name move --dirs N,E,S,W --macro '<macro call>' -o <id>.pxl.json --overwrite --json
pxl tile gen --kind dirt --size 16 --variants 4 --palette <palette.json> -o ground.pxl.json --json
pxl sheet <docs...> --layout rows-by-anim --pad 1 -o out/<id>.sheet.png --json
pxl render out/<id>.sheet.png --scale 4 --grid -o out/preview.png --json
pxl validate out/<id>.sheet.json --rules tank --json
```

## Writing input files

- Specs, palettes, and recipes are JSON files. **Write a JSON file, then pass its
  path** — do not inline large JSON in shell quotes (quoting errors are the #1
  failure mode).
- Use `pxl schema <kind>` (kinds: `palette`, `shapes`, `recipe`, `op`, ...) to get
  the exact expected structure before writing a file.
- `pxl list <kind>` (kinds: `gens`, `ops`, `macros`, `templates`, `palettes`) shows
  available ids.
- Recipes: once a step-by-step run succeeds, save it as `<asset>.recipe.json` and
  reproduce with `pxl build <recipe> -o out/`. Recipes make results auditable.

## Animations and directions

- `gen shapes` outputs a single frame named `base`. Copy it to direction frames
  with `--frame base --as N --op '{"op":"noop"}'`.
- 4 directions derive geometrically: `E = rotate90(cw) of N`, `S = flip y of N`,
  `W = flip x of E`. Keep the canvas square so rotation is lossless.
- Animation macros carry their own signature. If you pass a wrong argument, the
  `hint` field returns the exact accepted arguments, e.g.
  `tread_cycle(steps=4, region=auto_tread, cycle=tread, bob=1)`. Trust the hint,
  not your memory.
- `region=auto_tread` is the only valid region for tread_cycle — it infers tread
  columns from `ramps.tread` and rotates per direction automatically.

## QA thresholds (rules=tank)

`pxl validate --rules tank` enforces, per animation and direction:

- `anim_diff`: adjacent-frame pixel difference between 2% and 25%; first/last
  frames must loop seamlessly (unless the anim is explicitly non-looping).
- `tank_facing`: N/S bounding boxes identical; E/W are transposes of N.
- `tread_motion`: tread region must change every frame.
- `hull_stability`: hull pixel count drifts ≤ 5% across frames.
- `tile_seams`: tiles tile seamlessly on all four edges.
- `palette.alpha`: no semi-transparent pixels; all indices resolve in the palette.

Failed checks return structured values: check `id`, `value` (measured),
`limit` (threshold). Lower `value` is not always better — `anim_diff` is a band.
If frames differ too much (>25%), reduce motion amplitude per step (e.g. fewer
shift pixels, smaller bob); if too little (<2%), increase steps or amplitude.

## Determinism

- Always pass explicit `--seed`. Without it, pxl defaults seed to 1 and warns.
- Same inputs → same sha256. Verify reproducibility by running the build twice and
  comparing hashes when asked.

## Output contract recap

- Success: `{"ok": true, "cmd": "...", "out": "...", "warnings": [...]}` plus
  command-specific fields (`frames`, `checks`, etc.).
- Failure: `{"ok": false, "error": {"code", "message", "hint"?, "path"?}}`,
  non-zero exit code.
- `--overwrite` is required to replace an existing output file; add it whenever
  re-running a step in place.
