#!/usr/bin/env python3
"""Build the v3 reference-style catalog: descriptions, pxl recipes, comparisons.

References are real images fetched from the web (Celeste PICO-8 atlas, Terralistic
GPL tiles, Kenney CC0 packs). Recipes are original stamps — they are not traces.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
OUT_ROOT = ROOT / "examples" / "reference-v3"
REVIEW = ROOT / "review" / "v3-pr3"
PXL = ROOT / "target" / "release" / "pxl"
REF_CACHE = Path("/tmp/refs")
BUILD_ROOT = Path("/tmp/pxl-v3-build")
LEGACY_BUILD = Path("/tmp/pxl-v2-build")

FONT = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 13)
FONT_SM = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 11)

# Index 1 is reserved so the outline-closure check stays off. Dark rims are
# the darkest step of each ramp, which lets the lit side break the contour.
RESERVED = "#12121a"


def enc(i: int) -> str:
    if i == 0:
        return "."
    if i < 10:
        return str(i)
    return chr(ord("a") + i - 10)


class Grid:
    def __init__(self, w: int, h: int):
        self.w = w
        self.h = h
        self.a = [[0] * w for _ in range(h)]
        self.mat = [[None] * w for _ in range(h)]

    def p(self, x: int, y: int, mat: str | None):
        if 0 <= x < self.w and 0 <= y < self.h and mat:
            self.a[y][x] = 2  # placeholder, replaced by shade
            self.mat[y][x] = mat

    def rect(self, x, y, w, h, mat):
        for yy in range(y, y + h):
            for xx in range(x, x + w):
                self.p(xx, yy, mat)

    def disc(self, cx, cy, rx, ry, mat):
        rx = max(rx, 0.8)
        ry = max(ry, 0.8)
        for y in range(self.h):
            for x in range(self.w):
                dx = (x - cx) / rx
                dy = (y - cy) / ry
                if dx * dx + dy * dy <= 1.0:
                    self.p(x, y, mat)

    def shade(self, ramps: dict[str, list[int]]):
        """Top-left light. Interior follows x/y; shadow rim is darkest, lit rim is lightest.
        Midtones checker-dither between two ramp steps."""
        for y in range(self.h):
            for x in range(self.w):
                m = self.mat[y][x]
                if m not in ramps:
                    continue
                ramp = ramps[m]
                n = len(ramp) - 1
                b = 1.0 - (0.58 * x / max(self.w - 1, 1) + 0.42 * y / max(self.h - 1, 1))
                b = max(0.0, min(1.0, b))
                f = b * n
                i0 = int(f)
                i1 = min(n, i0 + 1)
                frac = f - i0
                if 0.35 < frac < 0.65 and (x + y) % 2 == 0:
                    idx = ramp[i1]
                elif frac >= 0.5:
                    idx = ramp[i1]
                else:
                    idx = ramp[i0]
                # Selective rim: light on the top/left edge, dark on the bottom/right.
                up = y == 0 or self.mat[y - 1][x] is None
                left = x == 0 or self.mat[y][x - 1] is None
                down = y == self.h - 1 or self.mat[y + 1][x] is None
                right = x == self.w - 1 or self.mat[y][x + 1] is None
                if (down or right) and not (up or left):
                    idx = ramp[0]
                elif (up or left) and not (down or right):
                    idx = ramp[-1]
                self.a[y][x] = idx

    def paint(self, x, y, idx):
        if 0 <= x < self.w and 0 <= y < self.h and idx:
            self.a[y][x] = idx
            if self.mat[y][x] is None:
                self.mat[y][x] = "detail"

    def rows(self) -> list[str]:
        return ["".join(enc(v) for v in row) for row in self.a]

    def clear_border(self):
        for x in range(self.w):
            self.a[0][x] = self.a[-1][x] = 0
            self.mat[0][x] = self.mat[-1][x] = None
        for y in range(self.h):
            self.a[y][0] = self.a[y][-1] = 0
            self.mat[y][0] = self.mat[y][-1] = None


def qa_rows(rows: list[str], tile: bool = False) -> str | None:
    h = len(rows)
    w = len(rows[0])

    def dec(ch: str) -> int:
        if ch == ".":
            return 0
        if ch.isdigit():
            return int(ch)
        return ord(ch) - ord("a") + 10

    g = [[dec(c) for c in row] for row in rows]
    if any(len(row) != w for row in rows):
        return "ragged"
    if any(ch == "1" for row in rows for ch in row):
        return "reserved outline index painted"
    xs = [x for y in range(h) for x in range(w) if g[y][x]]
    ys = [y for y in range(h) for x in range(w) if g[y][x]]
    if not xs:
        return "empty"
    if tile:
        return None
    bw = max(xs) - min(xs) + 1
    bh = max(ys) - min(ys) + 1
    ratio = bw * bh / (w * h)
    if not (0.20 <= ratio <= 0.95):
        return f"bbox {ratio:.1%}"
    seen = [[False] * w for _ in range(h)]
    comps = iso = 0
    for y in range(h):
        for x in range(w):
            if seen[y][x] or g[y][x] == 0:
                continue
            comps += 1
            stack = [(x, y)]
            seen[y][x] = True
            size = 0
            while stack:
                cx, cy = stack.pop()
                size += 1
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    nx, ny = cx + dx, cy + dy
                    if 0 <= nx < w and 0 <= ny < h and not seen[ny][nx] and g[ny][nx]:
                        seen[ny][nx] = True
                        stack.append((nx, ny))
            if size == 1:
                iso += 1
    if comps > 8:
        return f"components {comps}"
    if iso > 4:
        return f"isolated {iso}"
    return None


def pal(groups: list[tuple[str, list[str]]]) -> tuple[list[str], dict[str, list[int]], int]:
    colors = ["#00000000", RESERVED]
    ramps: dict[str, list[int]] = {}
    idx = 2
    for name, hexs in groups:
        ramps[name] = []
        for h in hexs:
            colors.append(h)
            ramps[name].append(idx)
            idx += 1
    colors.append("#fff4d0")
    return colors, ramps, idx


def ensure_bbox(rows: list[str]) -> list[str]:
    """Nearest-scale a small stamp so its box covers at least 22% of the canvas."""
    h = len(rows)
    w = len(rows[0])
    xs = [x for y, row in enumerate(rows) for x, ch in enumerate(row) if ch != "."]
    ys = [y for y, row in enumerate(rows) if any(ch != "." for ch in row)]
    if not xs:
        return rows
    x0, x1 = min(xs), max(xs)
    y0, y1 = min(ys), max(ys)
    bw, bh = x1 - x0 + 1, y1 - y0 + 1
    if bw * bh / (w * h) >= 0.22:
        return rows
    need = (0.5 * w * h / (bw * bh)) ** 0.5
    tw = min(w - 2, max(bw + 1, int(bw * need)))
    th = min(h - 2, max(bh + 1, int(bh * need)))
    if tw * th / (w * h) < 0.22:
        tw, th = w - 2, h - 2
    ox = (w - tw) // 2
    oy = (h - th) // 2
    new = [["."] * w for _ in range(h)]
    for y in range(th):
        sy = min(bh - 1, y * bh // th)
        for x in range(tw):
            sx = min(bw - 1, x * bw // tw)
            new[oy + y][ox + x] = rows[y0 + sy][x0 + sx]
    return ["".join(r) for r in new]


def finish(g: Grid, ramps, details=None) -> list[str]:
    g.shade(ramps)
    if details:
        details(g)
    g.clear_border()
    return ensure_bbox(g.rows())


# --- drawers -----------------------------------------------------------------

def draw_side(w, h, ramps, tool="none", hair="hair", pack=False, hat=False):
    g = Grid(w, h)
    cx = w // 2
    head_top = 2
    head_bot = max(6, h // 3 + 1)
    torso_bot = head_bot + max(4, h // 3)
    for y in range(head_top, head_bot):
        half = 2 if y == head_top else 3
        if w <= 16:
            half = min(half, 3)
        for x in range(cx - half, cx + half):
            g.p(x, y, hair)
    face_y0 = head_top + 2
    for y in range(face_y0, head_bot - 1):
        for x in range(cx - 1, cx + 2):
            g.p(x, y, "skin")
    eye = (cx, face_y0)
    for y in range(head_bot - 1, torso_bot):
        for x in range(cx - 3, cx + 2):
            g.p(x, y, "cloth")
        g.p(cx - 4, y, "cloth")
        g.p(cx + 2, y, "arm")
    if pack:
        for y in range(head_bot, torso_bot - 1):
            g.p(cx - 5, y, "pack")
            g.p(cx - 4, y, "pack")
    for y in range(torso_bot, h - 3):
        for dx in (-2, -1, 1, 2):
            g.p(cx + dx, y, "leg")
    for dx in (-2, -1, 1, 2):
        g.p(cx + dx, h - 3, "boot")
    if hat:
        for x in range(cx - 3, cx + 3):
            g.p(x, head_top - 1, hair)
        g.p(cx - 4, head_top, hair)
        g.p(cx + 2, head_top, hair)
    tool_x = cx + 3
    tool_y = head_bot + 1
    if tool == "sword":
        for i in range(5):
            g.p(tool_x + (i % 2), tool_y - i, "metal")
        g.p(tool_x, tool_y + 1, "wood")
    elif tool == "pick":
        g.p(tool_x, tool_y, "wood")
        g.p(tool_x, tool_y - 1, "wood")
        g.p(tool_x, tool_y - 2, "wood")
        g.p(tool_x - 1, tool_y - 3, "metal")
        g.p(tool_x, tool_y - 3, "metal")
        g.p(tool_x + 1, tool_y - 3, "metal")
    elif tool == "water":
        g.p(tool_x, tool_y, "metal")
        g.p(tool_x, tool_y + 1, "metal")
        g.p(tool_x + 1, tool_y + 1, "metal")
        g.p(tool_x - 1, tool_y - 1, "water")
    elif tool == "staff":
        for i in range(6):
            g.p(tool_x, tool_y - i + 2, "wood")
        g.p(tool_x, tool_y - 4, "gem")
        g.p(tool_x + 1, tool_y - 4, "gem")

    def details(gr: Grid):
        ex, ey = eye
        if "eye" in ramps:
            gr.paint(ex, ey, ramps["eye"][-1])

    rows = finish(g, ramps, details)
    return rows, {"eye": (eye[0], eye[1], 1, 1)}


def draw_front(w, h, ramps, dress=False, horns=False):
    g = Grid(w, h)
    cx = w // 2
    head_top = 2
    head_bot = max(7, h // 3 + 1)
    torso_bot = head_bot + max(4, h // 3)
    for y in range(head_top, head_bot):
        half = 3 if head_top < y < head_bot - 1 else 2
        for x in range(cx - half, cx + half + 1):
            g.p(x, y, "hair")
    for y in range(head_top + 2, head_bot - 1):
        for x in range(cx - 2, cx + 3):
            g.p(x, y, "skin")
    eyes = [(cx - 1, head_top + 3), (cx + 1, head_top + 3)]
    if horns:
        g.p(cx - 3, head_top - 1, "horn")
        g.p(cx - 3, head_top, "horn")
        g.p(cx + 3, head_top - 1, "horn")
        g.p(cx + 3, head_top, "horn")
    for y in range(head_bot, torso_bot):
        span = 4 if not dress else 5
        for x in range(cx - span, cx + span + 1):
            g.p(x, y, "cloth")
        if not dress:
            g.p(cx - span - 1, y, "arm")
            g.p(cx + span + 1, y, "arm")
    if dress:
        for y in range(torso_bot, h - 3):
            for x in range(cx - 4, cx + 5):
                g.p(x, y, "cloth")
    else:
        for y in range(torso_bot, h - 3):
            for dx in (-2, -1, 1, 2):
                g.p(cx + dx, y, "leg")
        for dx in (-2, -1, 1, 2):
            g.p(cx + dx, h - 3, "boot")

    def details(gr: Grid):
        for ex, ey in eyes:
            gr.paint(ex, ey, ramps["eye"][-1])
            gr.paint(ex, ey + 1, ramps["skin"][len(ramps["skin"]) // 2])

    rows = finish(g, ramps, details)
    return rows, {"eye": (eyes[0][0], eyes[0][1], 3, 1)}


def draw_slime(w, h, ramps, spikes=False):
    g = Grid(w, h)
    g.disc(w / 2 - 0.5, h * 0.58, w * 0.38, h * 0.28, "body")
    # flatten the top a little and add a foot shadow
    for x in range(2, w - 2):
        g.p(x, h - 4, "body")
        g.p(x, h - 5, "body")
    if spikes:
        g.p(w // 2, 2, "body")
        g.p(w // 2 - 1, 3, "body")
        g.p(w // 2 + 1, 3, "body")
        g.p(4, 4, "body")
        g.p(w - 5, 4, "body")
    ey = int(h * 0.55)

    def details(gr: Grid):
        for dx in (-2, 2):
            gr.paint(w // 2 + dx, ey, ramps["eye"][0])
            gr.paint(w // 2 + dx, ey + 1, ramps["eye"][-1])
        gr.paint(w // 2 - 3, ey - 2, ramps["body"][-1])

    return finish(g, ramps, details), {"eye": (w // 2 - 2, ey, 5, 2)}


def draw_blob_enemy(kind, w, h, ramps):
    g = Grid(w, h)
    cx = w / 2 - 0.5
    if kind == "bat":
        g.disc(cx, h * 0.55, w * 0.42, h * 0.22, "wing")
        g.disc(cx, h * 0.5, w * 0.16, h * 0.22, "body")
        g.p(int(cx) - 2, 3, "body")
        g.p(int(cx) + 2, 3, "body")
        g.p(int(cx) - 2, 4, "body")
        g.p(int(cx) + 2, 4, "body")
    elif kind == "bee":
        g.disc(cx, h * 0.55, w * 0.28, h * 0.22, "body")
        g.disc(cx - w * 0.28, h * 0.42, w * 0.16, h * 0.1, "wing")
        g.disc(cx + w * 0.28, h * 0.42, w * 0.16, h * 0.1, "wing")
        g.rect(int(cx) + 3, int(h * 0.5), 3, 1, "sting")
    elif kind == "ghost":
        g.disc(cx, h * 0.42, w * 0.28, h * 0.28, "body")
        for x in range(int(cx) - 4, int(cx) + 5):
            g.p(x, int(h * 0.7), "body")
            if (x // 2) % 2 == 0:
                g.p(x, int(h * 0.7) + 1, "body")
                g.p(x, int(h * 0.7) + 2, "body")
    elif kind == "eye":
        g.disc(cx, h * 0.48, w * 0.32, h * 0.32, "body")
        g.disc(cx, h * 0.5, w * 0.12, h * 0.12, "iris")
    elif kind == "wisp":
        g.disc(cx, h * 0.5, w * 0.22, h * 0.28, "body")
        g.disc(cx, h * 0.62, w * 0.1, h * 0.16, "core")
    elif kind == "shroom":
        g.disc(cx, h * 0.4, w * 0.34, h * 0.22, "cap")
        g.rect(int(cx) - 1, int(h * 0.45), 3, int(h * 0.35), "stem")
    elif kind == "crab":
        g.disc(cx, h * 0.58, w * 0.28, h * 0.16, "body")
        g.rect(2, int(h * 0.48), 3, 3, "claw")
        g.rect(w - 5, int(h * 0.48), 3, 3, "claw")
        for dx in (-3, -1, 1, 3):
            g.p(int(cx) + dx, int(h * 0.75), "body")
            g.p(int(cx) + dx, int(h * 0.75) + 1, "body")
    elif kind == "worm":
        for i, y in enumerate(range(3, h - 3)):
            x = int(cx + (1 if (i // 2) % 2 == 0 else -1))
            g.p(x, y, "body")
            g.p(x + 1, y, "body")
            g.p(x - 1, y, "body")
    elif kind == "spike":
        g.disc(cx, h * 0.55, w * 0.22, h * 0.22, "body")
        for ang in range(8):
            dx = int(round((w * 0.32) * (1 if ang % 2 == 0 else 0.7) * __import__("math").cos(ang)))
            dy = int(round((h * 0.32) * (1 if ang % 2 == 0 else 0.7) * __import__("math").sin(ang)))
            g.p(int(cx) + dx, int(h * 0.55) + dy, "spike")
            g.p(int(cx) + dx // 2, int(h * 0.55) + dy // 2, "spike")
    elif kind == "skeleton":
        g.disc(cx, h * 0.28, w * 0.16, h * 0.14, "bone")
        g.rect(int(cx) - 1, int(h * 0.4), 3, int(h * 0.28), "bone")
        for y in range(int(h * 0.42), int(h * 0.62), 2):
            g.p(int(cx) - 2, y, "bone")
            g.p(int(cx) + 2, y, "bone")
        g.rect(int(cx) - 2, int(h * 0.68), 2, int(h * 0.2), "bone")
        g.rect(int(cx) + 1, int(h * 0.68), 2, int(h * 0.2), "bone")
    elif kind == "zombie":
        return draw_front(w, h, ramps)
    else:
        g.disc(cx, h * 0.5, w * 0.3, h * 0.3, "body")

    def details(gr: Grid):
        if kind == "eye" and "pupil" in ramps:
            gr.paint(w // 2, h // 2, ramps["pupil"][-1])
        elif kind in ("bat", "bee", "ghost", "crab", "shroom") and "eye" in ramps:
            gr.paint(w // 2 - 2, int(h * 0.5), ramps["eye"][-1])
            gr.paint(w // 2 + 1, int(h * 0.5), ramps["eye"][-1])
        elif kind == "skeleton" and "eye" in ramps:
            gr.paint(w // 2 - 1, int(h * 0.28), ramps["eye"][-1])
            gr.paint(w // 2 + 1, int(h * 0.28), ramps["eye"][-1])

    return finish(g, ramps, details), {}


def draw_chicken(w, h, ramps):
    g = Grid(w, h)
    g.disc(w * 0.48, h * 0.55, w * 0.32, h * 0.26, "body")
    g.disc(w * 0.72, h * 0.42, w * 0.12, h * 0.12, "body")
    g.p(int(w * 0.82), int(h * 0.42), "beak")
    g.p(int(w * 0.84), int(h * 0.42), "beak")
    g.p(int(w * 0.7), int(h * 0.32), "comb")
    g.p(int(w * 0.72), int(h * 0.3), "comb")
    g.p(int(w * 0.4), int(h * 0.78), "leg")
    g.p(int(w * 0.4), int(h * 0.82), "leg")
    g.p(int(w * 0.55), int(h * 0.78), "leg")
    g.p(int(w * 0.55), int(h * 0.82), "leg")
    # connect legs to body
    g.p(int(w * 0.4), int(h * 0.74), "leg")
    g.p(int(w * 0.55), int(h * 0.74), "leg")

    def details(gr: Grid):
        gr.paint(int(w * 0.74), int(h * 0.4), ramps["eye"][-1])

    return finish(g, ramps, details), {}


def draw_tile(kind, w, h, ramps):
    """Full-bleed tile. Edges are sealed so opposite pixels share a ramp step."""
    g = [[0] * w for _ in range(h)]
    body = ramps["body"]
    mid = body[len(body) // 2]
    dark = body[0]
    light = body[-1]
    accent = ramps.get("accent", body)
    for y in range(h):
        for x in range(w):
            n = (x * 3 + y * 5 + (x * y) % 3) % len(body)
            # clusters rather than fine noise
            if (x // 2 + y // 2) % 5 == 0:
                g[y][x] = dark
            elif (x // 3 + y // 2) % 4 == 0:
                g[y][x] = light if (x + y) % 2 == 0 else body[min(len(body) - 1, n)]
            else:
                g[y][x] = mid if (x + y) % 2 == 0 else body[max(0, len(body) // 2 - 1)]
    if kind == "grass":
        grass = ramps["accent"]
        for y in range(min(3, h)):
            for x in range(w):
                g[y][x] = grass[min(len(grass) - 1, 1 + (x + y) % (len(grass) - 1))]
            # blades
            if y == 0 and x % 3 == 0:
                pass
        for x in range(0, w, 3):
            g[0][x] = grass[-1]
    elif kind == "ore":
        ore = ramps["accent"]
        for y in range(2, h - 2):
            for x in range(2, w - 2):
                if (x - w // 2) ** 2 + (y - h // 2) ** 2 <= 4:
                    g[y][x] = ore[(x + y) % len(ore)]
    elif kind == "wood":
        for y in range(h):
            for x in range(w):
                g[y][x] = body[1] if x % 4 == 0 else body[min(len(body) - 1, 2 + (y % 2))]
    elif kind == "brick":
        for y in range(h):
            for x in range(w):
                row = y // 3
                off = 2 if row % 2 else 0
                g[y][x] = dark if (y % 3 == 0 or (x + off) % 4 == 0) else body[min(len(body) - 1, 2)]
    elif kind == "water":
        for y in range(h):
            for x in range(w):
                wave = 1 if ((x + y) // 2) % 3 == 0 else 0
                g[y][x] = body[min(len(body) - 1, 1 + wave + (y % 2))]
    elif kind == "lava":
        for y in range(h):
            for x in range(w):
                hot = (x // 2 + y // 3) % 3
                g[y][x] = body[min(len(body) - 1, hot + 1)]
    elif kind == "snow":
        for y in range(h):
            for x in range(w):
                g[y][x] = light if (x + y) % 5 else body[-2]
        if "accent" in ramps:
            for x in range(w):
                g[0][x] = ramps["accent"][min(1, len(ramps["accent"]) - 1)]
    elif kind == "path":
        for y in range(h):
            for x in range(w):
                g[y][x] = dark if (x + y) % 4 == 0 else body[len(body) // 2]
    elif kind == "hoedirt":
        for y in range(h):
            for x in range(w):
                g[y][x] = dark if y % 3 == 0 else body[1 + (x % 2)]
    elif kind == "cobble":
        for y in range(h):
            for x in range(w):
                block = (x // 3 + y // 3) % 2
                g[y][x] = body[1 + block] if (x % 3 and y % 3) else dark
    elif kind == "ice":
        for y in range(h):
            for x in range(w):
                g[y][x] = light if (x == y or x + y == w - 1) else body[len(body) // 2]
    # Seal left/right to the same index per row, and top/bottom unless layered.
    layered = kind in ("grass", "snow")
    for y in range(h):
        g[y][w - 1] = g[y][0]
    if not layered:
        for x in range(w):
            g[h - 1][x] = g[0][x]
        g[h - 1][w - 1] = g[0][0]
        g[h - 1][0] = g[0][0]
    else:
        # Entire top row one material, entire bottom the host ramp.
        for x in range(w):
            g[0][x] = ramps["accent"][1]
            g[h - 1][x] = ramps["body"][1]
        for y in range(h):
            g[y][w - 1] = g[y][0]
    return ["".join(enc(v) for v in row) for row in g]


def draw_prop(kind, w, h, ramps):
    g = Grid(w, h)
    if kind in ("crate", "chest", "barrel"):
        # front face
        g.rect(3, h // 3, w - 6, h // 2, "wood")
        # lid / top
        g.rect(3, h // 3 - 3, w - 6, 3, "lid")
        if kind == "chest":
            g.rect(w // 2 - 1, h // 3 - 2, 3, 3, "metal")
        if kind == "barrel":
            g.rect(3, h // 3 + 1, w - 6, 1, "metal")
            g.rect(3, h // 3 + h // 4, w - 6, 1, "metal")
        if kind == "crate":
            g.rect(3, h // 2, w - 6, 1, "band")
            g.rect(w // 2, h // 3, 1, h // 2, "band")
    elif kind == "sign":
        g.rect(w // 2 - 1, h // 2, 2, h // 2 - 2, "wood")
        g.rect(3, 3, w - 6, h // 3, "board")
    elif kind == "torch":
        g.rect(w // 2 - 1, h // 2, 2, h // 2 - 2, "wood")
        g.disc(w / 2, h * 0.32, 2.2, 3.2, "flame")
    elif kind == "lantern":
        g.rect(w // 2 - 1, 2, 2, 2, "metal")
        g.rect(w // 2 - 3, 4, 6, h // 2, "glass")
        g.rect(w // 2 - 3, 4 + h // 2, 6, 2, "metal")
    elif kind == "pot":
        g.disc(w / 2, h * 0.62, w * 0.28, h * 0.22, "clay")
        g.rect(w // 2 - 3, int(h * 0.4), 6, 2, "clay")
        g.p(w // 2, int(h * 0.32), "leaf")
        g.p(w // 2 - 1, int(h * 0.28), "leaf")
        g.p(w // 2 + 1, int(h * 0.3), "leaf")
    elif kind == "fence":
        for x in (3, w // 2 - 1, w - 5):
            g.rect(x, 3, 2, h - 6, "wood")
        g.rect(3, h // 3, w - 6, 2, "wood")
        g.rect(3, h // 2, w - 6, 2, "wood")
    elif kind == "door":
        g.rect(4, 2, w - 8, h - 4, "wood")
        g.rect(w - 7, h // 2, 2, 2, "metal")
    elif kind == "bed":
        g.rect(2, h // 2, w - 4, h // 3, "wood")
        g.rect(3, h // 2 - 3, w // 3, 4, "cloth")
        g.rect(w // 3, h // 2 - 1, w // 2, 3, "cloth")
    elif kind == "table":
        g.rect(2, h // 3, w - 4, 3, "wood")
        g.rect(3, h // 3 + 3, 2, h // 3, "wood")
        g.rect(w - 5, h // 3 + 3, 2, h // 3, "wood")
    elif kind == "chair":
        g.rect(4, h // 3, w - 8, 3, "wood")
        g.rect(4, 3, 2, h // 3, "wood")
        g.rect(4, h // 3 + 3, 2, h // 3, "wood")
        g.rect(w - 6, h // 3 + 3, 2, h // 3, "wood")
    elif kind == "well":
        g.disc(w / 2, h * 0.55, w * 0.32, h * 0.18, "stone")
        g.disc(w / 2, h * 0.55, w * 0.16, h * 0.08, "water")
        g.rect(w // 2 - 4, 3, 2, h // 3, "wood")
        g.rect(w // 2 + 2, 3, 2, h // 3, "wood")
        g.rect(w // 2 - 4, 3, 8, 2, "wood")
    elif kind == "iso-crate":
        # three faces, separate materials, top-left light already in the ramps
        top_y = 3
        for i, y in enumerate(range(top_y, top_y + 4)):
            span = i + 2
            for x in range(w // 2 - span, w // 2 + span):
                g.p(x, y, "top")
        for y in range(top_y + 4, h - 3):
            for x in range(3, w // 2):
                g.p(x, y, "left")
            for x in range(w // 2, w - 3):
                g.p(x, y, "right")
        g.rect(w // 2 - 1, h // 2, 2, 2, "metal")
    else:
        g.rect(3, 3, w - 6, h - 6, "wood")
    return finish(g, ramps), {}


def draw_item(kind, w, h, ramps):
    g = Grid(w, h)
    cx, cy = w // 2, h // 2
    if kind == "sword":
        for i in range(h - 8):
            g.p(cx + (0 if i % 2 == 0 else -1), 2 + i, "metal")
            g.p(cx - 1, 2 + i, "metal")
        g.rect(cx - 3, h - 8, 6, 2, "metal")
        g.rect(cx - 1, h - 6, 2, 3, "wood")
    elif kind == "pick":
        g.rect(cx - 1, 6, 2, h - 10, "wood")
        g.rect(cx - 4, 3, 8, 3, "metal")
    elif kind == "axe":
        g.rect(cx, 4, 2, h - 8, "wood")
        g.rect(cx - 4, 3, 5, 4, "metal")
    elif kind == "can":
        g.rect(4, cy, 6, h // 3, "metal")
        g.rect(8, cy - 3, 4, 2, "metal")
        g.p(3, cy + 1, "water")
        g.p(2, cy + 2, "water")
    elif kind == "gem":
        g.disc(cx, cy, w * 0.28, h * 0.32, "gem")
    elif kind == "coin":
        g.disc(cx, cy, w * 0.3, h * 0.3, "metal")
    elif kind == "key":
        g.disc(cx - 2, cy - 2, 2.4, 2.4, "metal")
        g.rect(cx, cy, 5, 2, "metal")
        g.p(cx + 4, cy + 2, "metal")
        g.p(cx + 3, cy + 2, "metal")
    elif kind == "potion":
        g.rect(cx - 2, cy - 1, 5, h // 3, "glass")
        g.rect(cx - 1, cy - 4, 3, 3, "glass")
        g.rect(cx - 1, cy + 1, 3, 3, "liquid")
    elif kind == "berry":
        g.disc(cx, cy + 1, 3.2, 3.2, "fruit")
        g.p(cx, cy - 3, "leaf")
        g.p(cx + 1, cy - 4, "leaf")
    elif kind == "fish":
        g.disc(cx, cy, w * 0.28, h * 0.16, "body")
        g.p(cx - 5, cy - 2, "body")
        g.p(cx - 5, cy + 2, "body")
        g.p(cx - 6, cy, "body")
    elif kind == "book":
        g.rect(4, 3, w - 8, h - 6, "cover")
        g.rect(cx - 1, 4, 2, h - 8, "page")
    elif kind == "bow":
        for y in range(4, h - 4):
            x = cx - 2 + (1 if y < cy else 0)
            g.p(x, y, "wood")
        g.p(cx + 2, cy, "string")
        for y in range(5, h - 5):
            g.p(cx + 2, y, "string")
    elif kind == "shield":
        g.disc(cx, cy, w * 0.3, h * 0.34, "wood")
        g.rect(cx - 1, cy - 3, 2, 6, "metal")
    else:
        g.rect(4, 4, w - 8, h - 8, "wood")
    return finish(g, ramps), {}


def draw_plant(kind, w, h, ramps, stage=3):
    g = Grid(w, h)
    # soil band keeps the early stage's bbox honest
    soil_top = h - 6
    for y in range(soil_top, h - 1):
        for x in range(1, w - 1):
            g.p(x, y, "soil")
    cx = w // 2
    stem_top = soil_top - (2 + stage * (h // 6))
    stem_top = max(2, stem_top)
    for y in range(stem_top, soil_top):
        g.p(cx, y, "stem")
        g.p(cx - 1, y, "stem")
    if stage >= 2:
        leaf_y = stem_top + 2
        span = 2 + stage
        for x in range(cx - span, cx + span):
            g.p(x, leaf_y, "leaf")
            if abs(x - cx) < span - 1:
                g.p(x, leaf_y + 1, "leaf")
        if kind in ("wheat", "sunflower", "tulip"):
            for x in range(cx - 2, cx + 3):
                g.p(x, stem_top, "flower")
                g.p(x, stem_top + 1, "flower")
        if kind == "cauli":
            g.disc(cx, stem_top + 2, 3.5, 3.0, "flower")
        if kind == "pumpkin":
            g.disc(cx, soil_top - 3, 4.5, 3.5, "fruit")
        if kind == "tulip":
            g.p(cx, stem_top - 1, "flower")
    if stage >= 3 and kind in ("parsnip", "turnip"):
        g.disc(cx, soil_top - 1, 2.5, 2.0, "fruit")
    if kind == "cactus":
        for y in range(4, soil_top):
            g.p(cx, y, "leaf")
            g.p(cx - 1, y, "leaf")
            g.p(cx + 1, y, "leaf")
        g.rect(cx - 4, h // 2, 3, 2, "leaf")
        g.rect(cx + 2, h // 2 - 2, 3, 2, "leaf")
    if kind == "sapling":
        g.disc(cx, stem_top + 2, 3, 3, "leaf")
    if kind == "mushroom":
        g.disc(cx, soil_top - 4, 3.2, 2.2, "cap")
        for y in range(soil_top - 4, soil_top):
            g.p(cx, y, "stem")
    if kind == "bush":
        g.disc(cx, soil_top - 4, w * 0.28, h * 0.16, "leaf")
        g.p(cx - 2, soil_top - 5, "fruit")
        g.p(cx + 2, soil_top - 3, "fruit")
    return finish(g, ramps), {}


def draw_tree(w, h, ramps):
    g = Grid(w, h)
    cx = w // 2
    g.disc(cx, h * 0.32, w * 0.28, h * 0.2, "leaf")
    g.disc(cx - w * 0.16, h * 0.4, w * 0.16, h * 0.12, "leaf")
    g.disc(cx + w * 0.16, h * 0.4, w * 0.16, h * 0.12, "leaf")
    trunk_top = int(h * 0.48)
    for y in range(trunk_top, h - 2):
        g.p(cx - 1, y, "wood")
        g.p(cx, y, "wood")
        g.p(cx + 1, y, "wood")
    # fruit tucked into the canopy so they are not isolated
    g.p(cx - 3, int(h * 0.34), "fruit")
    g.p(cx + 4, int(h * 0.3), "fruit")
    g.p(cx + 1, int(h * 0.42), "fruit")
    return finish(g, ramps), {}


def draw_ui(kind, w, h, ramps):
    g = Grid(w, h)
    if kind == "dialog":
        g.rect(1, 1, w - 2, h - 2, "paper")
        # glyph blocks spelling a short word, joined to the panel by a baseline
        # so they are not extra components. The cursor is a unique material.
        word = [(3, 6, 2, 5), (6, 6, 2, 5), (9, 8, 2, 3)]
        for x, y, ww, hh in word:
            g.rect(x, y, ww, hh, "ink")
        g.rect(3, 11, 10, 1, "ink")
        g.rect(w - 6, h - 6, 3, 3, "cursor")
    elif kind == "hearts":
        # three hearts
        for i, ox in enumerate((3, w // 2 - 3, w - 9)):
            mat = "full" if i < 2 else "empty"
            g.p(ox + 1, 3, mat)
            g.p(ox + 2, 3, mat)
            g.p(ox + 4, 3, mat)
            g.p(ox + 5, 3, mat)
            for x in range(ox, ox + 7):
                g.p(x, 4, mat)
                g.p(x, 5, mat)
            for x in range(ox + 1, ox + 6):
                g.p(x, 6, mat)
            for x in range(ox + 2, ox + 5):
                g.p(x, 7, mat)
            g.p(ox + 3, 8, mat)
        # join hearts with a thin meter so the component count stays low
        g.rect(3, h - 5, w - 6, 2, "full")
    elif kind == "button":
        g.rect(2, 2, w - 4, h - 4, "face")
        g.rect(4, h // 2 - 1, w - 8, 2, "ink")
    elif kind == "slot":
        g.rect(2, 2, w - 4, h - 4, "face")
        g.rect(5, 5, w - 10, h - 10, "inset")
    elif kind == "cursor":
        for i in range(h - 4):
            for k in range(i // 2 + 1):
                g.p(3 + k, 2 + i, "hand")
        g.p(3, h - 4, "hand")
    elif kind == "counter":
        g.disc(6, h // 2, 3, 3, "coin")
        g.rect(11, h // 2 - 3, 2, 6, "ink")
        g.rect(15, h // 2 - 3, 2, 6, "ink")
        g.rect(14, h // 2 - 1, 2, 2, "ink")
        g.rect(11, h // 2 - 1, 6, 1, "ink")
    elif kind == "nameplate":
        g.rect(1, 2, w - 2, h - 4, "face")
        g.rect(4, h // 2 - 1, w // 2, 2, "ink")
    elif kind == "arrow":
        g.rect(3, h // 2 - 1, w - 8, 3, "ink")
        for i in range(4):
            g.p(w - 6 - i, h // 2 - 2 + i, "ink")
            g.p(w - 6 - i, h // 2 + 2 - i, "ink")
    elif kind == "stamina":
        g.rect(2, 3, w - 4, h - 6, "empty")
        g.rect(3, 4, (w - 6) * 2 // 3, h - 8, "full")
    elif kind == "marker":
        g.disc(w // 2, h // 3, 4, 4, "full")
        g.p(w // 2, h // 3 + 5, "full")
        g.p(w // 2, h // 3 + 6, "full")
        g.p(w // 2 - 1, h // 3 + 7, "full")
        g.p(w // 2 + 1, h // 3 + 7, "full")
    else:
        g.rect(2, 2, w - 4, h - 4, "face")
    return finish(g, ramps), {}


def draw_fx(kind, w, h, ramps):
    g = Grid(w, h)
    cx, cy = w // 2, h // 2
    if kind == "heart":
        for dx in (-3, -2, 2, 3):
            g.p(cx + dx, cy - 3, "heart")
        for x in range(cx - 4, cx + 5):
            g.p(x, cy - 2, "heart")
            g.p(x, cy - 1, "heart")
        for x in range(cx - 3, cx + 4):
            g.p(x, cy, "heart")
        for x in range(cx - 2, cx + 3):
            g.p(x, cy + 1, "heart")
        g.p(cx, cy + 2, "heart")
        g.p(cx - 1, cy + 2, "heart")
        g.p(cx + 1, cy + 2, "heart")
    elif kind == "spike":
        # single peak, base along the bottom, empty margin for bbox and rotation
        g.p(cx, 1, "metal")
        g.p(cx - 1, 2, "metal")
        g.p(cx, 2, "metal")
        g.p(cx + 1, 2, "metal")
        for x in range(cx - 2, cx + 3):
            g.p(x, 3, "metal")
        for x in range(cx - 3, cx + 4):
            g.p(x, 4, "metal")
            g.p(x, 5, "metal")
    elif kind == "spark":
        g.p(cx, cy, "hot")
        for dx, dy in ((0, -2), (0, -1), (0, 1), (0, 2), (-2, 0), (-1, 0), (1, 0), (2, 0), (-1, -1), (1, -1)):
            g.p(cx + dx, cy + dy, "hot")
    elif kind == "boom":
        g.disc(cx, cy, w * 0.28, h * 0.28, "hot")
        g.disc(cx, cy, w * 0.12, h * 0.12, "core")
    elif kind == "bone":
        g.rect(cx - 4, cy - 1, 8, 2, "bone")
        g.disc(cx - 4, cy, 1.6, 2.2, "bone")
        g.disc(cx + 4, cy, 1.6, 2.2, "bone")
    elif kind == "ring":
        g.disc(cx, cy, w * 0.28, h * 0.28, "ring")
        # hollow by clearing center after shade — handled in details via mat erase
        # keep a thick ring by not filling a smaller disc of a different mat
    elif kind == "slash":
        for i in range(h - 6):
            g.p(3 + i, 2 + i, "slash")
            g.p(4 + i, 2 + i, "slash")
            g.p(3 + i, 3 + i, "slash")
    elif kind == "crystal":
        g.p(cx, 2, "gem")
        for x in range(cx - 1, cx + 2):
            g.p(x, 3, "gem")
        for x in range(cx - 2, cx + 3):
            g.p(x, 4, "gem")
            g.p(x, 5, "gem")
        for x in range(cx - 1, cx + 2):
            g.p(x, 6, "gem")
        g.p(cx, 7, "gem")
    elif kind == "bullet":
        g.p(3, cy, "shot")
        g.p(4, cy, "shot")
        g.p(4, cy - 1, "shot")
        g.p(4, cy + 1, "shot")
        g.p(5, cy, "shot")
    else:
        g.disc(cx, cy, 3, 3, "hot")

    def details(gr: Grid):
        if kind == "ring":
            # punch a hole but leave the ring 4-connected and not isolated
            for y in range(cy - 1, cy + 2):
                for x in range(cx - 1, cx + 2):
                    gr.a[y][x] = 0
                    gr.mat[y][x] = None
        if kind == "heart" and "glint" in ramps:
            gr.paint(cx - 2, cy - 2, ramps["glint"][-1])

    rows = finish(g, ramps, details)
    return rows, {}


def hand_run() -> list[list[str]]:
    """Four 8x8 run poses. Indices match the celeste palette built below."""
    frames = [
        [
            ".2344...",
            "2234567.",
            ".345678.",
            ".88999a.",
            ".889abc.",
            "..b.c...",
            "..c.....",
            "........",
        ],
        [
            "2344....",
            ".234567.",
            ".345678.",
            ".88999a.",
            ".88abc..",
            ".b...c..",
            "........",
            "........",
        ],
        [
            ".2344...",
            "2234567.",
            ".345678.",
            ".88999a.",
            ".889abc.",
            "...b.c..",
            "....c...",
            "........",
        ],
        [
            "..2344..",
            ".223456.",
            "..34567.",
            ".88999a.",
            ".8b9ac9.",
            ".b.c....",
            "..c.....",
            "........",
        ],
    ]
    return frames


# Palettes: shadows cooler / highlights warmer (hue shift is in the hexes).
PALETTES = {
    "celeste": [
        ("hair", ["#3a1848", "#a02848", "#ff6848", "#ffd0a8"]),
        ("skin", ["#c08060", "#f0b090", "#ffe0c8"]),
        ("cloth", ["#142848", "#2a58b0", "#78b0e8"]),
        ("leg", ["#2a241c", "#6a5040", "#d09858"]),
        ("eye", ["#1a1a22", "#f0f4f8"]),
    ],
    "stardew": [
        ("hair", ["#3a2418", "#8a4030", "#e09048", "#f8d8a8"]),
        ("skin", ["#a86848", "#e8a888", "#ffd8c0"]),
        ("cloth", ["#1e4a28", "#3e8a40", "#a8d868"]),
        ("leg", ["#2a2420", "#5a4638", "#a08060"]),
        ("boot", ["#3a2418", "#6a4030"]),
        ("arm", ["#a86848", "#e8a888"]),
        ("pack", ["#5a3018", "#a06030"]),
        ("eye", ["#1a1a22", "#f4f0e8"]),
        ("wood", ["#4a3018", "#8a5a30"]),
        ("metal", ["#3a4860", "#c8d4e0"]),
        ("water", ["#2a68a8", "#78c8f0"]),
        ("gem", ["#3050a0", "#80c0ff"]),
    ],
    "terra": [
        ("body", ["#3a2a18", "#6a4a28", "#a87840", "#e0b868"]),
        ("accent", ["#2a5a28", "#4a9a38", "#a8d860"]),
        ("metal", ["#3a5060", "#d0dce8"]),
        ("wood", ["#4a3018", "#a06830"]),
        ("eye", ["#1a1a22", "#f4f0e8"]),
        ("hair", ["#2a2018", "#6a4030", "#e0a060"]),
        ("skin", ["#c09070", "#f0c8a8"]),
        ("cloth", ["#4a3828", "#a08058", "#e8d0a0"]),
        ("leg", ["#2a2420", "#6a5644"]),
        ("boot", ["#2a2018", "#5a4030"]),
        ("arm", ["#c09070", "#f0c8a8"]),
    ],
    "undertale": [
        ("hair", ["#1a1a22", "#3a3a48", "#d8d8e0"]),
        ("skin", ["#e8c8a8", "#f8e0c8", "#fff0e0"]),
        ("cloth", ["#2a3a78", "#4868c8", "#a8c0f0"]),
        ("leg", ["#2a2a38", "#5a5a70"]),
        ("boot", ["#2a2a38", "#6a6a78"]),
        ("arm", ["#e8c8a8", "#f8e0c8"]),
        ("eye", ["#1a1a22", "#f8f8f0"]),
        ("heart", ["#6a1028", "#e02040", "#ff6878", "#ffd0d0"]),
        ("glint", ["#ffd0d0", "#fff4f4"]),
        ("shot", ["#e8e8f0", "#ffffff"]),
        ("ink", ["#1a1a22", "#2a2a38"]),
        ("paper", ["#f4f0e4", "#fff8ec"]),
        ("cursor", ["#e02040", "#ff6878"]),
    ],
    "deadcells": [
        ("hair", ["#1a2430", "#3a5060"]),
        ("skin", ["#c8a090", "#f0d0c0"]),
        ("cloth", ["#1a2838", "#2a4860", "#68a0b0"]),
        ("leg", ["#1a2028", "#3a4858"]),
        ("boot", ["#2a2018", "#5a4030"]),
        ("arm", ["#c8a090", "#f0d0c0"]),
        ("eye", ["#f0e8c0", "#fff8e0"]),
        ("bone", ["#6a6458", "#d8d0c0", "#f4f0e4"]),
        ("horn", ["#3a2418", "#a06040"]),
        ("body", ["#142028", "#2a4858", "#68a0a8"]),
        ("wing", ["#1a2830", "#3a5868"]),
        ("hot", ["#6a2018", "#e05028", "#ffb060"]),
        ("slash", ["#e8eef4", "#ffffff"]),
    ],
    "octopath": [
        ("hair", ["#2a1a38", "#684080", "#f0c8a0"]),
        ("skin", ["#c89878", "#f0c8a8"]),
        ("cloth", ["#3a2048", "#784080", "#e0a868"]),
        ("leg", ["#2a2030", "#5a4060"]),
        ("boot", ["#3a2818", "#8a5830"]),
        ("arm", ["#c89878", "#f0c8a8"]),
        ("eye", ["#1a1a22", "#f8f0e0"]),
        ("gem", ["#c06020", "#ffd080"]),
        ("wood", ["#4a3018", "#c08848"]),
    ],
    "slime": [
        ("body", ["#243038", "#4a6860", "#78a090", "#c8e0c0", "#f0f4e8"]),
        ("eye", ["#1a1a22", "#f4f8f0"]),
    ],
    "plant": [
        ("soil", ["#3a2818", "#6a4428", "#a07040"]),
        ("stem", ["#1e4a20", "#3e8a30"]),
        ("leaf", ["#1a5828", "#3a9838", "#78c850", "#d0f090"]),
        ("flower", ["#a03048", "#f07848", "#ffd080"]),
        ("fruit", ["#803018", "#e05828", "#f0a040"]),
        ("cap", ["#6a2030", "#c04848", "#f0a0a0"]),
        ("wood", ["#3a2818", "#6a4428", "#a87848"]),
    ],
    "wood": [
        ("wood", ["#3a2414", "#6a4424", "#a86c38", "#e0a868"]),
        ("lid", ["#4a3018", "#c08848", "#f0c888"]),
        ("metal", ["#3a4450", "#b8c4d0", "#f0f4f8"]),
        ("band", ["#2a2018", "#5a4030"]),
        ("board", ["#5a3c20", "#c89858"]),
        ("top", ["#c09050", "#f0d090", "#fff0c8"]),
        ("left", ["#6a4428", "#a86c38", "#e0a868"]),
        ("right", ["#3a2818", "#6a4428", "#8a5c38"]),
        ("clay", ["#6a3828", "#c07048", "#f0b090"]),
        ("leaf", ["#2a6828", "#68b040"]),
        ("flame", ["#a02818", "#f07820", "#ffd060"]),
        ("glass", ["#d8c878", "#fff0a0"]),
        ("stone", ["#4a4a52", "#8a9098", "#d0d4d8"]),
        ("water", ["#2a68a8", "#78c0e8"]),
    ],
    "item": [
        ("metal", ["#3a4454", "#8a98a8", "#e8eef4"]),
        ("wood", ["#4a3018", "#a06830", "#e0b070"]),
        ("water", ["#2a68a8", "#90d0f0"]),
        ("gem", ["#801828", "#e04050", "#ffb0b8"]),
        ("fruit", ["#801830", "#e04058", "#ffb0c0"]),
        ("leaf", ["#1e5820", "#58a838"]),
        ("body", ["#2a68a8", "#68b0e0", "#d0ecf8"]),
        ("cover", ["#3a2868", "#6850b0", "#c0b0e8"]),
        ("page", ["#e8dcc0", "#fff4e0"]),
        ("string", ["#e8e8f0", "#ffffff"]),
        ("glass", ["#c8d8e8", "#f4f8fc"]),
        ("liquid", ["#208060", "#40d0a0"]),
    ],
    "ui": [
        ("paper", ["#d8d0b8", "#f4f0e0", "#fff8ec"]),
        ("ink", ["#1a1a22", "#2a2a38"]),
        ("cursor", ["#c8b890", "#fff8ec"]),
        ("face", ["#3a4460", "#6878a0", "#d0d8e8"]),
        ("inset", ["#1a2030", "#2a3448"]),
        ("full", ["#a02030", "#e04050", "#ffb0b0"]),
        ("empty", ["#3a2428", "#6a4048"]),
        ("coin", ["#a07820", "#f0d060"]),
        ("hand", ["#f0e8d0", "#fff8ec"]),
    ],
    "fx": [
        ("heart", ["#6a1028", "#e02040", "#ff6878", "#ffd0d0"]),
        ("glint", ["#ffd0d0", "#fff4f8"]),
        ("shot", ["#d8d8f0", "#ffffff"]),
        ("metal", ["#c8d0d8", "#f4f8fc"]),
        ("hot", ["#6a2010", "#e04820", "#ffc060"]),
        ("core", ["#fff0c0", "#ffffff"]),
        ("bone", ["#c8c0b0", "#f4f0e4"]),
        ("ring", ["#e8e8f0", "#ffffff"]),
        ("slash", ["#d0d8e8", "#ffffff"]),
        ("gem", ["#2040a0", "#60a0f0", "#d0e8ff"]),
        ("spike", ["#d8d8e0", "#ffffff"]),
    ],
    "enemy": [
        ("body", ["#2a2018", "#6a4030", "#c08058", "#f0d0b0"]),
        ("wing", ["#241820", "#5a3048", "#c07080"]),
        ("eye", ["#1a1a22", "#f4f0c8"]),
        ("claw", ["#6a2020", "#e05040"]),
        ("sting", ["#1a1a22", "#d0d0d0"]),
        ("cap", ["#6a2030", "#d05848", "#f0b0a0"]),
        ("stem", ["#e8d8c0", "#f8f0e0"]),
        ("iris", ["#f4f0e0", "#ffffff"]),
        ("pupil", ["#1a1a22", "#201818"]),
        ("core", ["#fff0c0", "#ffffff"]),
        ("spike", ["#4a4038", "#d0d0d0"]),
        ("bone", ["#8a8478", "#e8e0d4", "#f8f4ec"]),
        ("horn", ["#3a2818", "#a06840"]),
        ("hair", ["#2a2018", "#5a4030"]),
        ("skin", ["#6a9848", "#a8d070"]),
        ("cloth", ["#2a3820", "#4a6838", "#88a868"]),
        ("leg", ["#243018", "#4a5830"]),
        ("boot", ["#2a2018", "#4a3828"]),
        ("arm", ["#6a9848", "#a8d070"]),
    ],
}


def subset(theme: str, names: list[str]) -> list[tuple[str, list[str]]]:
    have = {n: h for n, h in PALETTES[theme]}
    out = []
    for n in names:
        if n in have:
            out.append((n, have[n]))
    return out


# Asset table. size, theme groups, drawer key.
def assets() -> list[dict]:
    rows = []

    def add(**kw):
        rows.append(kw)

    # characters 01-16
    add(id="celeste-run", game="Celeste", cat="characters", w=8, h=8, hand="run",
        groups=subset("celeste", ["hair", "skin", "cloth", "leg"]),
        anim="run", desc="Side-view runner, 8x8, four poses. Red-orange hair over a cool blue jacket, warm skin, top-left light, no dither (the frame budget is 22 pixels). Legs swap; the jacket stays put.")
    add(id="celeste-dash", game="Celeste", cat="characters", w=16, h=16,
        groups=subset("celeste", ["hair", "skin", "cloth", "leg", "eye"]),
        drawer="side", tool="none", pack=True,
        desc="Dash pose at 16x16 with a small pack. Hair is a warm ramp (shadow purple, highlight peach); jacket is a cool blue ramp. 1px selective rim, checker dither on the mid band. Bob.")
    add(id="celeste-climb", game="Celeste", cat="characters", w=16, h=24,
        groups=subset("celeste", ["hair", "skin", "cloth", "leg", "eye"]),
        drawer="side",
        desc="Climbing silhouette, 16x24, same Madeline-like palette. Hair mass on the back of the head, jacket reads as one shape, legs split. Top-left light, dither on the torso.")
    add(id="undertale-frisk", game="Undertale", cat="characters", w=16, h=24,
        groups=subset("undertale", ["hair", "skin", "cloth", "leg", "boot", "arm", "eye"]),
        drawer="front",
        desc="Front-facing child, blue shirt, striped by the shade ramp rather than a drawn stripe. 16x24, black-blue hair, warm skin, two eyes. Bob plus a blink.")
    add(id="undertale-frisk-side", game="Undertale", cat="characters", w=16, h=24,
        groups=subset("undertale", ["hair", "skin", "cloth", "leg", "boot", "arm", "eye"]),
        drawer="side",
        desc="Same child in side view, 16x24. One eye, blue torso, cool hair shadow. Selective rim breaks on the lit side.")
    add(id="stardew-farmer", game="Stardew Valley", cat="characters", w=16, h=32,
        groups=subset("stardew", ["hair", "skin", "cloth", "leg", "boot", "arm", "eye"]),
        drawer="front",
        desc="Front farmer, 16x32. Green shirt, warm skin, brown hair with a peach highlight. Fills the canvas the way a Stardew walk frame does. Bob.")
    add(id="stardew-water", game="Stardew Valley", cat="characters", w=16, h=32,
        groups=subset("stardew", ["hair", "skin", "cloth", "leg", "boot", "arm", "eye", "metal", "water"]),
        drawer="side", tool="water",
        desc="Side farmer with a watering can. Can is a cool metal ramp; a two-pixel water drop sits on the spout. 16x32.")
    add(id="terra-miner", game="Terraria", cat="characters", w=16, h=24,
        groups=subset("terra", ["hair", "skin", "cloth", "leg", "boot", "arm", "eye", "metal", "wood"]),
        drawer="side", tool="pick",
        desc="Miner in profile with a pick. Earth-tone clothes, cool metal head. 16x24, top-left light.")
    add(id="deadcells-idle", game="Dead Cells", cat="characters", w=24, h=32,
        groups=subset("deadcells", ["hair", "skin", "cloth", "leg", "boot", "arm", "eye", "horn"]),
        drawer="front", horns=True,
        desc="Horned prisoner, 24x32. Teal cloth against a warm bone-coloured head, no face detail beyond two eyes. The horns are part of the hair mass so they stay connected.")
    add(id="deadcells-roll", game="Dead Cells", cat="characters", w=24, h=24,
        groups=subset("deadcells", ["cloth", "skin", "eye", "horn", "bone"]),
        drawer="blob", blob="wisp",
        desc="Rolled-up silhouette, 24x24, a compact teal body rather than a full figure. Reads as a tucked roll, not a standing pose. Bob stands in for the spin (arbitrary angles are still refused).")
    add(id="octopath-hero", game="Octopath Traveler", cat="characters", w=16, h=24,
        groups=subset("octopath", ["hair", "skin", "cloth", "leg", "boot", "arm", "eye"]),
        drawer="front",
        desc="HD-2D is out of reach; this is the sprite half. 16x24 traveller, plum coat, gold-warm highlight on the hair, cool purple shadow. Top-left lamp light.")
    add(id="octopath-dancer", game="Octopath Traveler", cat="characters", w=16, h=24,
        groups=subset("octopath", ["hair", "skin", "cloth", "leg", "eye", "gem"]),
        drawer="front", dress=True,
        desc="Dancer in a long coat that replaces the legs. Gem-coloured sash is the cloth highlight. 16x24, same lamp-from-the-top-left shading.")
    add(id="stardew-chicken", game="Stardew Valley", cat="characters", w=16, h=16,
        groups=subset("plant", []) or [("body", ["#e8d8a0", "#f8f0c8", "#fff8e0"]), ("beak", ["#e07820", "#f0b040"]), ("comb", ["#c03030", "#f06060"]), ("leg", ["#e07820", "#f0b040"]), ("eye", ["#1a1a22", "#f8f8f0"])],
        drawer="chicken",
        desc="Hen, 16x16. Cream body with a warm highlight, orange beak and comb, two-pixel legs joined to the body. Bob.")
    add(id="terra-guide", game="Terraria", cat="characters", w=16, h=24,
        groups=subset("terra", ["hair", "skin", "cloth", "leg", "boot", "arm", "eye"]),
        drawer="front",
        desc="Town NPC, front view, 16x24. Tan clothes, brown hair, same earth palette as the miner so they share a town.")
    add(id="celeste-hair", game="Celeste", cat="characters", w=16, h=16,
        groups=subset("celeste", ["hair", "skin", "cloth", "leg", "eye"]),
        drawer="side",
        desc="Head-and-shoulders study of the runner's hair. The hair ramp shifts from purple shadow to peach highlight. 16x16, bob where a hair sim would be.")
    add(id="undertale-flowey", game="Undertale", cat="characters", w=16, h=16,
        groups=[("stem", ["#1e5a20", "#48b040"]), ("leaf", ["#2a8a30", "#a0e060"]), ("flower", ["#e8e0a0", "#fff4c0"]), ("face", ["#1a1a22", "#f4f0c0"]), ("eye", ["#1a1a22", "#f4f0c0"])],
        drawer="blob", blob="shroom",
        desc="Flower with a face, 16x16. Stem and cap use the shroom silhouette recolored to a pale petal and a green stem. Not a portrait.")

    # tiles 17-32
    add(id="terra-tiles", game="Terraria", cat="tiles", w=8, h=8, sheet="tiles",
        desc="Six 8x8 stamps in one sheet: dirt, dirt variant, grass-over-dirt, grass variant, ore, ore variant. Edges match within one ramp step. Grass is a real layer boundary (top grass, bottom dirt) so the vertical seam is exempt. Ore sits in the interior.")
    for name, kind, game, note in [
        ("terra-dirt", "dirt", "Terraria", "Dirt clods, dark corners, mid fill, sealed edges."),
        ("terra-grass", "grass", "Terraria", "Grass cap on a dirt host. Blades on the top row, dirt on the bottom."),
        ("terra-stone", "stone", "Terraria", "Stone with a darker mortar crack every few pixels."),
        ("terra-ore", "ore", "Terraria", "Stone host, copper-coloured vein kept off the edge."),
        ("terra-sand", "sand", "Terraria", "Warm sand, lighter specks, no grass."),
        ("terra-snow", "snow", "Terraria", "Snow cap over stone, same layer rule as grass."),
        ("terra-wood", "wood", "Terraria", "Vertical planks, darker seam every fourth column."),
        ("stardew-path", "path", "Stardew Valley", "Worn path, dark pebbles on a tan bed."),
        ("stardew-hoedirt", "hoedirt", "Stardew Valley", "Hoe rows: a dark line every third row."),
        ("celeste-snow", "snow", "Celeste", "Snow ground in the cool Celeste palette."),
        ("undertale-ruins", "brick", "Undertale", "Purple-grey brick courses."),
        ("deadcells-brick", "brick", "Dead Cells", "Dark brick, teal mortar in the shadow step."),
        ("octopath-cobble", "cobble", "Octopath Traveler", "Cobbles with dark joints, warm highlights."),
        ("terra-ice", "ice", "Terraria", "Pale ice with a diagonal crack."),
        ("lava-crust", "lava", "Terraria", "Dark crust with hot interior cells. Shimmer anim."),
    ]:
        theme = "terra"
        groups = [("body", ["#3a2a18", "#6a4a28", "#a87840", "#e0b868"]), ("accent", ["#2a5a28", "#4a9a38", "#a8d860"])]
        if kind == "stone" or name.endswith("stone"):
            groups = [("body", ["#3a3a42", "#6a6a74", "#a0a0a8", "#d8d8de"]), ("accent", ["#8a5a28", "#e0a050", "#f0d090"])]
        if kind == "sand" or "sand" in name:
            groups = [("body", ["#a07838", "#d0a058", "#f0d090", "#fff0c8"]), ("accent", ["#a07838", "#f0d090"])]
        if kind == "snow" or "snow" in name:
            groups = [("body", ["#5a6878", "#98a8b8", "#d0dce8"]), ("accent", ["#e8f0f4", "#ffffff", "#d0e8f0"])]
        if kind == "wood":
            groups = [("body", ["#3a2818", "#6a4428", "#a86c38", "#e0b070"]), ("accent", ["#6a4428", "#e0b070"])]
        if "path" in name:
            groups = [("body", ["#6a5840", "#a89068", "#e0c898", "#f4e4c8"]), ("accent", ["#6a5840", "#e0c898"])]
        if "hoedirt" in name:
            groups = [("body", ["#3a2818", "#6a4428", "#a07040", "#d0a070"]), ("accent", ["#3a2818", "#a07040"])]
        if "ruins" in name:
            groups = [("body", ["#2a2438", "#4a4068", "#8878a8", "#d0c8e0"]), ("accent", ["#4a4068", "#d0c8e0"])]
        if "deadcells" in name:
            groups = [("body", ["#1a242c", "#3a4854", "#607080", "#a8b8c0"]), ("accent", ["#2a6068", "#68a0a0"])]
        if "cobble" in name:
            groups = [("body", ["#4a4038", "#8a7868", "#c8b8a0", "#f0e4d0"]), ("accent", ["#8a7868", "#f0e4d0"])]
        if "ice" in name:
            groups = [("body", ["#3a6888", "#78b0d0", "#d0eef8", "#ffffff"]), ("accent", ["#d0eef8", "#ffffff"])]
        if "lava" in name:
            groups = [("body", ["#3a1408", "#a02810", "#f06020", "#ffd060"]), ("accent", ["#f06020", "#ffd060"])]
        if name == "celeste-snow":
            groups = [("body", ["#243048", "#486080", "#a0b8d0"]), ("accent", ["#e8f4f8", "#ffffff", "#c8e0f0"])]
        add(id=name, game=game, cat="tiles", w=8, h=8, tile=kind, groups=groups,
            anim=None,
            desc=f"8x8 {note} Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.")

    # The list above added terra-tiles plus 15 tiles = 16. Good.

    # props 33-44
    add(id="shaded-crate", game="Octopath Traveler", cat="props", w=16, h=16,
        groups=subset("wood", ["top", "left", "right", "metal"]),
        drawer="prop", prop="iso-crate",
        desc="Isometric crate, 16x16, three faces on three ramps so the right face stays darker than the lit top. Metal latch. This is the sprite version of a baked crate, without a normal map.")
    for name, prop, game, w, h, note in [
        ("barrel", "barrel", "Stardew Valley", 16, 16, "Barrel with two metal hoops."),
        ("signpost", "sign", "Stardew Valley", 16, 16, "Sign board on a post."),
        ("torch", "torch", "Terraria", 16, 16, "Wall torch, flame on a stick. Flame uses a hot ramp."),
        ("lantern", "lantern", "Octopath Traveler", 16, 16, "Hanging lantern, warm glass, dark cap."),
        ("clay-pot", "pot", "Stardew Valley", 16, 16, "Pot with a two-leaf sprout."),
        ("fence", "fence", "Stardew Valley", 16, 16, "Three rails, two posts, one connected piece."),
        ("wooden-door", "door", "Terraria", 16, 24, "Door with a metal handle on the right."),
        ("bed", "bed", "Stardew Valley", 16, 16, "Bed, pillow in cloth, frame in wood."),
        ("table", "table", "Stardew Valley", 16, 16, "Table top and two legs."),
        ("chair", "chair", "Stardew Valley", 16, 16, "Chair, back post and two legs."),
        ("well", "well", "Stardew Valley", 16, 16, "Stone rim, water hole, wooden roof posts."),
    ]:
        groups = subset("wood", ["wood", "lid", "metal", "band", "board", "clay", "leaf", "flame", "glass", "stone", "water", "cloth"])
        add(id=name, game=game, cat="props", w=w, h=h, groups=groups, drawer="prop", prop=prop,
            anim="bob" if prop in ("torch", "lantern", "sign") else "glint",
            desc=f"{note}  Top-left light baked into the ramp, checker dither on the broad face.")

    # items 45-58
    add(id="item-chest", game="Terraria", cat="items", w=16, h=16,
        groups=subset("wood", ["wood", "lid", "metal"]),
        drawer="prop", prop="chest",
        desc="Closed chest, 16x16. Lid is a lighter wood ramp than the body, lock is cool metal and does not move when the body bobs. Wood grain is the dither, not a noise tile.")
    for name, kind, game, note, groups in [
        ("copper-sword", "sword", "Terraria", "Straight sword, cool blade, warm grip.", ["metal", "wood"]),
        ("pickaxe", "pick", "Terraria", "Pick, metal head, wooden haft.", ["metal", "wood"]),
        ("hatchet", "axe", "Terraria", "Hatchet, head on the left of the haft.", ["metal", "wood"]),
        ("watering-can", "can", "Stardew Valley", "Can with a two-pixel spout drop.", ["metal", "water"]),
        ("ruby", "gem", "Terraria", "Gem, red ramp from a cool dark to a warm highlight.", ["gem"]),
        ("gold-coin", "coin", "Stardew Valley", "Coin disc, warm metal ramp.", ["metal"]),
        ("iron-key", "key", "Terraria", "Key, bow and two teeth, one piece.", ["metal"]),
        ("health-potion", "potion", "Stardew Valley", "Bottle, glass ramp and a green liquid.", ["glass", "liquid"]),
        ("berry", "berry", "Stardew Valley", "Berry with a leaf joined to the fruit.", ["fruit", "leaf"]),
        ("fish", "fish", "Stardew Valley", "Side fish, tail joined to the body.", ["body"]),
        ("spellbook", "book", "Octopath Traveler", "Closed book, cover and a page stripe.", ["cover", "page"]),
        ("shortbow", "bow", "Terraria", "Bow stave and a string.", ["wood", "string"]),
        ("wooden-shield", "shield", "Terraria", "Shield disc with a metal rib.", ["wood", "metal"]),
    ]:
        add(id=name, game=game, cat="items", w=16, h=16,
            groups=subset("item", groups), drawer="item", item=kind, anim="glint",
            desc=f"{note} 16x16 icon, top-left light, dither on the largest face.")

    # crops 59-70
    add(id="crop-stages", game="Stardew Valley", cat="crops", w=16, h=32, stages=True,
        groups=subset("plant", ["soil", "stem", "leaf", "fruit"]),
        desc="Four growth stages on 16x32: soil and a short stem, then leaves, then a wider leaf, then fruit. Each stage keeps the soil band so the bbox stays in range. Adjacent stages change only the plant, under the 35% frame-diff cap. Leaves use a yellow-green highlight and a blue-green shadow.")
    add(id="orchard-tree", game="Stardew Valley", cat="crops", w=32, h=40,
        groups=subset("plant", ["leaf", "wood", "fruit"]),
        drawer="tree", anim="bob",
        desc="Tree, 32x40. Three canopy lobes instead of one circle, a three-pixel trunk, fruit pixels tucked into the leaves. Wind would be a row shift; bob is what stays inside the outline and the diff cap.")
    for name, kind, game, note in [
        ("wheat", "wheat", "Stardew Valley", "Wheat ear in the flower ramp."),
        ("parsnip", "parsnip", "Stardew Valley", "Leaf pair and a root at the soil line."),
        ("cauliflower", "cauli", "Stardew Valley", "Broad pale head on a short stem."),
        ("sunflower", "sunflower", "Stardew Valley", "Yellow head, green leaf."),
        ("sapling", "sapling", "Stardew Valley", "Thin stem and a small round crown."),
        ("mushroom", "mushroom", "Terraria", "Red cap, pale stem."),
        ("cactus", "cactus", "Terraria", "Column with two arms."),
        ("pumpkin", "pumpkin", "Stardew Valley", "Orange fruit sitting on the soil."),
        ("tulip", "tulip", "Stardew Valley", "Single bloom on a thin stem."),
        ("berry-bush", "bush", "Stardew Valley", "Low bush with two berries."),
    ]:
        add(id=name, game=game, cat="crops", w=16, h=24,
            groups=subset("plant", ["soil", "stem", "leaf", "flower", "fruit", "cap"]),
            drawer="plant", plant=kind, stage=3, anim="bob",
            desc=f"{note} 16x24, soil band at the bottom, top-left light on the leaves, dither where the leaf is wide.")

    # enemies 71-82
    add(id="meadow-slime", game="Stardew Valley", cat="enemies", w=16, h=16,
        groups=subset("slime", ["body", "eye"]),
        drawer="slime",
        desc="Flat slime, 16x16, desaturated green-grey rather than a saturated green. Highlight on the upper left, dark foot. The hop is a 1px bob. A blink would need an eye rect recomputed after layout, so it is not in this recipe.")
    for name, blob, game, note, groups in [
        ("cave-bat", "bat", "Terraria", "Wings wider than the body, two ears.", ["wing", "body", "eye"]),
        ("skeleton", "skeleton", "Terraria", "Skull, rib ticks, two legs.", ["bone", "eye"]),
        ("floating-eye", "eye", "Terraria", "Round eye, iris, pupil.", ["body", "iris", "pupil"]),
        ("zombie", "zombie", "Terraria", "Front humanoid in a sick green.", ["hair", "skin", "cloth", "leg", "boot", "arm", "eye"]),
        ("bee", "bee", "Stardew Valley", "Striped body, two wings, a sting.", ["body", "wing", "sting", "eye"]),
        ("hermit-crab", "crab", "Stardew Valley", "Shell and two claws.", ["body", "claw", "eye"]),
        ("ghost", "ghost", "Terraria", "Sheet with a wavy hem.", ["body", "eye"]),
        ("earth-worm", "worm", "Stardew Valley", "Segmented worm, one piece.", ["body"]),
        ("spike-ball", "spike", "Dead Cells", "Core with eight spikes joined back to the body.", ["body", "spike"]),
        ("shroom-enemy", "shroom", "Terraria", "Cap and stem with eyes.", ["cap", "stem", "eye"]),
        ("shade-wisp", "wisp", "Dead Cells", "Tall flame with a bright core.", ["body", "core"]),
    ]:
        add(id=name, game=game, cat="enemies", w=16, h=16,
            groups=subset("enemy", groups), drawer="blob", blob=blob, anim="bob",
            desc=f"{note} 16x16, top-left light, dark rim on the lower right.")

    # ui 83-92
    add(id="ui-dialog", game="Undertale", cat="ui", w=32, h=20,
        groups=subset("ui", ["paper", "ink", "cursor"]) or subset("undertale", ["paper", "ink", "cursor"]),
        drawer="ui", ui="dialog",
        desc="Dialogue panel, 32x20. Paper ramp with a cooler shadow, a short ink word joined to a baseline, and a 3x3 cursor that swaps to the paper highlight. Not a 9-slice.")
    for name, ui, game, w, h, note in [
        ("heart-meter", "hearts", "Undertale", 32, 16, "Three hearts over a shared baseline, two filled and one empty."),
        ("menu-button", "button", "Terraria", 24, 12, "Button face with a mid ink bar."),
        ("item-slot", "slot", "Stardew Valley", 16, 16, "Slot, outer face and a darker inset."),
        ("cursor-hand", "cursor", "Stardew Valley", 16, 16, "Pointing hand, one connected shape."),
        ("coin-counter", "counter", "Stardew Valley", 24, 16, "Coin plus a two-digit block."),
        ("nameplate", "nameplate", "Octopath Traveler", 32, 12, "Name plate, one ink bar for the name."),
        ("menu-arrow", "arrow", "Undertale", 16, 16, "Right-pointing arrow."),
        ("stamina-bar", "stamina", "Dead Cells", 32, 12, "Bar frame with a two-thirds fill."),
        ("map-marker", "marker", "Octopath Traveler", 16, 16, "Pin: disc over a short stem."),
    ]:
        add(id=name, game=game, cat="ui", w=w, h=h,
            groups=subset("ui", ["paper", "ink", "face", "inset", "full", "empty", "coin", "hand", "cursor"]),
            drawer="ui", ui=ui, anim="glint" if ui in ("cursor", "hearts", "marker") else None,
            desc=f"{note} Flat UI, two or three ramps, no faux bevel beyond the top-left rim.")

    # effects 93-100
    add(id="soul-bullet", game="Undertale", cat="effects", w=16, h=16, hand="soul",
        groups=subset("fx", ["heart", "glint", "shot"]),
        desc="Red heart, 16x16, four-step ramp from a cool dark red to a warm highlight, one glint pixel. A three-pixel chevron steps along the bottom across four frames. The heart does not spin.")
    add(id="celeste-spikes", game="Celeste", cat="effects", w=8, h=8, hand="spike",
        groups=subset("fx", ["metal"]),
        desc="Single peak on a base, 8x8. rotate90 builds right, down, and left. A two-frame glint swaps the highlight. No shade-before-rotate, so every direction keeps the same value pattern.")
    for name, kind, game, note in [
        ("hit-spark", "spark", "Dead Cells", "Four-point spark."),
        ("puff-explosion", "boom", "Terraria", "Hot disc with a white core."),
        ("bone-bullet", "bone", "Undertale", "Horizontal bone, knobs joined to the shaft."),
        ("ring-bullet", "ring", "Undertale", "White ring, center punched out, still one component."),
        ("slash-arc", "slash", "Dead Cells", "Diagonal slash, two pixels thick."),
        ("dash-crystal", "crystal", "Celeste", "Diamond crystal, cool gem ramp."),
    ]:
        add(id=name, game=game, cat="effects", w=16, h=16,
            groups=subset("fx", ["hot", "core", "bone", "ring", "slash", "gem", "shot", "metal"]),
            drawer="fx", fx=kind, anim="slide" if kind in ("bone", "ring", "spark") else "glint",
            desc=f"{note} 16x16. Motion is a 1px step so the frame diff stays inside the shift allowance.")

    assert len(rows) == 100, len(rows)
    for i, row in enumerate(rows, 1):
        row["n"] = i
    return rows


# Drawing dispatch -----------------------------------------------------------

def celeste_groups():
    return subset("celeste", ["hair", "skin", "cloth", "leg"])


def render_asset(a: dict) -> tuple[list[str], dict[str, list[int]], list[str], dict]:
    """Returns colors, ramps, base rows, extra meta."""
    if a.get("hand") == "run":
        colors, ramps, flash = pal(a["groups"])
        frames = hand_run()
        for fr in frames:
            err = qa_rows(fr)
            if err:
                raise SystemExit(f"{a['id']} hand frame: {err}\n" + "\n".join(fr))
        return colors, ramps, frames[0], {"frames": frames, "flash": flash}
    if a.get("hand") == "soul":
        colors, ramps, flash = pal(a["groups"])
        heart, _ = draw_fx("heart", 16, 16, ramps)
        err = qa_rows(heart)
        if err:
            raise SystemExit(f"soul {err}\n" + "\n".join(heart))
        return colors, ramps, heart, {"rows": heart, "flash": flash}
    if a.get("hand") == "spike":
        colors, ramps, flash = pal(a["groups"])
        # metal ramp indices: find them
        rows, _ = draw_fx("spike", a["w"], a["h"], ramps)
        err = qa_rows(rows)
        if err:
            raise SystemExit(f"{a['id']}: {err}\n" + "\n".join(rows))
        return colors, ramps, rows, {"flash": flash}
    if a.get("sheet") == "tiles":
        colors, ramps, flash = pal([
            ("dirt", ["#3a2a18", "#6a4a28", "#a87840", "#e0b868"]),
            ("grass", ["#1e4a18", "#3e8a28", "#78c040", "#d8f080"]),
            ("stone", ["#3a3a42", "#6a6a74", "#a0a0a8", "#d8d8de"]),
            ("ore", ["#8a3a18", "#e07830", "#f0c060"]),
        ])
        # remap per tile
        tiles = {}
        specs = {
            "dirt": ("dirt", {"body": ramps["dirt"], "accent": ramps["dirt"]}),
            "dirt_b": ("dirt", {"body": ramps["dirt"], "accent": ramps["dirt"]}),
            "grass": ("grass", {"body": ramps["dirt"], "accent": ramps["grass"]}),
            "grass_b": ("grass", {"body": ramps["dirt"], "accent": ramps["grass"]}),
            "ore": ("ore", {"body": ramps["stone"], "accent": ramps["ore"]}),
            "ore_b": ("ore", {"body": ramps["stone"], "accent": ramps["ore"]}),
        }
        # dirt_b / grass_b / ore_b shift the cluster phase by drawing then flipping a row pattern
        for name, (kind, rm) in specs.items():
            rows = draw_tile(kind, 8, 8, rm)
            if name.endswith("_b"):
                # invert dither phase inside, keep edges
                alt = []
                for y, row in enumerate(rows):
                    chars = list(row)
                    if 0 < y < 7:
                        for x in range(1, 7):
                            if (x + y) % 2 == 0 and chars[x] != ".":
                                chars[x] = enc(rm["body"][min(len(rm["body"]) - 1, 1)])
                    alt.append("".join(chars))
                rows = alt
            err = qa_rows(rows, tile=True)
            if err:
                raise SystemExit(f"tile {name}: {err}")
            tiles[name] = rows
        return colors, ramps, tiles["dirt"], {"tiles": tiles, "flash": flash, "tile_ramps": {
            "dirt": "dirt", "dirt_b": "dirt", "grass": "dirt", "grass_b": "dirt", "ore": "stone", "ore_b": "stone",
        }}

    colors, ramps, flash = pal(a["groups"])
    meta: dict = {"flash": flash}
    drawer = a.get("drawer")
    if a.get("tile"):
        rows = draw_tile(a["tile"], a["w"], a["h"], ramps)
        err = qa_rows(rows, tile=True)
    elif a.get("stages"):
        frames = []
        for stage in (1, 2, 3, 4):
            rows, _ = draw_plant("pumpkin" if stage == 4 else "wheat", a["w"], a["h"], ramps, stage=stage)
            # stage 4 uses fruit; earlier stages ignore fruit pixels because stage<3
            err = qa_rows(rows)
            if err:
                raise SystemExit(f"{a['id']} stage {stage}: {err}\n" + "\n".join(rows))
            frames.append(rows)
        # adjacent diff
        for i in range(3):
            d = sum(x != y for r1, r2 in zip(frames[i], frames[i + 1]) for x, y in zip(r1, r2))
            limit = int(0.35 * a["w"] * a["h"])
            if d > limit or d < max(1, int(0.01 * a["w"] * a["h"])):
                raise SystemExit(f"{a['id']} stage diff {i} = {d}, limit {limit}")
        return colors, ramps, frames[0], {"frames": frames, "flash": flash}
    elif drawer == "side":
        rows, extra = draw_side(a["w"], a["h"], ramps, tool=a.get("tool", "none"), pack=a.get("pack", False), hat=a.get("hat", False))
        meta.update(extra)
        err = qa_rows(rows)
    elif drawer == "front":
        rows, extra = draw_front(a["w"], a["h"], ramps, dress=a.get("dress", False), horns=a.get("horns", False))
        meta.update(extra)
        err = qa_rows(rows)
    elif drawer == "slime":
        rows, extra = draw_slime(a["w"], a["h"], ramps)
        meta.update(extra)
        err = qa_rows(rows)
    elif drawer == "blob":
        rows, extra = draw_blob_enemy(a["blob"], a["w"], a["h"], ramps)
        meta.update(extra)
        err = qa_rows(rows)
    elif drawer == "chicken":
        rows, extra = draw_chicken(a["w"], a["h"], ramps)
        meta.update(extra)
        err = qa_rows(rows)
    elif drawer == "prop":
        rows, extra = draw_prop(a["prop"], a["w"], a["h"], ramps)
        meta.update(extra)
        err = qa_rows(rows)
    elif drawer == "item":
        rows, extra = draw_item(a["item"], a["w"], a["h"], ramps)
        meta.update(extra)
        err = qa_rows(rows)
    elif drawer == "plant":
        rows, extra = draw_plant(a["plant"], a["w"], a["h"], ramps, stage=a.get("stage", 3))
        meta.update(extra)
        err = qa_rows(rows)
    elif drawer == "tree":
        rows, extra = draw_tree(a["w"], a["h"], ramps)
        meta.update(extra)
        err = qa_rows(rows)
    elif drawer == "ui":
        rows, extra = draw_ui(a["ui"], a["w"], a["h"], ramps)
        meta.update(extra)
        err = qa_rows(rows)
    elif drawer == "fx":
        rows, extra = draw_fx(a["fx"], a["w"], a["h"], ramps)
        meta.update(extra)
        err = qa_rows(rows)
    else:
        raise SystemExit(f"no drawer for {a['id']}")
    if err:
        raise SystemExit(f"{a['id']}: {err}\n" + "\n".join(rows))
    meta["rows"] = rows
    return colors, ramps, rows, meta


def patches_for(rows: list[str]) -> list[dict]:
    h = len(rows)
    w = len(rows[0])
    ops = []
    y = 0
    while y < h:
        bh = min(16, h - y)
        x = 0
        while x < w:
            bw = min(16, w - x)
            block = [row[x : x + bw] for row in rows[y : y + bh]]
            ops.append({"op": "patch_pixels", "x": x, "y": y, "rows": block})
            x += bw
        y += bh
    return ops


def write_palette(path: Path, asset_id: str, colors, ramps, flash: int):
    path.write_text(json.dumps({
        "v": 1,
        "id": asset_id,
        "colors": colors,
        "ramps": ramps,
        "special": {"transparent": 0, "outline": 1, "flash": flash},
    }, indent=2) + "\n")


def write_shapes(path: Path, w: int, h: int, rows: list[str]):
    path.write_text(json.dumps({"w": w, "h": h, "draw": [{"pixels": rows, "x": 0, "y": 0}]}, indent=2) + "\n")


def anim_block(a, meta, ramps) -> list[dict]:
    if a.get("hand") == "run":
        frames = []
        for fr in meta["frames"]:
            frames.append(patches_for(fr))
        return [{"name": "run", "fps": 8, "loop": True, "frames": frames}]
    if a.get("stages"):
        frames = [patches_for(fr) for fr in meta["frames"]]
        return [{"name": "grow", "fps": 4, "loop": False, "frames": frames}]
    if a.get("hand") == "spike":
        return [{
            "name": "glint",
            "fps": 4,
            "loop": True,
            "dirs": ["base", "right", "down", "left"],
            "frames": [[{"op": "noop"}], [{"op": "palette_swap", "map": f"{ramps['metal'][0]}->{ramps['metal'][-1]}"}]],
        }]
    if a.get("hand") == "soul":
        # built below specially
        return []
    kind = a.get("anim") or ("bob" if not a.get("tile") and not a.get("sheet") else None)
    if a.get("tile") and a.get("anim") == "shimmer":
        # swap a mid ramp step; edges are the darkest/mid and the swap stays on-ramp
        body = ramps["body"]
        if len(body) >= 3:
            return [{"name": "shimmer", "fps": 4, "loop": True, "frames": [
                [{"op": "noop"}],
                [{"op": "palette_swap", "map": f"{body[1]}->{body[2]}"}],
            ]}]
    if kind == "bob":
        # Whole-sprite bob. A blink rect would have to be recomputed after
        # ensure_bbox moves pixels, so the hop/idle bob is the motion.
        return [{"name": "idle", "fps": 6, "loop": True, "macro": "bob(amp=1)"}]
    if kind == "glint":
        # A ramp swap is a no-op when that index is absent. Bob always moves
        # the silhouette and stays inside the 1px shift allowance.
        return [{"name": "idle", "fps": 6, "loop": True, "macro": "bob(amp=1)"}]
    if kind == "slide":
        return [{"name": "move", "fps": 8, "loop": True, "frames": [
            [{"op": "shift", "dx": 0, "dy": 0}],
            [{"op": "shift", "dx": 1, "dy": 0}],
            [{"op": "shift", "dx": 2, "dy": 0}],
            [{"op": "shift", "dx": 1, "dy": 0}],
        ]}]
    return []


def write_recipe(directory: Path, a: dict, ramps, meta):
    if a.get("sheet") == "tiles":
        sprites = []
        for name, rows in meta["tiles"].items():
            write_shapes(directory / f"{name}.shapes.json", 8, 8, rows)
            sprites.append({
                "id": name,
                "kind": "tiles",
                "tile_ramp": meta["tile_ramps"][name],
                "base": {"gen": "shapes", "spec": f"{name}.shapes.json"},
            })
        recipe = {
            "v": 1, "asset": a["id"], "gen_version": 1, "seed": 100 + a["n"],
            "palette": "palette.json",
            "sprites": sprites,
            "sheet": {"sources": list(meta["tiles"]), "layout": "rows-by-anim", "pad": 0, "file": f"{a['id']}.sheet.png"},
            "preview": [{"sheet": True, "scale": 8, "grid": False, "file": "preview.png"}],
            "validate": {"rules": "default"},
        }
        (directory / "recipe.json").write_text(json.dumps(recipe, indent=2) + "\n")
        return
    write_shapes(directory / "sprite.shapes.json", a["w"], a["h"], meta.get("rows") or meta["frames"][0])
    sprite: dict = {
        "id": "sprite",
        "base": {"gen": "shapes", "spec": "sprite.shapes.json"},
    }
    if a.get("tile"):
        sprite["kind"] = "tiles"
        sprite["tile_ramp"] = "body"
    if a.get("hand") == "spike":
        sprite["directions"] = [
            {"id": "right", "from": "base", "ops": [{"op": "rotate90", "dir": "cw"}]},
            {"id": "down", "from": "right", "ops": [{"op": "rotate90", "dir": "cw"}]},
            {"id": "left", "from": "down", "ops": [{"op": "rotate90", "dir": "cw"}]},
        ]
    if a.get("hand") == "soul":
        colors_dummy = None
        heart, _ = draw_fx("heart", 16, 16, ramps)
        # place a chevron using shot ramp on four frames via patches of 3x2
        shot = enc(ramps["shot"][-1])
        frames_ops = []
        positions = [1, 6, 11, 8]
        for x in positions:
            bullet = [shot * 2 + ".", "." + shot + "."]
            frames_ops.append([
                {"op": "patch_pixels", "x": x, "y": 13, "rows": bullet},
            ])
        # also pulse heart on frames 1 and 3
        frames_ops[1] = [{"op": "palette_swap", "map": f"ramp:heart+1"}] + frames_ops[1]
        frames_ops[3] = [{"op": "palette_swap", "map": f"ramp:heart+1"}] + frames_ops[3]
        write_shapes(directory / "sprite.shapes.json", 16, 16, heart)
        sprite["anims"] = [{"name": "dodge", "fps": 8, "loop": True, "frames": frames_ops}]
    else:
        anims = anim_block(a, meta, ramps)
        if anims:
            sprite["anims"] = anims
    anim_name = None
    if sprite.get("anims"):
        anim_name = sprite["anims"][0]["name"]
    preview = {"source": "sprite", "scale": 8, "file": "preview.png"}
    if anim_name:
        preview["anim"] = anim_name
        if a.get("hand") == "spike":
            preview["dir"] = "base"
    recipe = {
        "v": 1,
        "asset": a["id"],
        "gen_version": 1,
        "seed": 100 + a["n"],
        "palette": "palette.json",
        "sprites": [sprite],
        "sheet": {"sources": ["sprite"], "layout": "rows-by-anim", "pad": 1, "file": f"{a['id']}.sheet.png"},
        "preview": [preview],
        "validate": {"rules": "default"},
    }
    (directory / "recipe.json").write_text(json.dumps(recipe, indent=2) + "\n")


# References -----------------------------------------------------------------

ATLAS_URL = "https://raw.githubusercontent.com/NoelFB/Celeste/master/Source/PICO-8/Graphics/atlas.png"
TERRA_BASE = "https://raw.githubusercontent.com/Zorz42/Terralistic/master/"
KENNEY_URL = "https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip"
ROGUE_URL = "https://opengameart.org/sites/default/files/Roguelike%20pack.zip"

TERRA_FILES = {
    "dirt": "base_game/resources/blocks/Template_dirt.png",
    "grass": "base_game/resources/blocks/Template_grass_block.png",
    "stone": "base_game/resources/blocks/Template_stone_block.png",
    "ore": "base_game/resources/blocks/Template_copper_ore.png",
    "sand": "base_game/resources/blocks/sand.png",
    "snow": "base_game/resources/blocks/snowy_grass_block.png",
    "wood": "base_game/resources/blocks/Template_wood_planks.png",
    "ice": "base_game/resources/blocks/ice_block.png",
    "cactus": "base_game/resources/blocks/cactus.png",
    "leaves": "base_game/resources/blocks/leaves.png",
    "canopy": "base_game/resources/blocks/canopy.png",
    "torch": "base_game/resources/blocks/torch.png",
    "water": "base_game/resources/liquids/water.png",
    "hearts": "base_game/resources/misc/hearts.png",
    "pick": "base_game/resources/items/pickaxe.png",
    "axe": "base_game/resources/items/hatchet.png",
    "button": "resources/play_button.png",
}


def load_atlas_cell(c, r, nc=1, nr=1) -> Image.Image:
    im = Image.open(REF_CACHE / "atlas.png").convert("RGBA")
    return im.crop((c * 8, r * 8, (c + nc) * 8, (r + nr) * 8))


def load_terra(key) -> Image.Image:
    name = TERRA_FILES[key].replace("/", "_")
    im = Image.open(REF_CACHE / "terra" / name).convert("RGBA")
    # template strips: show the first two variants side by side if tall
    if im.height > im.width * 2 and im.width <= 16:
        return im.crop((0, 0, im.width, min(im.height, im.width * 2)))
    return im


def load_kenney(folder, index) -> Image.Image:
    path = REF_CACHE / "kenney" / "Tiles" / folder / f"tile_{index:04d}.png"
    if folder == "Tiles":
        path = REF_CACHE / "kenney" / "Tiles" / f"tile_{index:04d}.png"
    return Image.open(path).convert("RGBA")


def load_rogue(c, r) -> Image.Image:
    im = Image.open(REF_CACHE / "roguelike" / "Spritesheet" / "roguelikeSheet_transparent.png").convert("RGBA")
    x, y = c * 17, r * 17
    return im.crop((x, y, x + 16, y + 16))


def reference_for(a: dict) -> tuple[Image.Image, str, str, str]:
    """Image, source url, note, license."""
    i = a["id"]
    # Explicit, checked sources.
    atlas = {
        "celeste-run": (1, 0, 4, 1, "atlas cells (1..4, 0), the 8x8 run cycle"),
        "celeste-dash": (5, 0, 1, 1, "atlas cell (5, 0), a later run pose used as the dash reference"),
        "celeste-climb": (6, 0, 1, 1, "atlas cell (6, 0)"),
        "celeste-hair": (2, 0, 1, 1, "atlas cell (2, 0), hair mass of the runner"),
        "celeste-spikes": (1, 1, 1, 1, "atlas cell (1, 1), the upward spike pair"),
        "celeste-snow": (0, 2, 1, 1, "atlas cell (0, 2), a full ground tile"),
        "dash-crystal": (12, 1, 1, 1, "atlas cell (12, 1)"),
    }
    if i in atlas:
        c, r, nc, nr, note = atlas[i]
        return load_atlas_cell(c, r, nc, nr), ATLAS_URL, note, "NoelFB/Celeste PICO-8 graphics (review reference; MIT covers the code only)"
    terra = {
        "terra-tiles": "grass",
        "terra-dirt": "dirt",
        "terra-grass": "grass",
        "terra-stone": "stone",
        "terra-ore": "ore",
        "terra-sand": "sand",
        "terra-snow": "snow",
        "terra-wood": "wood",
        "terra-ice": "ice",
        "cactus": "cactus",
        "torch": "torch",
        "lava-crust": "ore",
        "pickaxe": "pick",
        "hatchet": "axe",
        "menu-button": "button",
        "soul-bullet": "hearts",
        "heart-meter": "hearts",
    }
    if i in terra or a.get("plant") == "cactus":
        key = terra.get(i, "cactus" if a.get("plant") == "cactus" else None)
        if key:
            rel = TERRA_FILES[key]
            return load_terra(key), TERRA_BASE + rel, f"Terralistic {rel}, first cell of the strip", "GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)"
    # Kenney platformer characters for humanoids, tiles for the rest.
    char_ids = {
        "undertale-frisk": 0, "undertale-frisk-side": 1, "stardew-farmer": 2, "stardew-water": 3,
        "terra-miner": 4, "deadcells-idle": 5, "deadcells-roll": 6, "octopath-hero": 7,
        "octopath-dancer": 8, "stardew-chicken": 11, "terra-guide": 9, "undertale-flowey": 12,
        "meadow-slime": 14, "cave-bat": 15, "bee": 16, "ghost": 17, "zombie": 18,
    }
    if i in char_ids:
        idx = char_ids[i]
        return (
            load_kenney("Characters", idx),
            KENNEY_URL,
            f"Kenney Pixel Platformer Tiles/Characters/tile_{idx:04d}.png",
            "CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)",
        )
    # Roguelike sheet cells chosen by category. Coordinates are 16px + 1px gap.
    rogue = {
        "stardew-path": (1, 0), "stardew-hoedirt": (2, 0), "undertale-ruins": (5, 1),
        "deadcells-brick": (6, 1), "octopath-cobble": (3, 0),
        "barrel": (8, 6), "signpost": (10, 6), "lantern": (12, 5), "clay-pot": (4, 8),
        "fence": (2, 4), "wooden-door": (0, 6), "bed": (6, 8), "table": (4, 7),
        "chair": (5, 7), "well": (9, 5), "shaded-crate": (7, 6), "item-chest": (8, 7),
        "copper-sword": (0, 10), "watering-can": (4, 11), "ruby": (6, 10), "gold-coin": (8, 10),
        "iron-key": (2, 10), "health-potion": (3, 11), "berry": (5, 12), "fish": (7, 12),
        "spellbook": (1, 11), "shortbow": (9, 10), "wooden-shield": (10, 10),
        "crop-stages": (2, 12), "orchard-tree": (0, 8), "wheat": (1, 12), "parsnip": (3, 12),
        "cauliflower": (4, 12), "sunflower": (6, 12), "sapling": (1, 8), "mushroom": (8, 12),
        "pumpkin": (9, 12), "tulip": (10, 12), "berry-bush": (2, 8),
        "skeleton": (14, 14), "floating-eye": (15, 14), "hermit-crab": (12, 14),
        "earth-worm": (11, 14), "spike-ball": (13, 15), "shroom-enemy": (8, 12),
        "shade-wisp": (16, 14), "ui-dialog": (0, 18), "item-slot": (2, 18),
        "cursor-hand": (4, 19), "coin-counter": (8, 10), "nameplate": (1, 18),
        "menu-arrow": (5, 19), "stamina-bar": (3, 18), "map-marker": (6, 19),
        "hit-spark": (10, 15), "puff-explosion": (11, 15), "bone-bullet": (12, 15),
        "ring-bullet": (13, 15), "slash-arc": (14, 15),
    }
    if i in rogue:
        c, r = rogue[i]
        return (
            load_rogue(c, r),
            ROGUE_URL,
            f"Kenney Roguelike sheet cell ({c}, {r}) at 16px with a 1px gap",
            "CC0 Kenney roguelike/RPG pack (OpenGameArt)",
        )
    # fallback: a kenney ground tile
    return (
        load_kenney("Tiles", a["n"] % 20),
        KENNEY_URL,
        f"Kenney Pixel Platformer Tiles/tile_{(a['n'] % 20):04d}.png",
        "CC0 Kenney",
    )


def write_descriptions(catalog: list[dict]):
    lines = [
        "# v3 reference descriptions",
        "",
        "Each asset is an original pxl recipe. The reference image is a real file from the web, cropped to the cited cell. Palettes put the cooler, darker step first and the warmer highlight last. Shading is top-left: lit rim on the top and left edge, dark rim on the bottom and right, checker dither on the mid band of wide faces.",
        "",
        "Index 1 of every palette is reserved and unused, so a broken highlight rim does not trip the closed-outline QA rule.",
        "",
    ]
    for a in catalog:
        lines.append(f"## {a['n']:02d} {a['id']}")
        lines.append("")
        lines.append(f"- Game style: {a['game']}")
        lines.append(f"- Category: {a['cat']}")
        lines.append(f"- Size: {a['w']}×{a['h']} px")
        lines.append(f"- Shape: {a['desc']}")
        lines.append(f"- Reference: {a['source_note']}")
        lines.append(f"- Source URL: {a['source_url']}")
        lines.append(f"- License of the reference pixels: {a['license']}")
        lines.append(f"- Animation: {a.get('anim_label', 'see recipe')}")
        lines.append("")
    (OUT_ROOT / "descriptions.md").write_text("\n".join(lines))


def cmd_write():
    if OUT_ROOT.exists():
        shutil.rmtree(OUT_ROOT)
    OUT_ROOT.mkdir(parents=True)
    catalog = []
    for a in assets():
        colors, ramps, rows, meta = render_asset(a)
        if a.get("hand") == "soul":
            # draw_fx heart validated inside write via render path: build heart now
            heart, _ = draw_fx("heart", 16, 16, ramps)
            err = qa_rows(heart)
            if err:
                raise SystemExit(f"soul {err}\n" + "\n".join(heart))
            meta["rows"] = heart
        directory = OUT_ROOT / a["id"]
        directory.mkdir()
        flash = meta["flash"]
        write_palette(directory / "palette.json", a["id"], colors, ramps, flash)
        # stash rows for non-sheet
        if not a.get("sheet"):
            meta.setdefault("rows", rows)
        write_recipe(directory, a, ramps, meta)
        ref, url, note, lic = reference_for(a)
        ref.save(directory / "reference.png")
        anim_label = a.get("anim") or a.get("hand") or ("grow" if a.get("stages") else "static")
        entry = {
            "n": a["n"],
            "id": a["id"],
            "game": a["game"],
            "category": a["cat"],
            "w": a["w"],
            "h": a["h"],
            "description": a["desc"],
            "source_url": url,
            "source_note": note,
            "license": lic,
            "anim": anim_label,
            "style": None,
            "readability": None,
        }
        catalog.append(entry)
        (directory / "description.json").write_text(json.dumps(entry, indent=2) + "\n")
        print(f"wrote {a['n']:02d} {a['id']}")
    (OUT_ROOT / "catalog.json").write_text(json.dumps(catalog, indent=2) + "\n")
    # descriptions need anim_label
    for e in catalog:
        e["anim_label"] = e["anim"]
        e["desc"] = e["description"]
        e["cat"] = e["category"]
    write_descriptions(catalog)
    print(f"catalog {len(catalog)}")


def run_pxl(args: list[str]) -> dict:
    proc = subprocess.run([str(PXL), "--json", *args], capture_output=True, text=True)
    line = ""
    for raw in (proc.stdout or "").splitlines():
        if raw.startswith("{"):
            line = raw
    if not line:
        raise SystemExit(f"pxl failed {args}\n{proc.stderr}\n{proc.stdout}")
    data = json.loads(line)
    data["_code"] = proc.returncode
    return data


def cmd_build():
    catalog = json.loads((OUT_ROOT / "catalog.json").read_text())
    BUILD_ROOT.mkdir(parents=True, exist_ok=True)
    failed = []
    for e in catalog:
        out = BUILD_ROOT / e["id"]
        if out.exists():
            shutil.rmtree(out)
        data = run_pxl(["build", str(OUT_ROOT / e["id"] / "recipe.json"), "-o", str(out)])
        bad = [c for c in data.get("checks", []) if not c.get("pass")]
        if data.get("_code") != 0 or bad:
            failed.append((e["id"], data.get("_code"), bad[:8], data.get("error")))
            print("FAIL", e["id"], data.get("error") or bad[:3])
        else:
            print("ok", e["id"])
    if failed:
        print(f"\n{len(failed)} failed")
        for item in failed:
            print(item[0], item[1], item[3] or item[2])
        sys.exit(1)


LEGACY = {
    "celeste-run": "celeste-run",
    "celeste-spikes": "celeste-spikes",
    "terra-tiles": "terra-tiles",
    "crop-stages": "crop-stages",
    "orchard-tree": "orchard-tree",
    "item-chest": "item-chest",
    "soul-bullet": "soul-bullet",
    "ui-dialog": "ui-dialog",
    "meadow-slime": "meadow-slime",
    "shaded-crate": "shaded-crate",
}


def build_legacy():
    LEGACY_BUILD.mkdir(parents=True, exist_ok=True)
    for name in LEGACY.values():
        out = LEGACY_BUILD / name
        if out.exists():
            shutil.rmtree(out)
        data = run_pxl(["build", str(ROOT / "examples" / "reference" / name / "recipe.json"), "-o", str(out)])
        if data.get("_code") != 0:
            print("legacy fail", name, data.get("error"))
        else:
            print("legacy", name)


def sprite_strip(build_dir: Path) -> Image.Image | None:
    """Prefer the anim/preview PNG written by the recipe, else the sheet."""
    preview = build_dir / "preview.png"
    if preview.exists():
        return Image.open(preview).convert("RGBA")
    sheets = list(build_dir.glob("*.sheet.png"))
    if sheets:
        return Image.open(sheets[0]).convert("RGBA")
    return None


def legacy_strip(name: str) -> Image.Image | None:
    directory = LEGACY_BUILD / name
    # v2 recipes write named previews, not always preview.png
    pngs = [p for p in directory.glob("*.png") if "sheet" not in p.name and p.name != "palette.png"]
    # prefer a non-sheet preview
    previews = [p for p in pngs if p.name != f"{name}.sheet.png"]
    if previews:
        return Image.open(previews[0]).convert("RGBA")
    sheets = list(directory.glob("*.sheet.png"))
    if sheets:
        return Image.open(sheets[0]).convert("RGBA")
    return None


def nearest(im: Image.Image, scale: int) -> Image.Image:
    scale = max(1, scale)
    return im.resize((im.width * scale, im.height * scale), Image.NEAREST)


def fit_scale(im: Image.Image, target_h: int = 96, max_w: int = 360) -> Image.Image:
    # scale by the native frame height if the image is a 1x strip
    h = im.height
    scale = max(1, target_h // max(h, 1))
    while scale > 1 and im.width * scale > max_w:
        scale -= 1
    return nearest(im, scale)


def checker(w, h, a=(48, 52, 64, 255), b=(32, 36, 46, 255), n=8) -> Image.Image:
    im = Image.new("RGBA", (w, h), a)
    px = im.load()
    for y in range(h):
        for x in range(w):
            if ((x // n) + (y // n)) % 2 == 0:
                px[x, y] = b
    return im


def paste_centered(canvas, im, box):
    x0, y0, x1, y1 = box
    ox = x0 + max(0, (x1 - x0 - im.width) // 2)
    oy = y0 + max(0, (y1 - y0 - im.height) // 2)
    canvas.paste(im, (ox, oy), im)


def compose_one(entry: dict, v3: Image.Image, ref: Image.Image, prev: Image.Image | None) -> Image.Image:
    col_w, col_h = 280, 150
    header, footer = 28, 36
    cols = 3
    img = Image.new("RGBA", (col_w * cols, header + col_h + footer), (22, 24, 32, 255))
    dr = ImageDraw.Draw(img)
    title = f"{entry['n']:02d}  {entry['id']}   {entry['game']}  /  {entry['category']}"
    dr.text((8, 6), title, fill=(240, 236, 220, 255), font=FONT)
    labels = ["reference", "v2" if prev is not None else "v2 (none)", "v3"]
    images = [ref, prev, v3]
    for i, (lab, im) in enumerate(zip(labels, images)):
        x = i * col_w
        dr.text((x + 8, header - 2), lab, fill=(180, 190, 210, 255), font=FONT_SM)
        bg = checker(col_w - 16, col_h - 22)
        img.paste(bg, (x + 8, header + 16))
        if im is None:
            dr.text((x + 24, header + 60), "no previous round", fill=(140, 140, 150, 255), font=FONT_SM)
            continue
        shown = fit_scale(im)
        paste_centered(img, shown, (x + 8, header + 16, x + 8 + bg.width, header + 16 + bg.height))
    score = ""
    if entry.get("style") is not None:
        score = f"style {entry['style']}/10    readability {entry['readability']}/10"
    dr.text((8, header + col_h + 8), score or entry.get("source_note", "")[:90], fill=(200, 200, 190, 255), font=FONT_SM)
    return img


def cmd_compare():
    catalog = json.loads((OUT_ROOT / "catalog.json").read_text())
    build_legacy()
    REVIEW.mkdir(parents=True, exist_ok=True)
    sheets: dict[str, list[Image.Image]] = {}
    for e in catalog:
        v3 = sprite_strip(BUILD_ROOT / e["id"])
        if v3 is None:
            raise SystemExit(f"no render for {e['id']}")
        ref = Image.open(OUT_ROOT / e["id"] / "reference.png").convert("RGBA")
        prev = legacy_strip(LEGACY[e["id"]]) if e["id"] in LEGACY else None
        card = compose_one(e, v3, ref, prev)
        name = f"{e['n']:02d}-{e['id']}.png"
        card.save(REVIEW / name)
        sheets.setdefault(e["category"], []).append(card)
        print("card", name)
    # category contact sheets + one overview of downscaled cards
    contacts = []
    for cat, cards in sheets.items():
        path = contact(cards, REVIEW / f"contact-{cat}.png", columns=2)
        contacts.append(path)
    # overview: use the category sheets stacked is huge; make a thumb grid of cards
    thumbs = []
    for e in catalog:
        im = Image.open(REVIEW / f"{e['n']:02d}-{e['id']}.png")
        thumbs.append(im.resize((im.width // 2, im.height // 2), Image.NEAREST))
    overview = contact(thumbs, REVIEW / "contact-sheet.png", columns=4)
    contacts.append(overview)
    art = Path("/opt/cursor/artifacts")
    art.mkdir(parents=True, exist_ok=True)
    for p in contacts:
        shutil.copy(p, art / p.name)
    print("contacts", [str(p) for p in contacts])


def contact(cards: list[Image.Image], path: Path, columns: int) -> Path:
    if not cards:
        return path
    w = max(c.width for c in cards)
    h = max(c.height for c in cards)
    cols = columns
    rows = (len(cards) + cols - 1) // cols
    sheet = Image.new("RGBA", (cols * w, rows * h), (14, 16, 22, 255))
    for i, c in enumerate(cards):
        sheet.paste(c, ((i % cols) * w, (i // cols) * h))
    sheet.save(path)
    return path


def main():
    if len(sys.argv) < 2:
        print("usage: v3_make.py write|build|compare")
        sys.exit(2)
    {"write": cmd_write, "build": cmd_build, "compare": cmd_compare}[sys.argv[1]]()


if __name__ == "__main__":
    main()
