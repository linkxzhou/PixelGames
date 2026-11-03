# step-300 CLI cheatsheet (PixelGames)

Discovered from `libs/pxl-cli` + `./target/debug/step-300 --help` on main. Prefer live `--help` after rebuilds.

```bash
PXL=./target/debug/step-300
# cargo +1.95.0 run -p pxl-cli --bin step-300 --
```

Global: `--json` `--seed` `--quiet` `--strict` `--overwrite` `--pxl-version-lock`

## Generate / ASCII / shapes / tiles

```bash
$PXL gen bollinger --template slime_8x8 --palette p16 --ramp slime --mirror x -o slime.pxl.json
$PXL gen shapes --spec prop.shapes.json --palette p16 -o prop.pxl.json
$PXL gen noise-tile --kind dirt --size 16 --palette tank --ramp ground -o dirt.pxl.json
$PXL ascii import sketch.ascii.txt --palette p16 -o sketch.ascii.json
$PXL ascii render sketch.ascii.txt --palette p16 --scale 8 -o sketch.png
$PXL ascii refine sketch.ascii.txt -o sketch_x2.ascii.txt
$PXL shapes refine prop.shapes.json -o prop_x2.shapes.json
$PXL tile gen --kind grass --size 16 --palette tank --variants 4 -o grass.pxl.json
```

## Palette / inspect / validate / render / build / sheet

```bash
$PXL list palettes|templates|ops|macros
$PXL palette show p16
$PXL palette ramp p16 --levels 4 --hue-shift 40 -o ramped.palette.json
$PXL palette apply in.pxl.json ramped.palette.json -o out.pxl.json
$PXL inspect doc.pxl.json
$PXL validate doc.pxl.json --json
$PXL render doc.pxl.json -o preview.png --scale 8
$PXL render doc.pxl.json -o asset.png --asset
$PXL render doc.pxl.json --anim walk --dir E -o walk.gif --scale 4
$PXL build recipe.json -o out/
$PXL sheet a.pxl.json --layout rows-by-anim -o sheet.png
```

## ase (v8 headless)

```bash
$PXL ase inspect file.aseprite
$PXL ase list-layers|list-tags|list-slices file.aseprite
$PXL ase convert file.aseprite -o out.png --scale 1
$PXL ase convert file.aseprite -o out.gif --tag walk
$PXL ase sheet file.aseprite -o sheet.png --data sheet.json --layout packed --padding 1
$PXL ase filter file.aseprite outline -o out.aseprite
$PXL ase op file.aseprite …          # multi-frame flip/rotate90 — see --help
$PXL ase split-slices file.aseprite -o slices/
$PXL ase import file.aseprite -o recipe_dir/
$PXL ase export-tileset file.aseprite -o tiles.png --index 0
```

### ase edit (source on main; rebuild if binary lacks them)

```bash
$PXL ase frame add file.aseprite --at 1 --duration 100 -o out.aseprite
$PXL ase frame remove file.aseprite 2 -o out.aseprite
$PXL ase frame copy file.aseprite 0 1 -o out.aseprite
$PXL ase frame set-duration file.aseprite 0 120 -o out.aseprite
$PXL ase tag add file.aseprite walk --from 0 --to 3 -o out.aseprite
$PXL ase tag rename|remove|set …
$PXL ase layer add|remove|rename|set …
$PXL ase cel move|unlink|clear|set …
$PXL ase flatten file.aseprite -o out.aseprite
$PXL ase sprite set file.aseprite --grid 16,16 -o out.aseprite
$PXL ase palette sort|set|add|remove|clear-frame …
```

Convert extras worth knowing: `--trim` `--shrink-to WxH` `--frame-range` `--ignore-layer` `--split-tags` `--play-subtags`.

Sheet extras: `--extrude` `--border-padding` `--shape-padding` `--sheet-pack` `--format json-hash|json-array` `--power-of-two-size`.
