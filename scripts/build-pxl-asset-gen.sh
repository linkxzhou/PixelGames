#!/usr/bin/env bash
# Build the standalone binaries bundled in scripts/skills/pxl-asset-gen/bin/.
#
#   scripts/build-pxl-asset-gen.sh            # host platform only
#   scripts/build-pxl-asset-gen.sh --linux    # also Linux x86_64 (musl) via cargo-zigbuild, if on macOS
#
# Output names (what the skill expects):
#   bin/pxl-asset-gen-darwin-arm64   bin/pxl-asset-gen-darwin-x64
#   bin/pxl-asset-gen-linux          (x86_64)   bin/pxl-asset-gen-linux-arm64
#
# The binary is libs/pxl-cli (`step-300`) built in release mode with Rust 1.95
# (required by the pxl-ase Rhai scripting used by `ase`). The skill itself ships
# no build scripts or sources; rerun this after changing libs/ and commit/copy bin/.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/scripts/skills/pxl-asset-gen/bin"
TOOLCHAIN="${PXL_TOOLCHAIN:-+1.95.0}"
WANT_LINUX=0
for arg in "$@"; do
  case "$arg" in
    --linux) WANT_LINUX=1 ;;
    -h|--help) sed -n 2,14p "$0"; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

host_name() {
  local os arch
  case "$(uname -s)" in Darwin) os=darwin ;; Linux) os=linux ;; *) echo "unsupported OS" >&2; exit 1 ;; esac
  case "$(uname -m)" in arm64|aarch64) arch=arm64 ;; x86_64|amd64) arch=x64 ;; *) echo "unsupported arch" >&2; exit 1 ;; esac
  if [[ $os == linux && $arch == x64 ]]; then echo "pxl-asset-gen-linux"; else echo "pxl-asset-gen-$os-$arch"; fi
}

sha256_of() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null; then shasum -a 256 "$1" | cut -d' ' -f1
  else openssl dgst -sha256 -r "$1" | cut -d' ' -f1
  fi
}

install_bin() {
  local src="$1" name="$2"
  mkdir -p "$OUT"
  cp "$src" "$OUT/$name"
  chmod +x "$OUT/$name"
  if command -v strip >/dev/null && [[ "$name" == "$(host_name)" ]]; then strip "$OUT/$name" 2>/dev/null || true; fi
  printf '%s  %s\n' "$(sha256_of "$OUT/$name")" "$name"
}

cd "$ROOT"
cargo "$TOOLCHAIN" build --release -p pxl-cli --bin step-300
install_bin target/release/step-300 "$(host_name)"

if [[ $WANT_LINUX == 1 && "$(uname -s)" != Linux ]]; then
  command -v cargo-zigbuild >/dev/null || { echo "--linux needs cargo-zigbuild + zig (cargo install cargo-zigbuild)" >&2; exit 1; }
  rustup "$TOOLCHAIN" target add x86_64-unknown-linux-musl >/dev/null
  cargo "$TOOLCHAIN" zigbuild --release -p pxl-cli --bin step-300 --target x86_64-unknown-linux-musl
  install_bin target/x86_64-unknown-linux-musl/release/step-300 pxl-asset-gen-linux
fi

"$OUT/$(host_name)" --version
