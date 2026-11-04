#!/usr/bin/env python3
import argparse
import os
from pathlib import Path
import shutil
import subprocess

EXAMPLES = {
    "01-clear": ("mq-example-clear", "clear.wasm"),
    "02-triangle": ("mq-example-triangle", "triangle.wasm"),
    "03-input": ("mq-example-input", "input.wasm"),
    "04-texture-nearest": ("mq-example-texture", "texture.wasm"),
    "05-plugin-js-call": ("mq-example-plugin", "plugin.wasm"),
    "06-fixed-step-js-loop": ("mq-example-fixed-step", "fixed_step.wasm"),
    "07-file-loading": ("mq-example-file-loading", "file_loading.wasm"),
}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True, type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    examples = Path(__file__).resolve().parent
    target = root / "target/miniquad-skill-examples"
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_PROFILE_RELEASE_OPT_LEVEL="z",
               CARGO_PROFILE_RELEASE_LTO="true", CARGO_PROFILE_RELEASE_CODEGEN_UNITS="1",
               CARGO_PROFILE_RELEASE_PANIC="abort", CARGO_PROFILE_RELEASE_STRIP="symbols")
    for directory, (package, output) in EXAMPLES.items():
        path = examples / directory
        print(f"build {directory}", flush=True)
        subprocess.run(["cargo", "build", "--release", "--target", "wasm32-unknown-unknown",
                        "--manifest-path", str(path / "Cargo.toml"), "-p", package], cwd=root, env=env, check=True)
        wasm = target / "wasm32-unknown-unknown/release" / f"{package.replace('-', '_')}.wasm"
        shutil.copyfile(wasm, path / output)
        shutil.copyfile(root / "vendor/miniquad/js/gl.js", path / "gl.js")
        subprocess.run(["python3", str(examples.parent.parent / "scripts/check_wasm_imports.py"),
                        str(path / output), "--interface", str(examples.parent.parent / "references/wasm-interface.json"),
                        "--html", str(path / "index.html")], check=True)
    subprocess.run(["node", "--test", str(examples / "tests/js.test.js")], check=True)


if __name__ == "__main__":
    main()
