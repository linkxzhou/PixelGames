"""Write <library>/index.html: a static, file://-safe browser for every asset."""
from collections import defaultdict
from html import escape

THEME = dict(bg='#1c2926', panel='#2b3b32', line='#415044', paper='#ede1c5',
             muted='#a6b5a1', accent='#e4bc57', check_a='#26352e', check_b='#2b3b33')
THUMB = 128

CSS = '''
:root{color-scheme:dark;--bg:%(bg)s;--panel:%(panel)s;--line:%(line)s;--paper:%(paper)s;--muted:%(muted)s;--accent:%(accent)s;--ca:%(check_a)s;--cb:%(check_b)s}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--paper);font:15px system-ui,sans-serif}
header,main{max-width:1250px;margin:auto;padding:32px}header{padding-top:54px}h1{font-size:42px;margin:10px 0}
h2{font-size:22px;margin:40px 0 14px;scroll-margin-top:90px}h2 span{color:var(--muted);font-weight:400}p{color:var(--muted);line-height:1.8}
.eyebrow{color:var(--accent);letter-spacing:3px;font-size:12px}.hero{display:grid;grid-template-columns:1fr 360px;gap:40px;align-items:center}
.pix{image-rendering:pixelated;image-rendering:crisp-edges}.scene{width:100%%;border:8px solid var(--line);border-radius:8px}
.badge{display:inline-block;padding:7px 12px;margin:0 6px 8px 0;background:var(--panel);border-radius:5px;color:var(--accent)}
.bar{position:sticky;top:0;z-index:3;background:var(--bg);padding:12px 0;border-bottom:1px solid var(--line)}
input{width:100%%;padding:14px;border:1px solid var(--line);border-radius:7px;background:var(--panel);color:var(--paper);font:inherit}
nav{display:flex;flex-wrap:wrap;gap:6px;margin-top:10px}nav a{padding:4px 10px;border:1px solid var(--line);border-radius:999px;text-decoration:none;font-size:12px}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(164px,1fr));gap:12px}
figure{margin:0;padding:14px;background:var(--panel);border:1px solid var(--line);border-radius:7px;text-align:center;cursor:zoom-in}
figure:hover{border-color:var(--accent)}.chk{background:repeating-conic-gradient(var(--ca) 0%% 25%%,var(--cb) 0%% 50%%) 0/16px 16px}
.thumb{width:%(thumb)spx;height:%(thumb)spx;margin:auto;display:flex;align-items:center;justify-content:center}
.thumb span{display:block;background-repeat:no-repeat}figcaption{margin-top:8px;font-size:13px}
small{display:block;overflow-wrap:anywhere;font-size:10px;color:var(--muted);margin:5px 0}a{color:var(--accent);font-size:11px}
.sw{display:flex;flex-wrap:wrap;gap:4px}.sw i{width:26px;height:26px;border-radius:4px;border:1px solid var(--line)}
.boards{display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:12px}.boards figure img{width:100%%;height:auto}
dialog{background:var(--panel);color:var(--paper);border:1px solid var(--line);border-radius:10px;max-width:min(92vw,720px)}
dialog img{width:min(80vw,512px);height:auto;max-height:70vh;object-fit:contain}dialog::backdrop{background:#000a}
footer{padding:40px 0;color:var(--muted)}[hidden]{display:none!important}
@media(max-width:700px){.hero{grid-template-columns:1fr}header,main{padding:20px}h1{font-size:32px}}
'''

JS = '''
const q=document.querySelector('#search'),dlg=document.querySelector('dialog');
q.addEventListener('input',()=>{const s=q.value.trim().toLowerCase();
document.querySelectorAll('figure[data-id]').forEach(f=>f.hidden=s&&!f.dataset.k.includes(s));
document.querySelectorAll('section[data-category]').forEach(c=>c.hidden=![...c.querySelectorAll('figure[data-id]')].some(f=>!f.hidden))});
document.querySelectorAll('figure[data-src]').forEach(f=>f.addEventListener('click',e=>{if(e.target.tagName==='A')return;
dlg.querySelector('img').src=f.dataset.src;dlg.querySelector('div').innerHTML=f.querySelector('figcaption').innerHTML;dlg.showModal()}));
dlg.addEventListener('click',e=>{if(e.target===dlg)dlg.close()});
'''


def _thumb(a):
    """Crop the canvas to source_rect with an integer-scaled CSS background."""
    cw, ch = a['canvas']
    x, y, w, h = a['source_rect']
    s = max(1, THUMB // max(w, h))
    style = (f'width:{w * s}px;height:{h * s}px;background-image:url({escape(a["png"])});'
             f'background-size:{cw * s}px {ch * s}px;background-position:-{x * s}px -{y * s}px')
    return f'<div class="thumb chk"><span class="pix" role="img" aria-label="{escape(a["name"])}" style="{style}"></span></div>'


def _card(a):
    links = ' · '.join(f'<a href="{escape(href)}">{label}</a>' for label, href in
                       (('PNG', a['png']), ('ASCII', a['source']), ('Recipe', a['recipe']), ('Doc', a['doc'])))
    w, h = a['canvas']
    rect = a['source_rect']
    keys = ' '.join([a['id'], a['name'], a['category'], *map(str, a.get('tags', []))]).lower()
    return (f'<figure data-id="{escape(a["id"])}" data-k="{escape(keys)}" data-src="{escape(a["png"])}">{_thumb(a)}'
            f'<figcaption>{escape(a["name"])}<small>{escape(a["id"])}</small>'
            f'<small>{w}×{h} · rect {rect[2]}×{rect[3]}@{rect[0]},{rect[1]}{" · tile" if a.get("tile") else ""}</small>'
            f'{links}</figcaption></figure>')


def write(root, config, entries, boards, atlases, hero):
    theme = {**THEME, **config.get('theme', {}), 'thumb': THUMB}
    groups = defaultdict(list)
    for a in entries:
        groups[a['category']].append(a)
    title = escape(config.get('title', config['id']))
    out = [f'<!doctype html><html lang="{config.get("lang", "zh-CN")}"><meta charset="utf-8">'
           '<meta name="viewport" content="width=device-width,initial-scale=1"><link rel="icon" href="data:,">'
           f'<title>{title}</title><style>{CSS % theme}</style><header><div class="hero"><div>'
           f'<div class="eyebrow">{escape(config.get("eyebrow", "PIXELGAMES / ASSET LIBRARY"))}</div>'
           f'<h1>{title}</h1><p>{escape(config.get("subtitle", ""))}</p>'
           f'<div class="badge">{len(entries)} 个素材</div><div class="badge">{len(groups)} 个分类</div>'
           f'<div class="badge">{len(config["palette"]["colors"]) - 1} 色调色板</div>'
           '<div class="badge">ASCII → pxl-asset-gen</div>']
    if config.get('note'):
        out.append(f'<p>{escape(config["note"])}</p>')
    out.append('<div class="sw">' + ''.join(
        f'<i title="{i}: {c}" style="background:{c[:7]}"></i>' for i, c in enumerate(config['palette']['colors']) if i) + '</div></div>')
    if hero:
        out.append(f'<img class="scene pix" src="{escape(hero)}" alt="组合场景">')
    out.append('</div></header><main><div class="bar"><input id="search" aria-label="搜索素材" '
               'placeholder="搜索名称 / ID / 分类"><nav>')
    out.extend(f'<a href="#cat-{escape(c)}">{escape(c)} {len(g)}</a>' for c, g in groups.items())
    out.append('<a href="#atlases">atlases</a><a href="#boards">boards</a></nav></div>')
    for category, group in groups.items():
        out.append(f'<section data-category="{escape(category)}"><h2 id="cat-{escape(category)}">'
                   f'{escape(category)} <span>{len(group)}</span></h2><div class="grid">'
                   + ''.join(_card(a) for a in group) + '</div></section>')
    out.append('<section><h2 id="atlases">Atlases <span>sheet --layout grid</span></h2><div class="boards">')
    out.extend(f'<figure data-src="{escape(p)}"><img class="pix chk" loading="lazy" src="{escape(p)}" alt="{escape(p)}">'
               f'<figcaption>{escape(p)}<small><a href="{escape(p)}">PNG</a> · <a href="{escape(j)}">JSON</a></small>'
               '</figcaption></figure>' for p, j in atlases)
    out.append('</div></section><section><h2 id="boards">Preview boards <span>scale 3</span></h2><div class="boards">')
    out.extend(f'<figure data-src="{escape(p)}"><img class="pix" loading="lazy" src="{escape(p)}" alt="{escape(p)}">'
               f'<figcaption>{escape(p)}<small><a href="{escape(i)}">slot index</a></small></figcaption></figure>'
               for p, i in boards)
    out.append('</div></section>')
    seed = config.get('seed', 1)
    out.append(f'<footer>确定性种子 {seed} · <a href="manifest.json">manifest.json</a> · '
               '<a href="validation.json">validation.json</a> · <a href="palette.json">palette.json</a>'
               f' · {escape(config.get("license_note", ""))}</footer></main>'
               '<dialog><img class="pix chk" alt=""><div></div></dialog>'
               f'<script>{JS}</script></html>')
    (root / 'index.html').write_text(''.join(out))
    return root / 'index.html'
