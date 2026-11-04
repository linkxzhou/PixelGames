"""Read-only integrity checks for a built library; never draws or edits pixels.

    python3 verify_library.py --project <out_dir> [--pxl <bin>] [--samples id,id]

Checks: manifest ids/count, file links, PNG size / palette membership / content
inside source_rect / seamless opposite edges for tile=True (needs Pillow),
manifest SHA-256, clean-rebuild determinism on samples, 500-line limit in tools/.
Results are merged into validation.json.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from art_kit import dump  # noqa: E402
from build_library import find_pxl  # noqa: E402

try:
    from PIL import Image
except ImportError:  # pixel checks become NOT_ASSESSED
    Image = None


def pixel_checks(root, manifest, problems):
    assert Image is not None
    palette = json.loads((root / manifest['palette']).read_text())['colors']
    allowed = {tuple(bytes.fromhex(c[1:7])) + (255,) for c in palette[1:]}
    tiles = []
    for a in manifest['assets']:
        with Image.open(root / a['png']) as image:
            rgba = image.convert('RGBA')
        if list(rgba.size) != a['canvas']:
            problems.append(f'{a["id"]}: size {rgba.size} != canvas {a["canvas"]}')
        colors = {c for _, c in rgba.getcolors(rgba.width * rgba.height)}
        if not all(c[3] == 0 or c in allowed for c in colors):
            problems.append(f'{a["id"]}: colours outside palette')
        box = rgba.getbbox()
        x, y, w, h = a['source_rect']
        if not box:
            problems.append(f'{a["id"]}: empty image')
        elif not (x <= box[0] and box[2] <= x + w and y <= box[1] and box[3] <= y + h):
            problems.append(f'{a["id"]}: content {box} leaks outside source_rect')
        if a.get('tile'):
            t = rgba.crop((x, y, x + w, y + h))
            seam = all(t.getpixel((0, i)) == t.getpixel((w - 1, i)) for i in range(h)) and \
                all(t.getpixel((i, 0)) == t.getpixel((i, h - 1)) for i in range(w))
            if not seam:
                problems.append(f'{a["id"]}: tile opposite edges differ')
            tiles.append(a['id'])
    return tiles


def determinism(root, pxl, manifest, samples, problems):
    by_id = {a['id']: a for a in manifest['assets']}
    picked = [s for s in samples if s in by_id] or [a['id'] for a in manifest['assets'][:4]]
    with tempfile.TemporaryDirectory(dir=root) as temp:
        for key in picked:
            a, out = by_id[key], Path(temp) / key
            subprocess.run([str(pxl), 'build', str(root / a['recipe']), '-o', str(out)], check=True, capture_output=True)
            png = out / 'check.png'
            subprocess.run([str(pxl), 'render', str(out / f'{key}.pxl.json'), '--asset', '-o', str(png)],
                           check=True, capture_output=True)
            if hashlib.sha256(png.read_bytes()).hexdigest() != a['sha256']:
                problems.append(f'{key}: clean rebuild hash differs')
    return picked


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    default_root = HERE.parent if (HERE / 'project.py').is_file() else None
    ap.add_argument('--project', type=Path, default=default_root, required=default_root is None)
    ap.add_argument('--pxl')
    ap.add_argument('--samples', default='', help='comma-separated ids to rebuild for determinism')
    args = ap.parse_args()
    root = args.project.resolve()
    pxl = find_pxl(args.pxl, root)
    manifest = json.loads((root / 'manifest.json').read_text())
    problems = []
    ids = [a['id'] for a in manifest['assets']]
    if not len(ids) == len(set(ids)) == manifest['count']:
        problems.append('manifest ids are not unique or count mismatches')
    for a in manifest['assets']:
        for field in ('png', 'doc', 'source', 'recipe'):
            if not (root / a[field]).is_file():
                problems.append(f'{a["id"]}: missing {a[field]}')
            elif field == 'png' and hashlib.sha256((root / a['png']).read_bytes()).hexdigest() != a['sha256']:
                problems.append(f'{a["id"]}: manifest sha256 mismatch')
    for rel in ['index.html', *manifest.get('atlases', []), *manifest.get('previews', [])]:
        if not (root / rel).is_file():
            problems.append(f'missing {rel}')
    for f in (root / 'tools').glob('*.py'):
        if len(f.read_text().splitlines()) > 500:
            problems.append(f'{f.name} exceeds 500 lines')
    tiles = pixel_checks(root, manifest, problems) if Image else None
    sampled = determinism(root, pxl, manifest, [s for s in args.samples.split(',') if s], problems)
    report = json.loads((root / 'validation.json').read_text())
    status = 'FAIL' if problems else 'PASS'
    report['export_integrity'] = dict(status=status, assets=len(ids), problems=problems,
                                      pixel_checks='PASS' if Image and not problems else
                                      ('NOT_ASSESSED: Pillow not installed' if not Image else status))
    report['tile_opposite_edges'] = dict(status='NOT_ASSESSED') if tiles is None else dict(status=status, tiles=tiles)
    report['determinism'] = dict(sample_assets=sampled, method='clean rebuild PNG SHA-256 equality')
    if report.get('status') == 'PASS' and problems:
        report['status'] = 'FAIL'
    dump(root / 'validation.json', report)
    print(json.dumps({'assets_checked': len(ids), 'problems': problems[:20], 'status': status}, ensure_ascii=False))
    sys.exit(1 if problems else 0)


if __name__ == '__main__':
    main()
