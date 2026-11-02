#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
cargo +1.95.0 build -p pxl-cli --bin step-300 -q
BIN=target/debug/step-300
mkdir -p work/v8-ase-pilot/out/slices work/v8-ase-pilot/out/import
$BIN ase inspect work/v8-ase-pilot/mini_indexed.aseprite
$BIN --overwrite ase convert work/v8-ase-pilot/mini_indexed.aseprite --save-as work/v8-ase-pilot/out/anim.gif --frame-range 0-2
$BIN --overwrite ase convert work/v8-ase-pilot/mini_indexed.aseprite --save-as work/v8-ase-pilot/out/dither.png \
  --color-mode indexed --dithering-algorithm ordered --dithering-matrix bayer8x8 \
  --palette work/v8-ase-pilot/pilot.gpl
$BIN --overwrite ase convert work/v8-ase-pilot/mini_indexed.aseprite --save-as work/v8-ase-pilot/out/cropped.png --crop 0,0,4,4
$BIN --overwrite ase list-slices work/v8-ase-pilot/mini_indexed.aseprite
$BIN --overwrite ase split-slices work/v8-ase-pilot/mini_indexed.aseprite --out work/v8-ase-pilot/out/slices
$BIN --overwrite ase sheet work/v8-ase-pilot/mini_indexed.aseprite --out work/v8-ase-pilot/out/sheet.png --layout horizontal --padding 1 --extrude 1 --trim
$BIN --overwrite ase filter work/v8-ase-pilot/mini_indexed.aseprite invert --out work/v8-ase-pilot/out/invert.png
$BIN --overwrite ase filter work/v8-ase-pilot/mini_indexed.aseprite brightness-contrast --brightness 20 --contrast 10 --out work/v8-ase-pilot/out/bright.png
$BIN --overwrite ase filter work/v8-ase-pilot/mini_indexed.aseprite hue-saturation --hue 30 --sat 10 --out work/v8-ase-pilot/out/huesat.png
$BIN --overwrite ase op work/v8-ase-pilot/mini_indexed.aseprite flip --axis h --out work/v8-ase-pilot/out/flipped.aseprite
$BIN --overwrite ase op work/v8-ase-pilot/mini_indexed.aseprite flood-fill --x 1 --y 1 --to-color '#ff0000' --out work/v8-ase-pilot/out/flood.png
$BIN --overwrite ase import work/v8-ase-pilot/mini_indexed.aseprite --out work/v8-ase-pilot/out/import --asset mini_indexed
# M7 deferred → landed
$BIN --overwrite ase convert work/v8-ase-pilot/mini_indexed.aseprite --save-as work/v8-ase-pilot/out/trimgrid.png --trim-by-grid --frame 0
$BIN --overwrite ase sheet work/v8-ase-pilot/mini_indexed.aseprite --out work/v8-ase-pilot/out/sheet_tag.png --tag idle --play-subtags --layout horizontal
$BIN --overwrite ase run -- work/v8-ase-pilot/mini_indexed.aseprite --tag idle --trim --save-as work/v8-ase-pilot/out/batch_out.png
# M9 script (Rhai; needs Rust that can parse thin-vec/edition2024, e.g. 1.95+)
$BIN ase script work/v8-ase-pilot/replace_red.rhai --workdir work/v8-ase-pilot \
  --script-param in=mini_indexed.aseprite \
  --script-param out=out/scripted.aseprite \
  --script-param to=#00ff00
echo "pilot ok (M7+M9)"
