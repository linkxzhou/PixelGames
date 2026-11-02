#!/usr/bin/env bash
# PixelGames build entry. Run `./build.sh help` for the command list.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEB="$ROOT/web"
WASM_OUT="$WEB/src/wasm"
WASM_FILE="$WASM_OUT/pxl_wasm_bg.wasm"

# Rust packages the wasm build depends on; a change under any of them makes the wasm stale.
WASM_SOURCES=(
  "$ROOT/Cargo.toml"
  "$ROOT/steps/step-300-asset-build/pxl-core"
  "$ROOT/steps/step-300-asset-build/pxl-format"
  "$ROOT/steps/step-300-asset-build/pxl-gen"
  "$ROOT/steps/step-300-asset-build/pxl-ops"
  "$ROOT/steps/step-300-asset-build/pxl-export"
  "$ROOT/steps/step-300-asset-build/pxl-engine"
  "$ROOT/steps/step-300-asset-build/pxl-wasm"
  "$ROOT/assets"
  "$ROOT/scripts/build-wasm.sh"
)

usage() {
  cat <<'EOF'
Usage: ./build.sh <command> [options]

Web playground
  dev [--port N] [--host] [--no-open]   Build the wasm if it is missing or stale,
                                        install web deps if needed, start the Vite
                                        dev server and open the browser.
  wasm [--dev]                          Build pxl-wasm into web/src/wasm
                                        (release by default; --dev is faster, larger).
  web                                   wasm + production build into web/dist.
  web-engine [--dev]                    Build pxl-web and the miniquad JS runtime.
  web-test                              Run JS runtime/game tests and step-500 integration tests.
  preview [--port N] [--host]           Serve web/dist (runs `web` first if missing).
  install                               yarn install in web/.

Rust
  pxl [--release]                       Build the step-300 CLI (target/debug|release/step-300).
                                        For the skill's bundled binaries use
                                        scripts/build-skill-bins.sh.
  run <pxl args...>                     Build if needed, then run the CLI with the args.
  png [gen-pxlgen options] [PATH ...]   Render recipe.json -> pxlgen.png next to each
                                        recipe (default: examples/reference-v3).
                                        Options: -s SCALE, -o NAME, -n, -N, -q, -v ...
                                        See `./build.sh png --help`.
  skill-bins [build-skill-bins options] Build step-100..500 into scripts/skills/pxl/bin/<platform>/.
                                        Default: host platform. See scripts/build-skill-bins.sh.
  skill-check [verify_skill options]    Validate the bundled pxl skill (structure, links, command
                                        drift, binaries, offline end-to-end, portability).
  test                                  cargo test for the rust workspace + web unit tests.
  check                                 cargo check (native + wasm32) and vue-tsc.

Other
  all                                   pxl --release, wasm, web.
  clean [--all]                         Remove web/dist and web/src/wasm.
                                        --all also runs `cargo clean`.
  help                                  This text.

Examples
  ./build.sh dev                        # http://localhost:5173
  ./build.sh dev --host --port 8080     # reachable from other machines
  ./build.sh png                        # all recipes in examples/reference-v3
  ./build.sh png -s 8 examples/reference-v3/barrel
  ./build.sh run ascii render examples/ascii/gem/gem.ascii.txt \
        --charset printable --palette p16 --scale 8 -o /tmp/gem.png --overwrite
EOF
}

log() { printf '\033[1m==> %s\033[0m\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || die "$1 not found. $2"
}

# ---- web helpers ---------------------------------------------------------

web_install() {
  need yarn "Install Node, then run: corepack enable"
  log "yarn install (web)"
  (cd "$WEB" && yarn install)
}

# Install when node_modules is missing or older than yarn.lock / package.json.
ensure_web_deps() {
  local state="$WEB/node_modules/.yarn-state.yml"
  if [[ ! -f "$state" || "$WEB/yarn.lock" -nt "$state" || "$WEB/package.json" -nt "$state" ]]; then
    web_install
  fi
}

wasm_is_stale() {
  [[ -f "$WASM_FILE" ]] || return 0
  local src
  for src in "${WASM_SOURCES[@]}"; do
    [[ -e "$src" ]] || continue
    if [[ -d "$src" ]]; then
      if [[ -n "$(find "$src" -type f \( -name '*.rs' -o -name '*.toml' -o -name '*.json' \) \
          -not -path '*/target/*' -newer "$WASM_FILE" -print -quit)" ]]; then
        return 0
      fi
    elif [[ "$src" -nt "$WASM_FILE" ]]; then
      return 0
    fi
  done
  return 1
}

cmd_wasm() {
  log "build pxl-wasm"
  bash "$ROOT/scripts/build-wasm.sh" "$@"
}

cmd_web_engine() {
  log "build pxl-web"
  bash "$ROOT/scripts/build-web-engine.sh" "$@"
}

ensure_wasm() {
  if wasm_is_stale; then
    cmd_wasm
  else
    log "wasm is up to date ($(du -k "$WASM_FILE" | cut -f1) KiB)"
  fi
}

parse_serve_args() {
  SERVE_ARGS=()
  OPEN=1
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --port) [[ $# -ge 2 ]] || die "--port needs a number"; SERVE_ARGS+=(--port "$2"); shift 2 ;;
      --host) SERVE_ARGS+=(--host); shift ;;
      --no-open) OPEN=0; shift ;;
      *) die "unknown option '$1'" ;;
    esac
  done
}

# ---- commands ------------------------------------------------------------

cmd_dev() {
  parse_serve_args "$@"
  need cargo "Install Rust from https://rustup.rs"
  ensure_web_deps
  ensure_wasm
  [[ $OPEN -eq 1 ]] && SERVE_ARGS+=(--open)
  log "vite dev server (Ctrl+C to stop; re-run './build.sh wasm' after Rust changes)"
  cd "$WEB"
  exec yarn dev "${SERVE_ARGS[@]}"
}

cmd_web() {
  ensure_web_deps
  ensure_wasm
  log "production build"
  (cd "$WEB" && yarn build)
  log "output: $WEB/dist"
}

cmd_preview() {
  parse_serve_args "$@"
  [[ -f "$WEB/dist/index.html" ]] || cmd_web
  [[ $OPEN -eq 1 ]] && SERVE_ARGS+=(--open)
  log "vite preview"
  cd "$WEB"
  exec yarn preview "${SERVE_ARGS[@]}"
}

cmd_pxl() {
  need cargo "Install Rust from https://rustup.rs"
  local profile=debug flags=()
  if [[ "${1:-}" == "--release" ]]; then
    profile=release
    flags=(--release)
  elif [[ -n "${1:-}" ]]; then
    die "unknown option '$1'"
  fi
  log "cargo build -p pxl-cli ($profile)"
  (cd "$ROOT" && cargo build -p pxl-cli ${flags[@]+"${flags[@]}"})
  log "binary: $ROOT/target/$profile/step-300"
}

cmd_run() {
  need cargo "Install Rust from https://rustup.rs"
  [[ $# -gt 0 ]] || die "run needs pxl arguments, e.g. ./build.sh run list macros"
  (cd "$ROOT" && cargo build -q -p pxl-cli)
  exec "$ROOT/target/debug/step-300" "$@"
}

cmd_png() {
  # gen-pxlgen.sh builds the pxl CLI itself (unless -B) and handles --help.
  bash "$ROOT/scripts/gen-pxlgen.sh" "$@"
}

cmd_web_test() {
  need node "Install Node.js for the JS runtime tests"
  need cargo "Install Rust from https://rustup.rs"
  log "JS runtime and audio adapter tests"
  (cd "$ROOT/steps/step-500-engine/runtime-js" && node --test tests/*.test.js)
  (cd "$ROOT/steps/step-300-asset-build/audio-adapter" && npm test)
  log "step-500 integration tests"
  (cd "$ROOT" && cargo test -p step-500-engine)
}

cmd_test() {
  need cargo "Install Rust from https://rustup.rs"
  log "cargo test (rust workspace)"
  (cd "$ROOT" && cargo test \
    -p pxl-core -p pxl-format -p pxl-gen -p pxl-ops -p pxl-qa \
    -p pxl-export -p pxl-art -p pxl-svg -p pxl-engine -p pxl-wasm -p pxl-audio -p pxl-cli \
    -p step-100-design -p step-200-art-spec -p step-400-package -p step-500-engine -p pxl-web \
    -p pxl-agent -p pxl-store -p pxl-contract)
  (cd "$ROOT" && cargo check --target wasm32-unknown-unknown -p pxl-web)
  if [[ -d "$WEB/node_modules" ]]; then
    log "web unit tests"
    (cd "$WEB" && yarn test)
  else
    log "skip web tests: run './build.sh install' first"
  fi
}

cmd_check() {
  need cargo "Install Rust from https://rustup.rs"
  log "cargo check (native)"
  (cd "$ROOT" && cargo check -p pxl-engine -p pxl-wasm -p pxl-cli)
  log "cargo check (wasm32-unknown-unknown)"
  (cd "$ROOT" && cargo check --target wasm32-unknown-unknown -p pxl-engine -p pxl-wasm -p pxl-web)
  if [[ -d "$WEB/node_modules" ]]; then
    log "vue-tsc"
    (cd "$WEB" && yarn typecheck)
  else
    log "skip vue-tsc: run './build.sh install' first"
  fi
}

cmd_skill_bins() {
  bash "$ROOT/scripts/build-skill-bins.sh" "$@"
}

cmd_skill_check() {
  need python3 "Install Python 3."
  python3 "$ROOT/scripts/tools/verify_skill.py" "$@"
}

cmd_all() {
  cmd_pxl --release
  cmd_wasm
  cmd_web
}

cmd_clean() {
  log "remove web/dist and web/src/wasm"
  rm -rf "$WEB/dist" "$WASM_OUT"
  if [[ "${1:-}" == "--all" ]]; then
    need cargo "Install Rust from https://rustup.rs"
    log "cargo clean"
    (cd "$ROOT" && cargo clean)
  fi
}

main() {
  local cmd="${1:-help}"
  [[ $# -gt 0 ]] && shift
  case "$cmd" in
    dev) cmd_dev "$@" ;;
    wasm) cmd_wasm "$@" ;;
    web) cmd_web "$@" ;;
    web-engine) cmd_web_engine "$@" ;;
    web-test) cmd_web_test ;;
    preview) cmd_preview "$@" ;;
    install) web_install ;;
    pxl) cmd_pxl "$@" ;;
    run) cmd_run "$@" ;;
    png) cmd_png "$@" ;;
    skill-bins) cmd_skill_bins "$@" ;;
    skill-check) cmd_skill_check "$@" ;;
    test) cmd_test ;;
    check) cmd_check ;;
    all) cmd_all ;;
    clean) cmd_clean "$@" ;;
    help|-h|--help) usage ;;
    *) usage >&2; die "unknown command '$cmd'" ;;
  esac
}

main "$@"
