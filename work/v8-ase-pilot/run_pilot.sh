#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
cargo build -p pxl-cli --bin step-300 -q
BIN=target/debug/step-300
mkdir -p work/v8-ase-pilot/out/slices
$BIN ase inspect work/v8-ase-pilot/mini_indexed.aseprite
$BIN --overwrite ase convert work/v8-ase-pilot/mini_indexed.aseprite --save-as work/v8-ase-pilot/out/anim.gif --frame-range 0-2
$BIN --overwrite ase convert work/v8-ase-pilot/mini_indexed.aseprite --save-as work/v8-ase-pilot/out/dither.png \
  --color-mode indexed --dithering-algorithm ordered --dithering-matrix bayer8x8 \
  --palette work/v8-ase-pilot/pilot.gpl
$BIN --overwrite ase list-slices work/v8-ase-pilot/mini_indexed.aseprite
$BIN --overwrite ase split-slices work/v8-ase-pilot/mini_indexed.aseprite --out work/v8-ase-pilot/out/slices
$BIN --overwrite ase sheet work/v8-ase-pilot/mini_indexed.aseprite --out work/v8-ase-pilot/out/sheet.png --layout horizontal
$BIN --overwrite ase filter work/v8-ase-pilot/mini_indexed.aseprite invert --out work/v8-ase-pilot/out/invert.png
$BIN --overwrite ase filter work/v8-ase-pilot/mini_indexed.aseprite brightness-contrast --brightness 20 --contrast 10 --out work/v8-ase-pilot/out/bright.png
$BIN --overwrite ase op work/v8-ase-pilot/mini_indexed.aseprite flip --axis h --out work/v8-ase-pilot/out/flipped.aseprite
echo "pilot ok"
