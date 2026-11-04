#!/usr/bin/env python3
import argparse
from pathlib import Path
import re


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--vendor", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    source = (args.vendor / "src/native/wasm/keycodes.rs").read_text()
    event = (args.vendor / "src/event.rs").read_text()
    keys = re.findall(r"^\s*(\d+)\s*=>\s*KeyCode::(\w+),", source, re.M)
    enum = re.search(r"pub enum KeyCode\s*\{(.*?)\n\}", event, re.S).group(1)
    values = {name: value for name, value in re.findall(r"^\s*(\w+)\s*=\s*([^,]+),", enum, re.M)}
    key_rows = "\n".join(f"| {code} | `{name}` | `{values.get(name, '?')}` |" for code, name in keys)
    text = f"""# Events and keys

## Contents

- Event delivery
- Key and modifier mapping
- Mouse and touch

`gl.js` calls wasm exports, which dispatch `EventHandler`. See `rust-api.md` for the complete callback signatures.

## Key and modifier mapping

Modifier bits are Shift=1, Ctrl=2, Alt=4, Super/logo=8. `KeyMods` exposes `shift`, `ctrl`, `alt`, and `logo` booleans. Unknown sapp key codes become `KeyCode::Unknown`.

| sapp code | KeyCode | Rust value |
|---:|---|---:|
{key_rows}

## Mouse and touch

Mouse buttons are `Left=0`, `Middle=1`, `Right=2`, and `Unknown=255` in Rust; the browser mapping translates sapp 0/1/2 to Left/Right/Middle. Wheel deltas arrive as `mouse_wheel_event(x, y)`.

Touch phases are sapp 10 Started, 11 Moved, 12 Ended, and 13 Cancelled. The default `touch_event` implementation emulates left-button presses and mouse motion. Focus loss/restoration maps to `window_minimized_event`/`window_restored_event`.
"""
    path = args.out / "events-and-keys.md"
    if args.check:
        if not path.is_file() or path.read_text() != text:
            raise SystemExit("stale generated reference: events-and-keys.md")
        print("miniquad event reference is current")
    else:
        args.out.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        print(f"extracted {len(keys)} sapp key mappings")


if __name__ == "__main__":
    main()
