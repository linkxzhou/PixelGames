# ASCII generation spec

Contents
- Text grid
- Import options
- Spec file
- Example

One character is one cell; a cell is `cell` pixels (default 1x1, so pass `--cell`). Flow: write `*.ascii.txt`, `ascii import` to `*.ascii.json`, then `gen ascii` (or `ascii render` for PNG/GIF in one step, or an `ascii` base in a recipe).

## Text grid (`*.ascii.txt`)

- Blocks are runs of non-empty lines; a blank line ends a block. All rows in a block must have equal width (`ascii.ragged_row`), or pass `--pad` to right-fill with the transparent char.
- Name a block with a `:: name` line above it (lowercase letters, digits, `_`). Append `frame` or `layer` to set the kind: `:: body layer`. Without headers, blocks are `base`, `f1`, `f2`... (or `layer0`... with `--blocks layers`). Duplicate names fail (`ascii.duplicate_block`). `--no-directives` turns `::` lines into plain content.
- `--charset digits` (default): only `0-9`, `0` is transparent. `--charset printable`: ASCII `0x20-0x7E`, `.` and space are transparent. Any other character fails with its line and column.
- Keep the same charset across a layout and its `--legend` file (`ascii.legend_charset`).

## Import options

| flag | values |
| --- | --- |
| `--size` | `32` (default) or `64`; any other value fails (`ascii.bad_size`) |
| `--cell` | `N`, `NxM`, or `fit` (largest integer multiple of the longer side; errors when it does not fit) |
| `--anchor` | `center` (default), `bottom-center` (characters), `top-left` (tiles) |
| `--kind` | `sprite` (default), `tiles`, `effect`, `ui`, `overlay` |
| `--palette` / `--legend` | palette id or file; shared `*.legend.json` stored as `legend_ref` (relative to the spec) |
| `--blocks` | `frames` (default) or `layers` |

`ascii import` returns `layout_px` (cols*cell_w, rows*cell_h); it must fit within `size` or compile fails with `ascii.size_overflow`. Unmapped characters get `"auto": true` legend entries, listed in `auto`; replace them before shipping. Without a palette they are the placeholder `#ff00ffff`; with `--palette` and `printable`, indices are assigned from 2 upward, skipping special and legend-used indices (`ascii.palette_full` when exhausted).

## Spec (`*.ascii.json`)

- `import` writes `v:1`, `charset`, `size`, `cell`, `anchor`, `offset`, `palette`, `palette_mode`, `legend`, `layers`, `frames`, `anims`, `post`, `kind`, `source`. Fields it does not set (`anims`, `post`, `mirror`, `keep`) are added by editing the JSON.
- Legend key is exactly one character allowed by the charset. A value is a palette index (`14`), `"outline"` / `"transparent"` / `"flash"`, a `#rrggbbaa` colour, or an object with exactly one paint source: `index`, `color` (`role` may carry a colour hint), `ramp` + `step` + optional `pattern` (`solid|checker|stripe_x[:N]|stripe_y[:N]|bevel`, plus `alt`), or `stamp` (cell-sized rows, no `pattern`).
- `palette_mode`: `strict` (default; every colour must exist in the palette) or `extend` (appends colours and writes `<out-stem>.palette.json`). `extend` is rejected inside a recipe.
- Frames: `{"rows":[...]}`, `{"layers":["a","b"]}` (bottom to top), or `{"from":"base","patch":[{"x":2,"y":2,"rows":["4..4"]}],"ops":[...]}`. In a patch, `.` (digits) or `_` (printable) means keep the cell; override with `keep`, which must not be a legend key.
- `anims`: each needs exactly one of `frames` (flipbook), `macro`, or `ops`; `from` defaults to `base`, `dirs` expands to `<from>_<dir>` sources. Frames a flipbook names must exist (`ascii.anim_source`).
- Compile order per frame: patch ops, `mirror` (`x|y|xy`), then `post`. `meta.ascii` records the spec file name.
- Recipes: declare anims in the recipe or the spec, never both (`ascii.recipe_anims`).
- Do not hand-edit rows in `*.ascii.json` when a `source.file` exists; edit the `.txt` and re-import (`source.sha256` records the text).

## Example

```bash
R=<skill-dir>/scripts/run.sh
$R step-300 ascii import examples/ascii/slime/slime.ascii.txt --size 32 --cell 2 --anchor bottom-center --palette p16 --kind sprite -o slime.ascii.json --json
$R step-300 gen ascii --spec slime.ascii.json -o slime.pxl.json --seed 7 --json
# add "anims":[{"name":"blink","frames":["base","blink"],"fps":4}] to slime.ascii.json, then:
$R step-300 ascii render slime.ascii.json --anim blink --scale 8 -o blink.gif --json
```

`ascii render` accepts a `.txt` directly but then has no anims (`ascii.anim_source`); import first, add `anims`, then render with `--anim`. `--emit-json` and `--emit-doc` save the compiled spec and document alongside the render. Shipped specs live in `examples/ascii/` of the repository.
