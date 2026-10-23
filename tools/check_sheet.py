#!/usr/bin/env python3
"""Check a pxl spritesheet JSON against the Aseprite-format contract.

Usage: python3 tools/check_sheet.py <sheet.json> [expected_min_frames]

Exits 0 when the sheet satisfies the Aseprite JSON structure (frames,
frameTags, meta) and pxl's own extensions (meta.pxl). Exits 1 with a
message on the first violation. This mirrors the acceptance check in
plan/miniagent-integration.md §4-3.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def fail(msg: str) -> None:
    print(f"FAIL: {msg}")
    sys.exit(1)


def main() -> None:
    if len(sys.argv) < 2:
        fail("usage: check_sheet.py <sheet.json> [expected_min_frames]")
    path = Path(sys.argv[1])
    try:
        sheet = json.loads(path.read_text())
    except OSError as e:
        fail(f"cannot read {path}: {e}")
    except json.JSONDecodeError as e:
        fail(f"{path} is not valid JSON: {e}")

    frames = sheet.get("frames")
    if not isinstance(frames, dict):
        fail("frames must be an object keyed by frame name")
    for name, frame in frames.items():
        rect = frame.get("frame")
        if not isinstance(rect, dict) or not {"x", "y", "w", "h"} <= set(rect):
            fail(f"frame {name}: missing frame rect x/y/w/h")
        if "duration" not in frame:
            fail(f"frame {name}: missing duration")

    meta = sheet.get("meta")
    if not isinstance(meta, dict):
        fail("missing meta")
    size = meta.get("size")
    if not isinstance(size, dict) or "w" not in size or "h" not in size:
        fail("meta.size must carry w/h")

    tags = meta.get("frameTags")
    if not isinstance(tags, list) or not tags:
        fail("meta.frameTags must be a non-empty list")
    covered: set[int] = set()
    for tag in tags:
        for key in ("name", "from", "to", "direction"):
            if key not in tag:
                fail(f"tag missing {key}: {tag}")
        if tag["from"] > tag["to"]:
            fail(f"tag {tag['name']}: from > to")
        covered.update(range(tag["from"], tag["to"] + 1))
    if len(frames) > 0 and covered != set(range(len(frames))):
        missing = set(range(len(frames))) - covered
        fail(f"frameTags do not cover every frame index: missing {sorted(missing)[:8]}")

    pxl_meta = meta.get("pxl")
    if not isinstance(pxl_meta, dict):
        fail("meta.pxl is missing (pxl sheets always embed it)")
    for key in ("version", "palette"):
        if key not in pxl_meta:
            fail(f"meta.pxl.{key} is missing")

    if len(sys.argv) > 2:
        try:
            minimum = int(sys.argv[2])
        except ValueError:
            fail(f"expected_min_frames must be an integer: {sys.argv[2]}")
        if len(frames) < minimum:
            fail(f"{len(frames)} frames < expected minimum {minimum}")

    print(f"OK: {path.name}: {len(frames)} frames, {len(tags)} tags")
    sys.exit(0)


if __name__ == "__main__":
    main()
