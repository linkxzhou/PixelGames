#!/usr/bin/env bash
# Launcher for the bundled pipeline binaries (macOS / Linux).
#
#   run.sh <step-100|step-200|step-300|step-400|step-500> [args...]
#   run.sh --platform           print the detected platform and exit
#   run.sh --verify             check bin/manifest.json hashes for this platform
#   run.sh --dev <step> [args]  use target/release/<step> from the repo (stale builds possible)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
skill="$(cd "$here/.." && pwd)"

die() { printf 'run.sh: %s\n' "$*" >&2; exit 127; }

detect_platform() {
  local os arch
  case "$(uname -s)" in
    Darwin) os=darwin ;;
    Linux) os=linux ;;
    *) die "unsupported OS '$(uname -s)'; on Windows use run.ps1" ;;
  esac
  case "$(uname -m)" in
    arm64|aarch64) arch=arm64 ;;
    x86_64|amd64) arch=x64 ;;
    *) die "unsupported architecture '$(uname -m)'" ;;
  esac
  echo "$os-$arch"
}

platform="$(detect_platform)"
bin_dir="$skill/bin/$platform"

available() {
  if [[ -d "$skill/bin" ]]; then
    find "$skill/bin" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort | tr '\n' ' '
  fi
}

case "${1:-}" in
  --platform) echo "$platform"; exit 0 ;;
  --verify)
    [[ -f "$skill/bin/manifest.json" ]] || die "bin/manifest.json is missing"
    python3 - "$skill/bin" "$platform" <<'PY'
import hashlib, json, os, sys
bin_root, platform = sys.argv[1], sys.argv[2]
m = json.load(open(os.path.join(bin_root, "manifest.json")))
files = m["platforms"].get(platform)
if not files:
    sys.exit("run.sh: manifest has no entry for %s" % platform)
bad = 0
for name, want in files.items():
    path = os.path.join(bin_root, platform, name)
    if not os.path.isfile(path):
        print("MISSING %s" % name); bad += 1; continue
    h = hashlib.sha256(open(path, "rb").read()).hexdigest()
    if "sha256:" + h != want:
        print("MISMATCH %s" % name); bad += 1
    else:
        print("ok %s" % name)
sys.exit(1 if bad else 0)
PY
    exit $?
    ;;
esac

if [[ "${1:-}" == "serve" ]]; then
  [[ $# -ge 2 ]] || die "usage: run.sh serve <dir> [--port N]"
  dir="$2"; shift 2
  port=8000
  if [[ "${1:-}" == "--port" ]]; then
    [[ $# -ge 2 ]] || die "--port needs a number"
    port="$2"; shift 2
  fi
  [[ $# -eq 0 ]] || die "unknown serve option '$1'"
  [[ -d "$dir" ]] || die "serve directory '$dir' does not exist"
  command -v python3 >/dev/null 2>&1 || die "python3 is required for serve"
  exec python3 -m http.server "$port" --directory "$dir"
fi

dev=0
if [[ "${1:-}" == "--dev" ]]; then dev=1; shift; fi

step="${1:-}"
[[ -n "$step" ]] || die "usage: run.sh [--dev] <step-100|step-200|step-300|step-400|step-500> [args...], or run.sh serve <dir> [--port N]"
shift
case "$step" in
  step-100|step-200|step-300|step-400|step-500) ;;
  *) die "unknown step '$step' (expected step-100 .. step-500)" ;;
esac

if [[ "$dev" -eq 1 ]]; then
  repo="$(cd "$skill/../../.." && pwd)"
  bin_dir="$repo/target/release"
  [[ -x "$bin_dir/$step" ]] || die "--dev: $bin_dir/$step not found; run: cargo build --release -p pxl-cli (and the step crates)"
  echo "run.sh: --dev using $bin_dir/$step" >&2
fi

if [[ ! -x "$bin_dir/$step" ]]; then
  die "no $step binary for platform '$platform' (bundled platforms: $(available)). Build one with: scripts/build-skill-bins.sh --target $platform"
fi

# Put the bundle first so step-200 finds step-300 without further setup.
export PATH="$bin_dir:$PATH"
if [[ "$step" == "step-300" && ( "${1:-}" == "audio" || "${1:-}" == "plan" ) && -z "${PXL_AUDIO_ADAPTER:-}" ]]; then
  has_adapter=0
  for a in "$@"; do [[ "$a" == "--adapter" || "$a" == --adapter=* ]] && has_adapter=1; done
  if [[ "$has_adapter" -eq 0 ]]; then
    export PXL_AUDIO_ADAPTER="$skill/assets/audio-toolchain"
  fi
fi
if [[ "$step" == "step-200" ]]; then
  has_bin=0
  for a in "$@"; do [[ "$a" == "--pxl-bin" || "$a" == --pxl-bin=* ]] && has_bin=1; done
  # --pxl-bin only exists on run / lint / capabilities.
  sub="${1:-}"
  if [[ "$has_bin" -eq 0 && ( "$sub" == run || "$sub" == lint || "$sub" == capabilities ) ]]; then
    exec "$bin_dir/$step" "$@" --pxl-bin "$bin_dir/step-300"
  fi
fi
if [[ "$step" == "step-500" && "${1:-}" == "build" ]]; then
  has_runtime=0
  for a in "$@"; do [[ "$a" == "--runtime-dir" || "$a" == --runtime-dir=* ]] && has_runtime=1; done
  if [[ "$has_runtime" -eq 0 ]]; then
    exec "$bin_dir/$step" "$@" --runtime-dir "$skill/assets/web-runtime"
  fi
fi
exec "$bin_dir/$step" "$@"
