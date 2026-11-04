"""Authoring kit for asset libraries: ASCII grids in, recipes out.

Python only places palette glyphs on a grid; the pxl-asset-gen binary owns rasterization,
validation, sheets and PNG export. Never write image pixels from here.
"""
import json
from pathlib import Path
import re

DEFAULT_GLYPHS = '.#abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'
ASCII_SIZES = (32, 64, 128, 256)  # `ascii` only accepts these square canvases


def dump(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n')


class Grid:
    """Square glyph canvas. '.' is always transparent (palette index 0)."""

    legend: 'dict[str, int] | None' = None

    def __init__(self, size=32, fill='.'):
        self.size = size
        self.rows = [list(fill * size) for _ in range(size)]

    def mark(self, x, y, glyph):
        if 0 <= x < self.size and 0 <= y < self.size:
            if Grid.legend is not None:
                assert glyph in Grid.legend, f'glyph {glyph!r} is not in the palette legend'
            self.rows[y][x] = glyph
        return self

    def rect(self, x, y, w, h, glyph):
        for yy in range(y, y + h):
            for xx in range(x, x + w):
                self.mark(xx, yy, glyph)
        return self

    def frame(self, x, y, w, h, glyph):
        self.rect(x, y, w, 1, glyph).rect(x, y + h - 1, w, 1, glyph)
        return self.rect(x, y, 1, h, glyph).rect(x + w - 1, y, 1, h, glyph)

    def stamp(self, x, y, text, remap=None):
        """Paste a multi-line string or row list; '.' cells are skipped."""
        lines = text.strip('\n').splitlines() if isinstance(text, str) else text
        for yy, row in enumerate(lines):
            for xx, ch in enumerate(row):
                if ch != '.':
                    self.mark(x + xx, y + yy, (remap or {}).get(ch, ch))
        return self

    def line(self, x0, y0, x1, y1, ch):
        dx, dy = abs(x1 - x0), -abs(y1 - y0)
        sx, sy = (1 if x0 < x1 else -1), (1 if y0 < y1 else -1)
        err = dx + dy
        while True:
            self.mark(x0, y0, ch)
            if (x0, y0) == (x1, y1):
                return self
            e = 2 * err
            if e >= dy:
                err += dy
                x0 += sx
            if e <= dx:
                err += dx
                y0 += sy

    def oval(self, x, y, w, h, ch):
        for yy in range(h):
            for xx in range(w):
                if ((2 * xx + 1 - w) / w) ** 2 + ((2 * yy + 1 - h) / h) ** 2 <= 1:
                    self.mark(x + xx, y + yy, ch)
        return self

    def mirror_x(self, width=None):
        """Copy the left half onto the right half (symmetric props/characters)."""
        w = width or self.size
        for row in self.rows:
            for xx in range(w // 2):
                row[w - 1 - xx] = row[xx]
        return self

    def replace(self, mapping):
        self.rows = [[mapping.get(c, c) for c in r] for r in self.rows]
        return self

    def clone(self):
        g = Grid(self.size)
        g.rows = [r[:] for r in self.rows]
        return g

    def crop(self, rect):
        x, y, w, h = rect
        return [r[x:x + w] for r in self.strings()[y:y + h]]

    def strings(self):
        return [''.join(r) for r in self.rows]

    def used(self):
        return set(''.join(self.strings())) - {'.'}


class Library:
    """Asset registry plus writers for palette / ASCII spec / recipe sources."""

    def __init__(self, root, config):
        self.root = Path(root)
        self.config = config
        pal = config['palette']
        self.colors = pal['colors']
        assert self.colors[0].lower().endswith('00'), 'palette index 0 must be transparent'
        glyphs = pal.get('glyphs', DEFAULT_GLYPHS)
        assert glyphs[0] == '.', "glyph 0 must be '.' (transparent)"
        assert len(glyphs) >= len(self.colors), 'need one glyph per palette colour'
        assert len(set(glyphs)) == len(glyphs), 'glyphs must be unique'
        self.legend = dict(zip(glyphs, range(len(self.colors))))
        self.glyph = {i: g for g, i in self.legend.items()}
        Grid.legend = self.legend
        self.seed = config.get('seed', 1)
        self.assets = []
        self._ids = set()

    def g(self, ramp, level):
        """Glyph for palette ramp `ramp` at `level` (0 = darkest)."""
        return self.glyph[self.config['palette']['ramps'][ramp][level]]

    def add(self, asset_id, name, category, grid, section=None, region=None, **meta):
        assert asset_id not in self._ids, f'duplicate asset id {asset_id}'
        assert re.fullmatch(r'[a-z0-9_]+', asset_id), f'asset id must be snake_case: {asset_id}'
        assert grid.size in ASCII_SIZES, f'{asset_id}: canvas {grid.size} not in {ASCII_SIZES}; ' \
            'put small art in a 32 canvas and pass region=[0, 0, 16, 16]'
        limit = self.config.get('max_colors', 16)
        if grid.size >= 64:
            limit = self.config.get('max_colors_large', len(self.colors) - 1)
        assert len(grid.used()) <= limit, (asset_id, sorted(grid.used()), limit)
        assert grid.used(), f'{asset_id} is empty'
        self._ids.add(asset_id)
        asset = dict(id=asset_id, name=name, category=category, grid=grid,
                     section=section, source_rect=region or [0, 0, grid.size, grid.size], **meta)
        self.assets.append(asset)
        return grid

    def get(self, asset_id):
        return next(a for a in self.assets if a['id'] == asset_id)

    def place(self, target, asset_id, x, y):
        """Stamp an asset's source_rect onto another grid (scenes, boards)."""
        a = self.get(asset_id)
        target.stamp(x, y, a['grid'].crop(a['source_rect']))
        return target

    def ascii_spec(self, grid, palette_rel):
        return dict(v=1, charset='printable', size=grid.size, cell=[1, 1],
                    palette=palette_rel, legend=self.legend,
                    frames={'base': {'rows': grid.strings()}})

    def write_palette(self):
        pal = self.config['palette']
        dump(self.root / 'palette.json', dict(
            v=1, id=pal.get('id', 'library'), colors=self.colors,
            ramps=pal.get('ramps', {}), special=pal.get('special', {'transparent': 0})))

    def write_source(self, asset):
        asset_id, grid = asset['id'], asset['grid']
        folder = self.root / 'sources' / asset_id
        dump(folder / f'{asset_id}.ascii.json', self.ascii_spec(grid, '../../palette.json'))
        dump(folder / f'{asset_id}.recipe.json', dict(
            v=1, asset=asset_id, seed=self.seed, palette='../../palette.json',
            sprites=[dict(id=asset_id, base={'gen': 'ascii', 'spec': f'{asset_id}.ascii.json'},
                          anims=[dict(name='idle', fps=1, loop=True, mode='static',
                                      frames=[[{'op': 'noop'}]])])],
            sheet={'sources': [asset_id], 'layout': 'rows-by-anim', 'pad': 1,
                   'file': f'{asset_id}.sheet.png'}))
        return folder / f'{asset_id}.recipe.json'
