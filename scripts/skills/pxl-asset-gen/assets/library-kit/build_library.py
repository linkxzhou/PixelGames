"""Build a browsable pixel-asset library folder with the pxl-asset-gen binary.

    python3 build_library.py --project <out_dir> [--pxl <bin>] [--limit N] [--jobs N] [--clean]

Binary: --pxl > $PXL_BIN > <skill>/bin/pxl-asset-gen-<platform> (found when run from the skill).

<out_dir>/tools/project.py must define CONFIG (dict) and populate(lib); it may
define scene(lib) -> Grid for the hero image. Output layout (all relative):

    index.html  manifest.json  validation.json  palette.json
    assets/<id>/<id>.png|.pxl.json|.sheet.png   sources/<id>/<id>.ascii.json|.recipe.json
    atlases/<category>.png|.json                previews/<category>_NN.png|.index.json, scene.png
    tools/  (project modules + vendored kit, so the folder rebuilds standalone)
"""
import argparse
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from art_kit import ASCII_SIZES, Grid, Library, dump  # noqa: E402
import gallery  # noqa: E402

KIT_FILES = ('art_kit.py', 'build_library.py', 'gallery.py', 'animation_gallery.py', 'verify_library.py')
OUTPUT_DIRS = ('assets', 'sources', 'atlases', 'previews')


def bin_names():
    """Skill binary names for this machine, most specific first."""
    system = platform.system().lower()
    arch = {'arm64': 'arm64', 'aarch64': 'arm64', 'x86_64': 'x64', 'amd64': 'x64'}.get(
        platform.machine().lower(), platform.machine().lower())
    names = [f'pxl-asset-gen-{system}-{arch}']
    if system == 'linux' and arch == 'x64':
        names.insert(0, 'pxl-asset-gen-linux')
    return names


def find_pxl(explicit, root=None):
    """--pxl > $PXL_BIN > <skill>/bin/pxl-asset-gen-<platform>."""
    if explicit:
        return Path(explicit).resolve()
    if os.environ.get('PXL_BIN'):
        return Path(os.environ['PXL_BIN']).resolve()
    skill_bin = HERE.parent.parent / 'bin'
    for name in bin_names():
        if (skill_bin / name).is_file():
            return skill_bin / name
    sys.exit(f'no {bin_names()[0]} found: pass --pxl <skill>/bin/pxl-asset-gen-<platform> or set PXL_BIN')


class Pxl:
    def __init__(self, exe, cwd):
        self.exe, self.cwd = exe, cwd

    def __call__(self, *args):
        res = subprocess.run([str(self.exe), *map(str, args)], cwd=self.cwd, text=True, capture_output=True)
        if res.returncode:
            raise RuntimeError(f'{self.exe.name} {" ".join(map(str, args))}\n{res.stdout}\n{res.stderr}')
        return res.stdout


def load_project(root):
    tools = root / 'tools'
    if not (tools / 'project.py').is_file():
        sys.exit(f'{tools / "project.py"} is missing; copy <skill>/assets/project_template/project.py there first')
    sys.path.insert(0, str(tools))
    return importlib.import_module('project')


def vendor_kit(root):
    dest = root / 'tools'
    if dest.resolve() == HERE:
        return
    for name in KIT_FILES:
        shutil.copy2(HERE / name, dest / name)


def build_asset(pxl, lib, asset):
    recipe = lib.write_source(asset)
    out = lib.root / 'assets' / asset['id']
    pxl('build', recipe, '-o', out, '--overwrite')
    doc = out / f'{asset["id"]}.pxl.json'
    result = json.loads(pxl('validate', doc, '--json'))
    pxl('render', doc, '--asset', '-o', out / f'{asset["id"]}.png', '--overwrite')
    return dict(id=asset['id'], ok=bool(result.get('ok')), result=result)


def board_cell(config, category, group):
    fixed = config.get('board_cell', {}).get(category)
    if fixed:
        return fixed
    need = max(max(a['source_rect'][2], a['source_rect'][3]) for a in group)
    return max(16, -(-need // 16) * 16)


def boards(lib, category, group):
    cell = board_cell(lib.config, category, group)
    want = cell * max(1, math.ceil(math.sqrt(len(group))))
    size = next((s for s in ASCII_SIZES if s >= want), ASCII_SIZES[-1])
    size = max(size, next(s for s in ASCII_SIZES if s >= cell))
    cols = size // cell
    style = lib.config.get('board', {})
    bg = style.get('bg', lib.glyph.get(2, lib.glyph[1]))
    frame = style.get('frame', lib.glyph[1])
    made = []
    for page, start in enumerate(range(0, len(group), cols * cols)):
        g, slots = Grid(size, bg), []
        for idx, a in enumerate(group[start:start + cols * cols]):
            x, y = idx % cols * cell, idx // cols * cell
            g.frame(x, y, cell, cell, frame)
            rx, ry, w, h = a['source_rect']
            g.stamp(x + (cell - w) // 2, y + (cell - h) // 2, a['grid'].crop([rx, ry, w, h]))
            slots.append(dict(id=a['id'], name=a['name'], x=x, y=y, size=cell))
        board = f'{category}_{page + 1:02}'
        dump(lib.root / 'previews' / f'{board}.ascii.json', lib.ascii_spec(g, '../palette.json'))
        dump(lib.root / 'previews' / f'{board}.index.json', slots)
        made.append(board)
    return made


def entry(lib, a):
    e = {k: v for k, v in a.items() if k != 'grid' and v is not None}
    aid = a['id']
    e.update(canvas=[a['grid'].size] * 2, png=f'assets/{aid}/{aid}.png',
             doc=f'assets/{aid}/{aid}.pxl.json', source=f'sources/{aid}/{aid}.ascii.json',
             recipe=f'sources/{aid}/{aid}.recipe.json')
    e['sha256'] = hashlib.sha256((lib.root / e['png']).read_bytes()).hexdigest()
    return e


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    default_root = HERE.parent if (HERE / 'project.py').is_file() else None
    ap.add_argument('--project', type=Path, default=default_root, required=default_root is None)
    ap.add_argument('--pxl')
    ap.add_argument('--limit', type=int, help='smoke-test the first N assets (manifest marks partial)')
    ap.add_argument('--jobs', type=int, default=os.cpu_count() or 4)
    ap.add_argument('--clean', action='store_true', help='remove assets/sources/atlases/previews first')
    args = ap.parse_args()
    root = args.project.resolve()
    project = load_project(root)
    config = project.CONFIG
    pxl = Pxl(find_pxl(args.pxl, root), root)
    if args.clean:
        for d in OUTPUT_DIRS:
            shutil.rmtree(root / d, ignore_errors=True)
    vendor_kit(root)
    lib = Library(root, config)
    project.populate(lib)
    lib.write_palette()
    selected = lib.assets[:args.limit] if args.limit else lib.assets
    with ThreadPoolExecutor(max(1, args.jobs)) as pool:
        checks = list(pool.map(lambda a: build_asset(pxl, lib, a), selected))
    print(f'built + validated {len(checks)} assets', file=sys.stderr)

    groups = defaultdict(list)
    for a in selected:
        groups[a['category']].append(a)
    board_ids, atlases = [], []
    for category, group in groups.items():
        board_ids.extend(boards(lib, category, group))
        docs = [root / 'assets' / a['id'] / f'{a["id"]}.pxl.json' for a in group]
        pxl('sheet', *docs, '--layout', 'grid', '--pad', '1', '-o', root / 'atlases' / f'{category}.png', '--overwrite')
        atlases.append((f'atlases/{category}.png', f'atlases/{category}.json'))
    scale = config.get('preview_scale', 3)
    for b in board_ids:
        pxl('ascii', 'render', root / 'previews' / f'{b}.ascii.json', '--scale', scale,
            '-o', root / 'previews' / f'{b}.png', '--overwrite')
    hero = f'previews/{board_ids[0]}.png' if board_ids else None
    if hasattr(project, 'scene') and not args.limit:
        spec = root / 'previews' / 'scene.ascii.json'
        dump(spec, lib.ascii_spec(project.scene(lib), '../palette.json'))
        pxl('ascii', 'render', spec, '--scale', scale, '-o', root / 'previews' / 'scene.png', '--overwrite')
        hero = 'previews/scene.png'

    entries = [entry(lib, a) for a in selected]
    version = pxl('--version').strip()
    dump(root / 'manifest.json', dict(
        version=1, library=config['id'], title=config.get('title', config['id']),
        license_note=config.get('license_note', ''), seed=lib.seed, palette='palette.json',
        tool_version=version, partial=bool(args.limit), count=len(entries),
        categories=dict(Counter(a['category'] for a in selected)), brief=config.get('brief', {}),
        scope=config.get('scope', ''), deferred=config.get('deferred', []), hero=hero,
        atlases=[p for p, _ in atlases], previews=[f'previews/{b}.png' for b in board_ids], assets=entries))
    failed = [c['id'] for c in checks if not c['ok']]
    dump(root / 'validation.json', dict(
        status='FAIL' if failed else 'PASS', scope='validate on every exported asset document',
        asset_count=len(checks), failed=failed, checks=checks, tool_version=version,
        visual_review='NOT_ASSESSED', animation_playback='NOT_ASSESSED'))
    index = gallery.write(root, config, entries, [(f'previews/{b}.png', f'previews/{b}.index.json')
                                                  for b in board_ids], atlases, hero)
    print(json.dumps({'assets': len(entries), 'categories': len(groups), 'boards': len(board_ids),
                      'failed': failed, 'index': str(index), 'pxl': str(pxl.exe)}, ensure_ascii=False))
    sys.exit(1 if failed else 0)


if __name__ == '__main__':
    main()
