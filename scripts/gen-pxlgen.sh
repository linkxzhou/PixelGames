#!/usr/bin/env bash
# Batch: examples/<dir>/*/recipe.json -> pxlgen.png (same size as reference.png)
# Usage: scripts/gen-pxlgen.sh [examples/reference-v3] [scale]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIR="${1:-$ROOT/examples/reference-v3}"; SCALE="${2:-1}"
cargo build -q -p pxl-cli --manifest-path "$ROOT/Cargo.toml"
PXL="$ROOT/target/debug/pxl"; ok=0; fail=0
for r in "$DIR"/*/recipe.json; do
  d="$(dirname "$r")"; n="$(basename "$d")"
  read -r gen spec seed pal < <(python3 -c 'import json,sys;j=json.load(open(sys.argv[1]));b=j["sprites"][0]["base"];print(b["gen"],b["spec"],j.get("seed",0),j["palette"])' "$r")
  tmp="$(mktemp -t pxlgen).json"
  if (cd "$d" && "$PXL" --quiet gen "$gen" --spec "$spec" --palette "$pal" --seed "$seed" -o "$tmp" --overwrite \
      && "$PXL" --quiet render "$tmp" --scale "$SCALE" -o pxlgen.png --overwrite); then ok=$((ok+1)); else echo "FAIL $n"; fail=$((fail+1)); fi
  rm -f "$tmp"
done
echo "done: $ok ok, $fail failed"
