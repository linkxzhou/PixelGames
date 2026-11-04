#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import re


def signatures(text, pattern):
    return [" ".join(match.split()) for match in re.findall(pattern, text, re.S)]


def extract(vendor):
    native = (vendor / "src/native/wasm.rs").read_text()
    fs = (vendor / "src/native/wasm/fs.rs").read_text()
    webgl = (vendor / "src/native/wasm/webgl.rs").read_text()
    fs_public = (vendor / "src/fs.rs").read_text()
    graphics = (vendor / "src/graphics.rs").read_text()
    event = (vendor / "src/event.rs").read_text()
    loader = (vendor / "js/gl.js").read_text()
    exports = signatures(native + "\n" + fs_public, r"pub\s+extern\s+\"C\"\s+(fn\s+\w+\s*\([^)]*\)(?:\s*->\s*[^\{]+)?)\s*\{")
    imports = []
    for source in [native, fs, webgl]:
        for block in re.findall(r'extern\s+"C"\s*\{(.*?)\}', source, re.S):
            imports += signatures(block, r"(?:pub\s+)?(fn\s+\w+\s*\([^)]*\)(?:\s*->\s*[^;]+)?)\s*;")
    trait = re.search(r"pub trait RenderingBackend\s*\{(.*)\}\s*$", graphics, re.S).group(1)
    backend = signatures(trait, r"(fn\s+\w+\s*\([^;{]*\)(?:\s*->\s*[^;{]+)?)\s*(?:;|\{)")
    handler = re.search(r"pub trait EventHandler\s*\{(.*)\}\s*$", event, re.S).group(1)
    events = signatures(handler, r"(fn\s+\w+\s*\([^;{]*\)(?:\s*->\s*[^;{]+)?)\s*(?:;|\{)")
    loader_names = sorted(set(re.findall(r"^\s{8}([A-Za-z_]\w*):\s*(?:function|\w)", loader, re.M))
                          | set(re.findall(r"^\s{8}([A-Za-z_]\w*)\s*,?\s*$", loader, re.M)))
    return {
        "schema_version": 1,
        "miniquad_version": "0.4.6",
        "wasm_exports": sorted(exports),
        "js_imports": sorted(imports),
        "loader_functions": loader_names,
        "rendering_backend": backend,
        "event_handler": events,
    }


def render(data):
    export_lines = "\n".join(f"- `{item}`" for item in data["wasm_exports"] + ["fn main()"])
    import_lines = "\n".join(f"- `{item}`" for item in data["js_imports"])
    rust_lines = "\n".join(f"- `{item}`" for item in data["rendering_backend"])
    event_lines = "\n".join(f"- `{item}`" for item in data["event_handler"])
    return {
        "wasm-exports.md": f"# wasm exports\n\nGenerated from `src/native/wasm.rs` and `src/fs.rs`; run `extract_interfaces.py --check` after upgrades.\n\n{export_lines}\n",
        "js-imports.md": f"# JavaScript imports\n\n## Contents\n\n- Rust imports\n- Loader coverage\n\nGenerated from the `extern \"C\"` blocks in `wasm.rs`, `wasm/fs.rs`, and `wasm/webgl.rs`.\n\n## Rust imports\n\n{import_lines}\n\n## Loader coverage\n\n`check_wasm_imports.py` verifies these names against `js/gl.js`; plugin imports are checked against the example HTML.\n",
        "rust-api.md": f"# Rust API\n\n## Contents\n\n- Entry and configuration\n- EventHandler\n- RenderingBackend\n\nUse `miniquad::start(conf, || Box::new(handler))`. `Conf` contains `window_title`, `window_width`, `window_height`, `high_dpi`, `fullscreen`, `sample_count`, `window_resizable`, `icon`, and `platform`.\n\n## EventHandler\n\n{event_lines}\n\n## RenderingBackend\n\n{rust_lines}\n\nWindow functions are `screen_size`, `dpi_scale`, `high_dpi`, `blocking_event_loop`, `order_quit`, `quit`, `request_quit`, `cancel_quit`, `set_cursor_grab`, `schedule_update`, `show_mouse`, `set_mouse_cursor`, `set_window_size`, `set_window_position`, `set_fullscreen`, `clipboard_get/set`, `dropped_file_count/bytes/path`, `show_keyboard`, and `new_rendering_backend`. Apple-specific functions are unavailable on wasm.\n",
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--vendor", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    data = extract(args.vendor)
    documents = render(data)
    stale = []
    args.out.mkdir(parents=True, exist_ok=True)
    interface = json.dumps(data, indent=2) + "\n"
    if args.check:
        if not (args.out / "wasm-interface.json").is_file() or (args.out / "wasm-interface.json").read_text() != interface:
            stale.append("wasm-interface.json")
        stale += [name for name, text in documents.items() if not (args.out / name).is_file() or (args.out / name).read_text() != text]
        if stale:
            raise SystemExit("stale generated references: " + ", ".join(stale))
        print("miniquad interface references are current")
        return
    (args.out / "wasm-interface.json").write_text(interface)
    for name, text in documents.items():
        (args.out / name).write_text(text)
    print(f"extracted {len(data['wasm_exports'])} exports, {len(data['js_imports'])} imports, {len(data['rendering_backend'])} backend methods")


if __name__ == "__main__":
    main()
