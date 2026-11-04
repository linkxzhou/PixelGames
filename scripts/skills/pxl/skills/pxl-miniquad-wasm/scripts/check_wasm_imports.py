#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import re


def uleb(data, offset):
    value = shift = 0
    while True:
        byte = data[offset]
        offset += 1
        value |= (byte & 127) << shift
        if byte < 128:
            return value, offset
        shift += 7


def sections(data, wanted):
    offset = 8
    found = []
    while offset < len(data):
        section = data[offset]
        offset += 1
        size, offset = uleb(data, offset)
        end = offset + size
        if section == wanted:
            count, cursor = uleb(data, offset)
            for _ in range(count):
                if section == 2:
                    length, cursor = uleb(data, cursor)
                    module = data[cursor:cursor + length].decode()
                    cursor += length
                length, cursor = uleb(data, cursor)
                name = data[cursor:cursor + length].decode()
                cursor += length
                kind = data[cursor]
                cursor += 1
                _, cursor = uleb(data, cursor)
                found.append((module if section == 2 else "", name, kind))
        offset = end
    return found


def main():
    parser = argparse.ArgumentParser(description="Check wasm imports against miniquad gl.js and example plugins")
    parser.add_argument("wasm", type=Path)
    parser.add_argument("--interface", required=True, type=Path)
    parser.add_argument("--html", type=Path)
    args = parser.parse_args()
    data = args.wasm.read_bytes()
    if data[:4] != b"\0asm":
        raise SystemExit("not a wasm binary")
    interface = json.loads(args.interface.read_text())
    imports = sections(data, 2)
    exports = sections(data, 7)
    loader = set(interface["loader_functions"])
    custom = set()
    if args.html:
        custom = set(re.findall(r"(?:imports|importObject)\.env\.([A-Za-z_]\w*)", args.html.read_text()))
    missing = sorted(name for module, name, kind in imports if module == "env" and name not in loader and name not in custom)
    export_names = {name for _, name, _ in exports}
    required = {"main", "crate_version", "allocate_vec_u8", "frame", "key_down", "key_up", "resize"}
    missing_exports = sorted(required - export_names)
    print(f"imports: {len(imports)}, exports: {len(exports)}, plugin imports: {sorted(custom)}")
    if missing:
        print("Missed function: " + ", ".join(missing))
    if missing_exports:
        print("Missing exports: " + ", ".join(missing_exports))
    if missing or missing_exports:
        raise SystemExit(1)
    print("all wasm imports are covered; no Missed function")


if __name__ == "__main__":
    main()
