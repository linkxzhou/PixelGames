"""File-safe animation showcase; consume authored manifest timing, not atlas idle timing."""
from collections import defaultdict
from html import escape
import json

CSS = '''
#animations{margin:32px 0;padding:24px;border:1px solid var(--accent);border-radius:12px;background:#24372f}
#animations h2{margin:8px 0}#animations .motion-toolbar{display:flex;flex-wrap:wrap;gap:12px;align-items:center;margin:20px 0}
#animations button,#animations select{font:inherit;color:var(--paper);background:#33443a;border:1px solid #64735d;border-radius:6px;padding:8px 12px;cursor:pointer}
#animations button:focus-visible,#animations select:focus-visible{outline:2px solid var(--accent);outline-offset:3px}
#animations button:disabled{opacity:.5;cursor:default}#animations button[aria-pressed=true]{border-color:var(--accent);color:var(--accent)}
#animations .motion-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,240px),1fr));gap:16px}
#animations article{min-width:0;background:#1c2926;border:1px solid #415044;border-radius:8px;padding:16px}
#animations h3{font-size:16px;margin:0 0 4px}#animations small{display:block;overflow-wrap:anywhere;color:var(--muted)}
#animations .motion-stage{height:176px;overflow:auto;display:grid;place-items:center;margin:16px 0;background:repeating-conic-gradient(#26352e 0% 25%,#2b3b33 0% 50%) 0/16px 16px}
#animations .motion-sprite{display:block;flex:none;background-repeat:no-repeat;image-rendering:pixelated;image-rendering:crisp-edges}
#animations .motion-frames{display:flex;flex-wrap:wrap;gap:6px;margin:12px 0}#animations .motion-frames button{font-size:12px;padding:6px 9px}
#animations .motion-status{font-size:12px;color:var(--muted);min-height:1.5em}#animations .motion-error{color:#ffb6a3}
@media(max-width:700px){#animations{padding:16px}}
'''

JS = '''
(()=>{
const root=document.querySelector('#animations');
if(!root)return;
const sequences=JSON.parse(root.querySelector('[data-motion-json]').textContent);
const preference=matchMedia('(prefers-reduced-motion: reduce)');
const toggle=root.querySelector('[data-motion-toggle]');
const speed=root.querySelector('[data-motion-speed]');
const scale=root.querySelector('[data-motion-scale]');
let playing=!preference.matches,rate=1,zoom=4,raf=null,last=null;
const states=sequences.map((seq,i)=>{
  const card=root.querySelectorAll('[data-motion-card]')[i];
  return {seq,card,sprite:card.querySelector('.motion-sprite'),status:card.querySelector('.motion-status'),
    buttons:[...card.querySelectorAll('[data-frame]')],index:0,elapsed:0,ready:false,failed:false};
});
function paint(s){
  const f=s.seq.frames[s.index],[x,y,w,h]=f.source_rect,[cw,ch]=f.canvas;
  Object.assign(s.sprite.style,{width:`${w*zoom}px`,height:`${h*zoom}px`,
    backgroundImage:`url(${JSON.stringify(f.png)})`,backgroundSize:`${cw*zoom}px ${ch*zoom}px`,
    backgroundPosition:`${-x*zoom}px ${-y*zoom}px`});
  s.sprite.setAttribute('aria-label',f.name);
  s.card.dataset.frame=String(s.index);
  s.status.textContent=s.failed?'帧图片加载失败，请检查素材路径':!s.ready?'正在加载帧图片…':
    `帧 ${s.index+1} / ${s.seq.frames.length} · ${f.duration_ms} ms · ${f.id}`;
  s.buttons.forEach((b,i)=>b.setAttribute('aria-pressed',String(i===s.index)));
}
function sync(){
  toggle.textContent=playing?'暂停全部':'播放全部';
  toggle.setAttribute('aria-pressed',String(playing));
  root.querySelector('[data-motion-state]').textContent=playing?'按原始帧时长循环播放':'已暂停 · 可点击帧号检查姿势';
}
function tick(now){
  raf=null;
  const delta=last===null?0:now-last;last=now;
  for(const s of states){
    if(!s.ready||s.failed)continue;
    s.elapsed=(s.elapsed+delta*rate)%s.seq.total_ms;
    const previous=s.index;
    while(s.elapsed>=s.seq.frames[s.index].duration_ms){
      s.elapsed-=s.seq.frames[s.index].duration_ms;s.index=(s.index+1)%s.seq.frames.length;
    }
    if(previous!==s.index)paint(s);
  }
  schedule();
}
function schedule(){
  if(playing&&!document.hidden&&states.some(s=>s.ready&&!s.failed)&&raf===null)raf=requestAnimationFrame(tick);
}
function stopClock(){if(raf!==null)cancelAnimationFrame(raf);raf=null;last=null;}
function setPlaying(value){stopClock();playing=value;sync();schedule();}
toggle.addEventListener('click',()=>setPlaying(!playing));
speed.addEventListener('change',()=>{rate=Number(speed.value);last=null;});
scale.addEventListener('change',()=>{zoom=Number(scale.value);states.forEach(paint);});
document.addEventListener('visibilitychange',()=>{stopClock();schedule();});
preference.addEventListener('change',e=>{if(e.matches)setPlaying(false);});
for(const s of states){
  function select(index){setPlaying(false);s.index=index;s.elapsed=0;paint(s);}
  s.buttons.forEach((b,i)=>b.addEventListener('click',()=>select(i)));
  s.card.querySelector('[data-motion-next]').addEventListener('click',()=>select((s.index+1)%s.seq.frames.length));
  paint(s);
  Promise.all(s.seq.frames.map(f=>new Promise((resolve,reject)=>{
    const image=new Image();image.onload=resolve;image.onerror=reject;image.src=f.png;
  }))).then(()=>{s.ready=true;paint(s);schedule();}).catch(()=>{
    s.failed=true;s.status.classList.add('motion-error');paint(s);
  });
}
sync();
})();
'''


def collect(entries):
    """Validate explicit sequences; single frames remain static (including smoke builds)."""
    groups = defaultdict(list)
    for entry in entries:
        sequence = entry.get('sequence')
        if sequence is None:
            continue
        if not isinstance(sequence, str) or not sequence.strip():
            raise ValueError(f'{entry["id"]}: sequence must be a non-empty string')
        for key in ('frame', 'duration_ms'):
            value = entry.get(key)
            if type(value) is not int or value < (1 if key == 'duration_ms' else 0):
                raise ValueError(f'{entry["id"]}: {key} must be a valid integer')
        groups[sequence].append(entry)
    result = []
    for sequence, frames in groups.items():
        frames.sort(key=lambda a: a['frame'])
        if [f['frame'] for f in frames] != list(range(len(frames))):
            raise ValueError(f'{sequence}: frame indices must be unique and contiguous from zero')
        first = frames[0]
        for f in frames:
            if any(f.get(k) != first.get(k) for k in ('canvas', 'source_rect', 'direction')):
                raise ValueError(f'{sequence}: canvas, source_rect and direction must match')
            cw, ch = f['canvas']
            x, y, w, h = f['source_rect']
            if min(cw, ch, w, h) <= 0 or min(x, y) < 0 or x+w > cw or y+h > ch:
                raise ValueError(f'{sequence}: invalid source_rect')
        if len(frames) < 2:
            continue
        result.append(dict(id=sequence, total_ms=sum(f['duration_ms'] for f in frames),
                           frames=[{k: f[k] for k in ('id', 'name', 'png', 'canvas', 'source_rect', 'duration_ms')}
                                   for f in frames]))
    return result


def render(entries):
    """Return self-contained HTML/CSS/JS; no fetch, dependency or image rewriting."""
    sequences = collect(entries)
    if not sequences:
        return ''
    out = [f'<style>{CSS}</style><section id="animations" aria-labelledby="motion-title">'
           '<div class="eyebrow">MOTION STUDIES / 逐帧动效</div>'
           f'<h2 id="motion-title">让像素动起来 <span>{len(sequences)} 组动画</span></h2>'
           '<p>直接播放素材原始帧，保留每帧节奏；不使用平滑插值或 CSS 位移伪造动画。'
           '开启系统“减少动态效果”时默认暂停。</p>'
           '<div class="motion-toolbar"><button type="button" data-motion-toggle>播放全部</button>'
           '<label>速度 <select data-motion-speed><option value="0.5">0.5×</option>'
           '<option value="1" selected>1×</option><option value="2">2×</option></select></label>'
           '<label>像素缩放 <select data-motion-scale><option value="1">1×</option>'
           '<option value="2">2×</option><option value="4" selected>4×</option>'
           '<option value="6">6×</option></select></label>'
           '<span data-motion-state role="status"></span></div><div class="motion-grid">']
    for seq in sequences:
        first = seq['frames'][0]
        title = first['name'].rsplit('·', 1)[0]
        out.append(f'<article data-motion-card><h3>{escape(title)}</h3><small>{escape(seq["id"])}</small>'
                   f'<small>{len(seq["frames"])} 帧 · 循环 {seq["total_ms"]} ms</small>'
                   '<div class="motion-stage"><span class="motion-sprite" role="img"></span></div>'
                   '<div class="motion-status"></div><div class="motion-frames" aria-label="选择动画帧">')
        out.extend(f'<button type="button" data-frame="{i}" aria-label="第 {i+1} 帧，{f["duration_ms"]} 毫秒">'
                   f'{i+1} · {f["duration_ms"]}ms</button>' for i, f in enumerate(seq['frames']))
        out.append('</div><button type="button" data-motion-next>下一帧</button></article>')
    payload = json.dumps(sequences, ensure_ascii=False).replace('<', '\\u003c').replace('&', '\\u0026')
    out.append('</div><noscript><p>动效播放需要启用 JavaScript；下方静态素材仍可浏览。</p></noscript>'
               f'<script type="application/json" data-motion-json>{payload}</script></section><script>{JS}</script>')
    return ''.join(out)
