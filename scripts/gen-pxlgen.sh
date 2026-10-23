#!/usr/bin/env bash
# Batch: recipe.json -> pxlgen.png (static base frame of each sprite, no anims),
# written next to the recipe.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEFAULT_DIR="$ROOT/examples/reference-v3"

usage() {
  cat <<EOF
Usage: $(basename "$0") [options] [PATH ...]

Scan recipe JSON files and render each sprite's base frame to a PNG placed in
the same directory as the recipe.

PATH can be:
  - a directory : scanned recursively for recipe.json / *.recipe.json
  - a JSON file : used directly as a recipe
Default PATH: examples/reference-v3

Options:
  -h, --help           Show this help and exit
  -s, --scale N        Integer render scale (default: 1)
  -o, --output NAME    Output file name (default: pxlgen.png). With several
                       sprites in one recipe, the first uses NAME and the
                       others use <stem>-<sprite id>.<ext>. When a directory
                       holds more than one recipe, NAME is ignored and each
                       recipe writes <recipe stem>.<ext> (e.g. brick.png for
                       brick.recipe.json) so outputs never collide.
      --seed N         Override the seed stored in the recipe
  -n, --skip-existing  Skip recipes whose output file already exists
  -N, --dry-run        List what would be generated, do nothing
  -B, --no-build       Do not run cargo build (use existing binary)
  -q, --quiet          Only print failures and the summary
  -v, --verbose        Print each generated file

Environment:
  PXL                  Path to the pxl binary (default: target/debug/pxl)

Exit status: 0 if all succeeded, 1 if any failed, 2 on usage error.

Examples:
  $(basename "$0")                                  # whole default folder
  $(basename "$0") examples/reference-v3/barrel     # one folder
  $(basename "$0") -s 8 examples/reference-v3/barrel/recipe.json
  $(basename "$0") -n examples/reference examples/reference-v3
EOF
}

die() { echo "error: $*" >&2; exit 2; }

SCALE=1; OUT_NAME="pxlgen.png"; SEED=""; SKIP_EXISTING=0; DRY=0; BUILD=1
QUIET=0; VERBOSE=0; PATHS=()

while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    -s|--scale) [ $# -ge 2 ] || die "$1 needs a value"; SCALE="$2"; shift 2 ;;
    --scale=*) SCALE="${1#*=}"; shift ;;
    -o|--output) [ $# -ge 2 ] || die "$1 needs a value"; OUT_NAME="$2"; shift 2 ;;
    --output=*) OUT_NAME="${1#*=}"; shift ;;
    --seed) [ $# -ge 2 ] || die "$1 needs a value"; SEED="$2"; shift 2 ;;
    --seed=*) SEED="${1#*=}"; shift ;;
    -n|--skip-existing) SKIP_EXISTING=1; shift ;;
    -N|--dry-run) DRY=1; shift ;;
    -B|--no-build) BUILD=0; shift ;;
    -q|--quiet) QUIET=1; shift ;;
    -v|--verbose) VERBOSE=1; shift ;;
    --) shift; while [ $# -gt 0 ]; do PATHS+=("$1"); shift; done ;;
    -*) usage >&2; die "unknown option: $1" ;;
    *) PATHS+=("$1"); shift ;;
  esac
done

case "$SCALE" in ''|*[!0-9]*|0) die "--scale must be a positive integer, got '$SCALE'" ;; esac
case "$SEED" in *[!0-9]*) die "--seed must be a non-negative integer, got '$SEED'" ;; esac
case "$OUT_NAME" in */*|'') die "--output must be a plain file name, got '$OUT_NAME'" ;; esac
command -v python3 >/dev/null 2>&1 || die "python3 is required"

[ ${#PATHS[@]} -gt 0 ] || PATHS=("$DEFAULT_DIR")

# ---- collect recipes --------------------------------------------------------
RECIPES=()
for p in "${PATHS[@]}"; do
  if [ -d "$p" ]; then
    found=0
    while IFS= read -r f; do RECIPES+=("$f"); found=1; done < <(
      find "$p" -type f \( -name 'recipe.json' -o -name '*.recipe.json' \) | LC_ALL=C sort)
    [ "$found" -eq 1 ] || echo "warn: no recipe found under $p" >&2
  elif [ -f "$p" ]; then
    RECIPES+=("$p")
  else
    die "path not found: $p"
  fi
done
[ ${#RECIPES[@]} -gt 0 ] || { echo "nothing to do" >&2; exit 1; }

# ---- binary -----------------------------------------------------------------
PXL="${PXL:-$ROOT/target/debug/pxl}"
if [ "$DRY" -eq 0 ]; then
  if [ "$BUILD" -eq 1 ]; then
    command -v cargo >/dev/null 2>&1 || die "cargo not found (use -B with PXL=/path/to/pxl)"
    cargo build -q -p pxl-cli --manifest-path "$ROOT/Cargo.toml"
  fi
  [ -x "$PXL" ] || die "pxl binary not found: $PXL (build it or set PXL)"
fi

TMP="$(mktemp -d -t pxlgen)"
trap 'rm -rf "$TMP"' EXIT

# Print one TSV line per sprite: id gen spec template mirror ramp seed palette
# ("-" = empty). Exit 3 = not a sprite recipe.
read_recipe() {
  python3 - "$1" "$SEED" <<'PY'
import json, sys
try:
    j = json.load(open(sys.argv[1]))
except Exception as e:
    sys.stderr.write("invalid json: %s\n" % e); sys.exit(1)
if not isinstance(j, dict) or "palette" not in j:
    sys.exit(3)
sprites = j.get("sprites") or []
if not sprites:
    sys.exit(4)
def f(v):
    v = "" if v is None else str(v)
    return v if v else "-"
for s in sprites:
    b = s.get("base") or {}
    seed = sys.argv[2] if sys.argv[2] != "" else b.get("seed", j.get("seed", 0))
    print("\t".join(f(x) for x in (
        s.get("id", "sprite"), b.get("gen"), b.get("spec"), b.get("template"),
        b.get("mirror"), b.get("ramp"), seed, j["palette"])))
PY
}

ext="${OUT_NAME##*.}"
[ "$ext" != "$OUT_NAME" ] || ext="png"

# Number of collected recipes living in one directory (no associative arrays:
# macOS ships bash 3.2).
recipes_in_dir() {
  local dir="$1" c=0 f
  for f in "${RECIPES[@]}"; do
    if [ "$(cd "$(dirname "$f")" && pwd)" = "$dir" ]; then c=$((c+1)); fi
  done
  echo "$c"
}

# First output file name for one recipe: OUT_NAME when the recipe is alone in
# its directory, otherwise derived from the recipe file so shared folders do
# not overwrite each other's pxlgen.png.
first_output_name() {
  local rp="$1" rd rbase
  rd="$(cd "$(dirname "$rp")" && pwd)"
  if [ "$(recipes_in_dir "$rd")" -le 1 ]; then printf '%s\n' "$OUT_NAME"; return; fi
  rbase="$(basename "$rp")"
  case "$rbase" in
    *.recipe.json) rbase="${rbase%.recipe.json}" ;;
    recipe.json) rbase="$(basename "$rd")" ;;
    *) rbase="${rbase%.json}" ;;
  esac
  printf '%s\n' "$rbase.$ext"
}

ok=0; fail=0; skip=0
log() { [ "$QUIET" -eq 1 ] || echo "$@"; }

for r in "${RECIPES[@]}"; do
  d="$(cd "$(dirname "$r")" && pwd)"; rf="$(basename "$r")"; n="$(basename "$d")"
  set +e; lines="$(read_recipe "$r" 2>"$TMP/err")"; rc=$?; set -e
  case $rc in
    0) ;;
    3) log "skip $r (not a recipe)"; skip=$((skip+1)); continue ;;
    4) log "skip $r (no sprites; tiles-only recipes are not supported)"; skip=$((skip+1)); continue ;;
    *) echo "FAIL $r: $(cat "$TMP/err")"; fail=$((fail+1)); continue ;;
  esac

  first_out="$(first_output_name "$r")"
  stem="${first_out%.*}"; fext="${first_out##*.}"

  idx=0
  while IFS=$'\t' read -r id gen spec tpl mirror ramp seed pal; do
    if [ "$idx" -eq 0 ]; then out="$first_out"; else out="$stem-$id.$fext"; fi
    idx=$((idx+1))
    label="$n/$id"
    if [ "$SKIP_EXISTING" -eq 1 ] && [ -e "$d/$out" ]; then
      log "skip $label ($out exists)"; skip=$((skip+1)); continue
    fi
    if [ "$DRY" -eq 1 ]; then echo "would write $d/$out  [$gen, seed $seed]"; continue; fi

    args=(--quiet gen "$gen" --palette "$pal" --seed "$seed")
    [ "$spec"   = "-" ] || args+=(--spec "$spec")
    [ "$tpl"    = "-" ] || args+=(--template "$tpl")
    [ "$mirror" = "-" ] || args+=(--mirror "$mirror")
    [ "$ramp"   = "-" ] || args+=(--ramp "$ramp")

    tmp="$TMP/$$.$idx.json"
    if (cd "$d" && "$PXL" "${args[@]}" -o "$tmp" --overwrite \
        && "$PXL" --quiet render "$tmp" --scale "$SCALE" -o "$out" --overwrite) 2>"$TMP/err"; then
      ok=$((ok+1))
      if [ "$VERBOSE" -eq 1 ]; then echo "ok   $d/$out"; fi
    else
      echo "FAIL $label (recipe: $rf)"; sed 's/^/     /' "$TMP/err" >&2
      fail=$((fail+1))
    fi
    rm -f "$tmp"
  done <<< "$lines"
done

echo "done: $ok ok, $fail failed, $skip skipped"
[ "$fail" -eq 0 ]
