#!/usr/bin/env bash
# Install the prebuilt binaries used by scripts/skills/pxl-asset-gen/bin/.
#
# This script never compiles. With no --from it installs what the repo already
# has (target/.../release/step-300 per platform); --from copies from a dir or
# URL base; --release pulls GitHub release assets. Every artifact is checked
# before install: a Linux build needing a newer glibc than the host has (or not
# statically linked) is rejected, not shipped.
#
#   scripts/build-pxl-asset-gen.sh --check
#   scripts/build-pxl-asset-gen.sh                              # from local target/
#   scripts/build-pxl-asset-gen.sh --from <dir|url-base> [--all | --platform <p>]
#   scripts/build-pxl-asset-gen.sh --release --platform linux
#
# Platforms: darwin-arm64 | darwin-x64 | linux (x86_64) | linux-arm64
# Artifact names, identical in every source:
#   pxl-asset-gen-darwin-arm64   pxl-asset-gen-darwin-x64
#   pxl-asset-gen-linux          x86_64, must be static (musl)
#   pxl-asset-gen-linux-arm64    aarch64, must be static
#
# Env:
#   PXL_ASSET_GEN_SOURCE     same as --from (dir or URL base)
#   PXL_ASSET_GEN_OUT        install dir (default: <repo>/scripts/skills/pxl-asset-gen/bin)
#   PXL_ASSET_GEN_GLIBC_MAX  glibc ceiling for dynamic Linux builds (default: host glibc, else 2.28)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${PXL_ASSET_GEN_OUT:-$ROOT/scripts/skills/pxl-asset-gen/bin}"
GLIBC_MAX="${PXL_ASSET_GEN_GLIBC_MAX:-}"
GLIBC_MAX_SET=$([[ -n "$GLIBC_MAX" ]] && echo 1 || echo 0)
SOURCE="${PXL_ASSET_GEN_SOURCE:-}"
RELEASE_URL="https://github.com/linkxzhou/PixelGames/releases/latest/download"
ALL_PLATFORMS="darwin-arm64 darwin-x64 linux linux-arm64"
MODE=install
PLATFORMS=()

die() { echo "error: $*" >&2; exit 1; }
sha256_of() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null; then shasum -a 256 "$1" | cut -d' ' -f1
  else openssl dgst -sha256 -r "$1" | cut -d' ' -f1
  fi
}
artifact_of() { if [[ "$1" == linux ]]; then echo pxl-asset-gen-linux; else echo "pxl-asset-gen-$1"; fi; }
host_platform() {
  case "$(uname -s)/$(uname -m)" in
    Darwin/arm64) echo darwin-arm64 ;;
    Darwin/x86_64) echo darwin-x64 ;;
    Linux/x86_64|Linux/amd64) echo linux ;;
    Linux/aarch64|Linux/arm64) echo linux-arm64 ;;
    *) die "unsupported host: $(uname -s)/$(uname -m); pass --platform explicitly" ;;
  esac
}
host_glibc() {
  [[ "$(uname -s)" == Linux ]] || return 0
  ldd --version 2>/dev/null | awk 'NR == 1 && /[0-9]+\.[0-9]+/ { print $NF; exit }'
}
local_artifact() { # platform → absolute path of an existing target/ build, else empty
  local p="$1" host cands c
  host=$(host_platform 2>/dev/null || echo none)
  case "$p" in
    linux) cands="target/x86_64-unknown-linux-musl/release/step-300 target/x86_64-unknown-linux-gnu/release/step-300" ;;
    linux-arm64) cands="target/aarch64-unknown-linux-musl/release/step-300 target/aarch64-unknown-linux-gnu/release/step-300" ;;
    darwin-arm64) cands="target/aarch64-apple-darwin/release/step-300" ;;
    darwin-x64) cands="target/x86_64-apple-darwin/release/step-300" ;;
    *) return 0 ;;
  esac
  [[ "$p" == "$host" ]] && cands="target/release/step-300 $cands"
  for c in $cands; do
    [[ -f "$ROOT/$c" ]] && { echo "$ROOT/$c"; return 0; }
  done
  return 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --from) SOURCE="${2:?--from needs a dir or URL}"; shift 2 ;;
    --release) SOURCE="$RELEASE_URL"; shift ;;
    --platform) PLATFORMS+=("${2:?--platform needs a value}"); shift 2 ;;
    --all) PLATFORMS=($ALL_PLATFORMS); shift ;;
    --glibc-max) GLIBC_MAX="${2:?--glibc-max needs a version}"; GLIBC_MAX_SET=1; shift 2 ;;
    --check) MODE=check; shift ;;
    -h|--help) sed -n '2,24p' "$0"; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
done

if [[ "$GLIBC_MAX_SET" == 1 ]]; then
  GLIBC_MAX_SRC="flag/env"
elif [[ -n "$(host_glibc)" ]]; then
  GLIBC_MAX="$(host_glibc)"; GLIBC_MAX_SRC="host glibc"
else
  GLIBC_MAX=2.28; GLIBC_MAX_SRC="default"
fi

# ELF: arch|linkage|interp  (static means no PT_INTERP, i.e. musl static build)
elf_report() {
  python3 - "$1" <<'PY'
import struct, sys
b = open(sys.argv[1], 'rb').read()
if b[:4] != b'\x7fELF':
    print('not-elf'); sys.exit()
e = '<' if b[5:6] == b'\x01' else '>'
wide = b[4:5] == b'\x02'
if wide:
    phoff, = struct.unpack_from(e + 'Q', b, 0x20)
    phentsize, phnum = struct.unpack_from(e + 'HH', b, 0x36)
else:
    phoff, = struct.unpack_from(e + 'I', b, 0x1c)
    phentsize, phnum = struct.unpack_from(e + 'HH', b, 0x2a)
machine, = struct.unpack_from(e + 'H', b, 0x12)
arch = {0x3e: 'x86-64', 0xb7: 'aarch64'}.get(machine, 'machine-%d' % machine)
interp = ''
for i in range(phnum):
    off = phoff + i * phentsize
    if struct.unpack_from(e + 'I', b, off)[0] != 3:
        continue
    if wide:
        poff, = struct.unpack_from(e + 'Q', b, off + 0x08)
        psz, = struct.unpack_from(e + 'Q', b, off + 0x20)
    else:
        poff, = struct.unpack_from(e + 'I', b, off + 0x04)
        psz, = struct.unpack_from(e + 'I', b, off + 0x10)
    interp = b[poff:poff + psz].rstrip(b'\0').decode('utf-8', 'replace')
    break
print('%s|%s|%s' % (arch, 'dynamic' if interp else 'static', interp))
PY
}
macho_report() {
  python3 - "$1" <<'PY'
import struct, sys
b = open(sys.argv[1], 'rb').read()
e = {b'\xcf\xfa\xed\xfe': '<', b'\xce\xfa\xed\xfe': '<',
     b'\xfe\xed\xfa\xcf': '>', b'\xfe\xed\xfa\xce': '>'}.get(b[:4])
if not e or len(b) < 8:
    print('not-macho'); sys.exit()
cpu, = struct.unpack_from(e + 'i', b, 4)
print({0x0100000c: 'arm64', 0x01000007: 'x86_64'}.get(cpu, 'cpu-%d' % cpu))
PY
}
glibc_ceiling() {
  local v
  v=$(grep -a -o 'GLIBC_2\.[0-9]\+' "$1" 2>/dev/null | sed 's/^GLIBC_//' | sort -V | tail -1 || true)
  printf '%s' "${v:-none}"
}
ver_le() { [[ "$1" == none ]] && return 0; [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -1)" == "$2" ]]; }

ERR=""; ARCH=""; LINK=""
check_file() { # platform path → 0 ok, ERR explains otherwise
  local p="$1" f="$2" rep want
  ERR=""; ARCH=""; LINK=""
  [[ -f "$f" ]] || { ERR="missing"; return 1; }
  case "$p" in
    linux|linux-arm64)
      rep=$(elf_report "$f")
      [[ "$rep" == not-elf ]] && { ERR="not an ELF executable"; return 1; }
      IFS='|' read -r ARCH LINK rep <<<"$rep"
      want=$([[ "$p" == linux ]] && echo x86-64 || echo aarch64)
      [[ "$ARCH" == "$want" ]] || { ERR="arch $ARCH, want $want"; return 1; }
      if [[ "$LINK" == dynamic ]]; then
        local ceil; ceil=$(glibc_ceiling "$f")
        ver_le "$ceil" "$GLIBC_MAX" || {
          ERR="dynamic build needs glibc <=$ceil, host allows <=$GLIBC_MAX (interp ${rep:-?}); ship a static x86_64-unknown-linux-musl build"
          return 1; }
        LINK="dynamic/glibc<=$ceil"
      else
        LINK=static
      fi ;;
    darwin-arm64|darwin-x64)
      ARCH=$(macho_report "$f")
      [[ "$ARCH" == not-macho ]] && { ERR="not a Mach-O executable"; return 1; }
      want=${p#darwin-}; [[ "$want" == x64 ]] && want=x86_64
      [[ "$ARCH" == "$want" ]] || { ERR="arch $ARCH, want $want"; return 1; }
      LINK=macho ;;
  esac
  return 0
}

fetch() { # name dest
  if [[ "$SOURCE" == *'://'* ]]; then
    curl -fsSL --retry 2 -o "$2" "$SOURCE/$1" 2>/dev/null || return 1
  else
    [[ -f "$SOURCE/$1" ]] || return 1
    cp "$SOURCE/$1" "$2"
  fi
}

SUMS_CACHE=""
load_sums() {
  [[ -n "$SOURCE" ]] || return 0
  SUMS_CACHE=$(mktemp)
  fetch SHA256SUMS "$SUMS_CACHE" || { rm -f "$SUMS_CACHE"; SUMS_CACHE=""; }
}
verify_sum() { # name file
  [[ -n "$SUMS_CACHE" ]] || return 0
  local want got
  want=$(awk -v n="$1" '$2 == n { print $1; exit }' "$SUMS_CACHE")
  [[ -z "$want" ]] && { echo "warn: $1 is not listed in the source SHA256SUMS" >&2; return 0; }
  got=$(sha256_of "$2")
  [[ "$got" == "$want" ]] || { echo "error: $1 sha256 $got != $want" >&2; return 1; }
  return 0
}

install_platform() {
  local p="$1" name tmp src origin
  name=$(artifact_of "$p")
  tmp=$(mktemp)
  if [[ -n "$SOURCE" ]]; then
    if ! fetch "$name" "$tmp"; then
      rm -f "$tmp"
      echo "FAIL $name: not found in $SOURCE" >&2
      return 1
    fi
    origin="$SOURCE/$name"
  else
    src=$(local_artifact "$p")
    if [[ -z "$src" ]]; then
      rm -f "$tmp"
      echo "FAIL $name: no local target/ build for $p" >&2
      return 1
    fi
    cp "$src" "$tmp"
    origin="${src#"$ROOT/"}"
  fi
  if ! check_file "$p" "$tmp"; then
    rm -f "$tmp"
    echo "FAIL $name ($origin): $ERR" >&2
    return 1
  fi
  verify_sum "$name" "$tmp" || { rm -f "$tmp"; return 1; }
  mkdir -p "$OUT"
  cp "$tmp" "$OUT/$name"
  rm -f "$tmp"
  chmod 755 "$OUT/$name"
  if [[ "$p" == "$(host_platform 2>/dev/null || echo none)" ]] && command -v strip >/dev/null; then
    strip "$OUT/$name" 2>/dev/null || true
  fi
  printf '%-26s %-8s %-18s %s  (%s)\n' "$name" "$ARCH" "$LINK" "$(sha256_of "$OUT/$name")" "$origin"
  printf '%s  %s\n' "$(sha256_of "$OUT/$name")" "$name" >> "$SUMS_TMP"
}

check_platform() {
  local p="$1" name f
  name=$(artifact_of "$p"); f="$OUT/$name"
  if [[ ! -f "$f" ]]; then
    printf '%-26s %-8s %-18s %s\n' "$name" "-" "-" "skip: not installed"
    return 0
  fi
  if check_file "$p" "$f"; then
    printf '%-26s %-8s %-18s %s\n' "$name" "$ARCH" "$LINK" "$(sha256_of "$f")"
    return 0
  fi
  printf '%-26s %-8s %-18s %s\n' "$name" "-" "-" "FAIL: $ERR"
  return 1
}

if [[ "$MODE" == install ]]; then
  echo "source: ${SOURCE:-local target/ builds}"
  echo "glibc-max for dynamic Linux builds: $GLIBC_MAX ($GLIBC_MAX_SRC)"
fi
echo "out:    $OUT"

bad=0
if [[ "$MODE" == check ]]; then
  [[ ${#PLATFORMS[@]} -gt 0 ]] || PLATFORMS=($ALL_PLATFORMS)
  for p in "${PLATFORMS[@]}"; do check_platform "$p" || bad=1; done
  [[ "$bad" -eq 0 ]] || { echo "one or more binaries cannot run on this host" >&2; exit 1; }
  exit 0
fi

[[ ${#PLATFORMS[@]} -gt 0 ]] || PLATFORMS=("$(host_platform)")
load_sums
SUMS_TMP=$(mktemp)
for p in "${PLATFORMS[@]}"; do install_platform "$p" || bad=1; done
if [[ "$bad" -eq 0 ]]; then
  mkdir -p "$OUT"
  LC_ALL=C sort -k2 "$SUMS_TMP" > "$OUT/SHA256SUMS"
fi
rm -f "$SUMS_TMP"
if [[ "$bad" -ne 0 ]]; then
  echo "install incomplete" >&2
  [[ -n "$SOURCE" ]] || cat >&2 <<'EOF'
no local target/ artifact for that platform. This script never compiles; feed it
artifacts built elsewhere:
  scripts/build-pxl-asset-gen.sh --from <dir|url-base>   # flat files or SHA256SUMS dir
  scripts/build-pxl-asset-gen.sh --release               # GitHub release assets
EOF
  exit 1
fi
