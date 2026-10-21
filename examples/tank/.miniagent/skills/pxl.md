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
2. Before writing a JSON file, run `pxl list <kind> --json` or `pxl schema <kind> --json`. Kinds for list: `generators`, `ops`, `macros`, `templates`, `palettes`. Kinds for schema: `recipe`, `ops`, `op`, `pxl`, `palette`, `parts`, `sheet`.
3. Read the exit code. `0` is success. Anything else is a failure.
4. On failure, read `error.code`, `error.message`, `error.path`, and `error.hint`. Fix the input and retry that step. Stop after 3 failed attempts on the same step and report the last hint.
5. Finish only after `pxl validate <sheet.json> --rules <rules> --json` exits 0. Use `tank` for the tank demo, `character` for a paper-doll, and `default` for anything else.

Exit codes:

| code | name | meaning |
| --- | --- | --- |
| 0 | ok | command succeeded |
| 1 | E_IO | disk or stdin/stdout failure |
| 2 | E_USAGE | bad arguments |
| 3 | E_SCHEMA | JSON does not match the schema |
| 4 | E_SEMANTIC | known fields, impossible combination. Arbitrary-angle `rotate` and `tile autotile` are not implemented. `part_transform` and `compose` are. |
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

## Farmer (paper-doll)

A character is a parts rig. `examples/farmer/` is the reference: 16×32, four facings, walk, idle, and a down-facing hoe. Right is a mirror of left. Build the body from parts with pivots and per-facing sprites.

Working files:

- `farmer.palette.json` — index 0 is `#00000000`. Ramps `skin`, `hair`, `shirt`, `pants`, plus swap targets `blonde`, `raven`, `red`, `green`.
- `farmer.parts.json` — parts with pivots, per-facing sprites (or `"up": "down"` aliases), `order` per facing, and `anims` whose frames are integer `dx` / `dy` / `rot90`.
- `farmer.recipe.json` — `base.gen` is `parts`. Each direction is `compose`. Animations use `doll(anim=walk)`, `doll(anim=idle)`, or `doll(anim=hoe)`.

`post` on a parts sprite runs after every `compose` (outline, then a ramp swap for a variant). One palette serves the whole sheet. A variant is `palette_swap` with `ramp:hair->blonde`, not a second palette file.

```bash
pxl schema parts --json
pxl gen parts --spec farmer.parts.json --dir down -o farmer.pxl.json --json
pxl build farmer.recipe.json -o out/ --json
pxl validate out/farmer.sheet.json --rules character --json
```

`--rules character` requires: palette indices only, facings down/up/left/right, left and right bounding boxes equal, right pixels the x-flip of left (idle and every walk/idle frame), down and up boxes within 2px, at least 4 walk frames and 2 idle frames per facing, walk diffs 1%–35% including the loop, idle diffs 0.5%–25%, one shared foot line across walk, idle, and the hoe, and a non-looping `hoe` of at least 4 frames on down.

Keep the head bob on the head part. A torso shift drags the legs and moves the foot line. `rot90` is `cw`, `ccw`, or `180` around the part pivot. If a tool swing drops below the boots, shorten the tool. When a check fails, change the sprite or the pose.

When several sprites share an animation name, the first one keeps tags like `walk_down`. Later sprites get `{id}_walk_down`. `--rules character` checks the first sprite's anims.
