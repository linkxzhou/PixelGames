---
name: pxl
description: Drive the deterministic pxl CLI to generate palette-indexed sprites, animations, tiles, and spritesheets. Always pass --json, validate before finishing, and never write PNG bytes by hand.
temperature: 0.2
max_steps: 40
---

# pxl

`pxl` is a local Rust CLI. It writes palette-indexed documents (`.pxl.json`), indexed PNG spritesheets, GIF previews, and Aseprite JSON. The same recipe, seed, and `pxl` version produce byte-identical files.

You do not have image libraries. Do not write PNG or GIF bytes yourself. Do not use Python, ImageMagick, or any other tool to draw, crop, or recolor pixels. Every pixel comes from `pxl`.

## How to call it

1. Add `--json` to every command. Stdout is one JSON object. Logs stay on stderr.
2. Before writing a JSON file, run `pxl list <kind> --json` or `pxl schema <kind> --json`. Kinds for list: `generators`, `ops`, `macros`, `templates`, `palettes`. Kinds for schema: `recipe`, `ops`, `op`, `pxl`, `palette`, `sheet`.
3. Read the exit code. `0` is success. Anything else is a failure.
4. On failure, read `error.code`, `error.message`, `error.path`, and `error.hint`. Fix the input and retry that step. Stop after 3 failed attempts on the same step and report the last hint.
5. Finish only after `pxl validate <sheet.json> --rules tank --json` (or `--rules default` for a non-tank sprite) exits 0.

Exit codes:

| code | name | meaning |
| --- | --- | --- |
| 0 | ok | command succeeded |
| 1 | E_IO | disk or stdin/stdout failure |
| 2 | E_USAGE | bad arguments |
| 3 | E_SCHEMA | JSON does not match the schema |
| 4 | E_SEMANTIC | known fields, impossible combination. `rotate`, `part_transform`, `compose`, and `tile autotile` are not implemented yet |
| 5 | E_QA | `validate` or `build` checks failed. The `checks` array is on the same JSON object |
| 6 | E_EXISTS | the output file already exists. Pass `--overwrite`, or pick a new path |

`pxl build -o` takes a directory and replaces files inside it. Every other command refuses to replace an existing file unless you pass `--overwrite`.

Do not pass `--seed` unless you mean to override the file. A recipe's `"seed"` is the reproducible input.

## Tank demo

The checked-in files in this directory already pass `--rules tank`. Prefer `pxl build tank.recipe.json -o out/ --json` when the task is the tank demo. Use the step-by-step commands only when you are changing one stage.

Working files:

- `tank.palette.json` — 12 colors. Index 0 is `#00000000`. Ramps `olive`, `tread`, `ground`. Cycle `tread` is `[6, 7, 1]`.
- `tank_body.shapes.json` — 16×16, gun pointing north (small y), red muzzle index 10 (`a` in the text matrix). Treads are two columns on each side so the move diff stays inside 2%–25%.
- `tank.recipe.json` — shapes, hull texture, four directions, `tread_cycle` (4 frames) and `idle_rumble` (2 frames), four dirt tiles, sheet, GIF, preview.

Directions, in this order: `N` is the base, `E` is `rotate90` cw of `N`, `S` is flip y of `N`, `W` is flip x of `E`. The canvas stays square.

Step-by-step, if you are not using `build`:

```bash
pxl palette show tank.palette.json --json
pxl gen shapes --spec tank_body.shapes.json --palette tank.palette.json -o tank.pxl.json --json
pxl op tank.pxl.json --op '{"op":"texture","gen":"bollinger","template":"panel_4x8","region":[4,7,8,4],"ramp":"olive","seed":42}' -o tank.pxl.json --overwrite --json
pxl op tank.pxl.json --frame base --as N --op '{"op":"noop"}' -o tank.pxl.json --overwrite --json
pxl op tank.pxl.json --frame N --as E --op '{"op":"rotate90","dir":"cw"}' -o tank.pxl.json --overwrite --json
pxl op tank.pxl.json --frame N --as S --op '{"op":"flip","axis":"y"}' -o tank.pxl.json --overwrite --json
pxl op tank.pxl.json --frame E --as W --op '{"op":"flip","axis":"x"}' -o tank.pxl.json --overwrite --json
pxl anim tank.pxl.json --name move --dirs N,E,S,W --fps 10 --macro 'tread_cycle(steps=4, region=auto_tread, cycle=tread, bob=1)' -o tank.pxl.json --overwrite --json
pxl anim tank.pxl.json --name idle --dirs N,E,S,W --fps 4 --macro 'idle_rumble(steps=2, amp=1)' -o tank.pxl.json --overwrite --json
pxl tile gen --kind dirt --size 16 --seed 7 --palette tank.palette.json --variants 4 -o ground.pxl.json --json
pxl sheet tank.pxl.json ground.pxl.json --layout rows-by-anim --pad 1 -o out/tank.sheet.png --json
pxl render tank.pxl.json --anim move --dir E --scale 6 -o out/tank_move_E.gif --json
pxl render out/tank.sheet.png --scale 4 --grid -o out/preview.png --json
pxl validate out/tank.sheet.json --rules tank --json
```

`--rules tank` requires: every pixel in the palette, no semi-transparent colors, N/S bounding boxes equal, E/W the transpose of N, at least 4 move frames and 2 idle frames per direction, adjacent move frames (including the loop) differ by 2%–25%, tread pixels change every frame, hull pixel-count variance at most 5%, and at least 4 ground tiles whose opposite edges match or sit one ramp step apart.

If a diff check is over 25%, the tread region is too big or the hull bob is repainting too many pixels. Shrink the tread rect. Do not loosen the checker.

`pxl mcp` and `pxl tile autotile` are not available. Stay on the shell commands above.
