#!/usr/bin/env bash
# Build pxl-wasm for the browser and write the JS glue into web/src/wasm/.
#
#   scripts/build-wasm.sh            release build (small, what the page uses)
#   scripts/build-wasm.sh --dev      debug build (fast, large)
#
# Needs: rustup target wasm32-unknown-unknown, and a wasm-bindgen CLI whose
# version equals the `wasm-bindgen` crate pinned in steps/step-300-asset-build/pxl-wasm/Cargo.toml.
# wasm-opt (binaryen) is used when present and skipped otherwise.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$root/web/src/wasm"
profile="release"
if [[ "${1:-}" == "--dev" ]]; then
  profile="debug"
fi

want="$(sed -n 's/^wasm-bindgen = "=\(.*\)"$/\1/p' "$root/steps/step-300-asset-build/pxl-wasm/Cargo.toml")"
if [[ -z "$want" ]]; then
  echo "error: cannot read the pinned wasm-bindgen version from steps/step-300-asset-build/pxl-wasm/Cargo.toml" >&2
  exit 1
fi
if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "error: wasm-bindgen CLI not found. Install it with:" >&2
  echo "  cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi
have="$(wasm-bindgen --version | awk '{print $2}')"
if [[ "$have" != "$want" ]]; then
  echo "error: wasm-bindgen CLI is $have but the crate is pinned to $want." >&2
  echo "  cargo install wasm-bindgen-cli --version $want --locked --force" >&2
  exit 1
fi
if ! rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
  echo "error: missing target. Run: rustup target add wasm32-unknown-unknown" >&2
  exit 1
fi

cd "$root"
if [[ "$profile" == "release" ]]; then
  # Size-oriented settings apply to this build only; the workspace profiles stay as they are.
  export CARGO_PROFILE_RELEASE_OPT_LEVEL=z
  export CARGO_PROFILE_RELEASE_LTO=true
  export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
  export CARGO_PROFILE_RELEASE_PANIC=abort
  export CARGO_PROFILE_RELEASE_STRIP=symbols
  cargo build --release --target wasm32-unknown-unknown -p pxl-wasm
else
  cargo build --target wasm32-unknown-unknown -p pxl-wasm
fi

wasm="$root/target/wasm32-unknown-unknown/$profile/pxl_wasm.wasm"
rm -rf "$out"
mkdir -p "$out"
wasm-bindgen --target web --out-dir "$out" --out-name pxl_wasm "$wasm"

if [[ "$profile" == "release" ]] && command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
    -o "$out/pxl_wasm_bg.wasm" "$out/pxl_wasm_bg.wasm"
fi

bytes="$(wc -c < "$out/pxl_wasm_bg.wasm" | tr -d ' ')"
echo "wrote $out"
echo "pxl_wasm_bg.wasm: $bytes bytes ($((bytes / 1024)) KiB)"
