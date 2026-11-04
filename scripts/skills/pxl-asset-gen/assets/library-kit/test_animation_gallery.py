"""Regression checks for animation metadata, generated HTML and player controls."""
from html.parser import HTMLParser
import json
from pathlib import Path
import shutil
import subprocess
import unittest

import animation_gallery as motion


def frames():
    return [dict(id=f'walk_{i}', name=f'行走·{i}', sequence='walk', frame=i,
                 duration_ms=duration, canvas=[32, 32], source_rect=[2, 3, 16, 16],
                 png=f'assets/walk_{i}.png', direction='south')
            for i, duration in enumerate((180, 140, 140))]


class Scripts(HTMLParser):
    def __init__(self):
        super().__init__()
        self.types = []
        self.current = None
        self.data = []

    def handle_starttag(self, tag, attrs):
        if tag == 'script':
            self.current = dict(attrs).get('type', 'javascript')
            self.types.append(self.current)

    def handle_data(self, data):
        if self.current == 'application/json':
            self.data.append(data)

    def handle_endtag(self, tag):
        if tag == 'script':
            self.current = None


class AnimationTests(unittest.TestCase):
    def test_group_order_timing_and_static(self):
        result = motion.collect(list(reversed(frames())))
        self.assertEqual(result[0]['total_ms'], 460)
        self.assertEqual([f['id'] for f in result[0]['frames']], ['walk_0', 'walk_1', 'walk_2'])
        self.assertEqual(motion.render([]), '')
        self.assertEqual(motion.render(frames()[:1]), '')
        static = frames()
        for f in static:
            del f['sequence']
        self.assertEqual(motion.render(static), '')

    def test_bad_metadata_rejected(self):
        for key, value in [('frame', 0), ('frame', '1'), ('frame', True),
                           ('duration_ms', 0), ('duration_ms', -1), ('duration_ms', 1.5),
                           ('canvas', [64, 64]), ('direction', 'north'),
                           ('source_rect', [0, 0, 40, 40])]:
            with self.subTest(key=key, value=value):
                entries = frames()
                entries[1][key] = value
                with self.assertRaises(ValueError):
                    motion.collect(entries)
        with self.assertRaises(ValueError):
            motion.collect(frames()[1:])
        entries = frames()
        del entries[0]['duration_ms']
        with self.assertRaises(ValueError):
            motion.collect(entries)

    def test_embedded_json_escape_and_no_fetch(self):
        entries = frames()
        entries[0]['name'] = '</script><script>alert(1)</script>&'
        parser = Scripts()
        html = motion.render(entries)
        parser.feed(html)
        self.assertEqual(parser.types, ['application/json', 'javascript'])
        self.assertEqual(json.loads(''.join(parser.data))[0]['frames'][0]['name'], entries[0]['name'])
        self.assertNotIn('fetch(', html)

    @unittest.skipUnless(shutil.which('node'), 'node required for deterministic player tests')
    def test_player(self):
        script = r'''
const assert=require('node:assert/strict');
const vm=require('node:vm');
function boot(reduced=false,fail=false){
  class Element{
    constructor(){this.handlers={};this.style={};this.dataset={};this.attrs={};this.textContent='';this.classList={add(){}};}
    addEventListener(name,fn){this.handlers[name]=fn;}
    setAttribute(key,value){this.attrs[key]=value;}
    fire(name){this.handlers[name]({});}
  }
  const buttons=Array.from({length:3},()=>new Element());
  const sprite=new Element(),status=new Element(),next=new Element(),card=new Element();
  card.querySelector=s=>({'.motion-sprite':sprite,'.motion-status':status,'[data-motion-next]':next}[s]);
  card.querySelectorAll=()=>buttons;
  const toggle=new Element(),speed=new Element(),scale=new Element(),state=new Element();
  const root={querySelector:s=>({'[data-motion-json]':{textContent:JSON.stringify(SEQUENCES)},
    '[data-motion-toggle]':toggle,'[data-motion-speed]':speed,'[data-motion-scale]':scale,'[data-motion-state]':state}[s]),
    querySelectorAll:()=>[card]};
  const doc=new Element();doc.hidden=false;doc.querySelector=()=>root;
  const preference=new Element();preference.matches=reduced;
  let id=0;const queue=new Map();
  const context={document:doc,matchMedia:()=>preference,
    requestAnimationFrame:fn=>{queue.set(++id,fn);return id;},cancelAnimationFrame:n=>queue.delete(n),
    Image:class{set src(value){if(fail)this.onerror();else this.onload();}}};
  vm.runInNewContext(SOURCE,context);
  const advance=now=>{const tasks=[...queue.values()];queue.clear();tasks.forEach(fn=>fn(now));};
  return {card,sprite,status,next,buttons,toggle,speed,scale,state,doc,preference,queue,advance};
}
(async()=>{
  const p=boot();await new Promise(setImmediate);
  assert.equal(p.toggle.textContent,'暂停全部');
  assert.equal(p.sprite.style.backgroundPosition,'-8px -12px');
  assert.equal(p.sprite.style.backgroundSize,'128px 128px');
  assert.equal(p.sprite.style.width,'64px');
  p.advance(0);p.advance(179);assert.equal(p.card.dataset.frame,'0');
  p.advance(180);assert.equal(p.card.dataset.frame,'1');
  p.advance(320);assert.equal(p.card.dataset.frame,'2');
  p.advance(460);assert.equal(p.card.dataset.frame,'0');
  p.toggle.fire('click');assert.equal(p.queue.size,0);
  p.next.fire('click');assert.equal(p.card.dataset.frame,'1');
  p.buttons[2].fire('click');assert.equal(p.card.dataset.frame,'2');
  assert.equal(p.buttons[2].attrs['aria-pressed'],'true');
  p.scale.value='2';p.scale.fire('change');assert.equal(p.sprite.style.width,'32px');
  p.speed.value='2';p.speed.fire('change');p.toggle.fire('click');
  p.advance(1000);p.advance(1070);assert.equal(p.card.dataset.frame,'0');
  p.doc.hidden=true;p.doc.fire('visibilitychange');assert.equal(p.queue.size,0);
  p.doc.hidden=false;p.doc.fire('visibilitychange');p.advance(50000);assert.equal(p.card.dataset.frame,'0');
  p.preference.handlers.change({matches:true});assert.equal(p.queue.size,0);
  const reduced=boot(true);await new Promise(setImmediate);
  assert.equal(reduced.toggle.textContent,'播放全部');assert.equal(reduced.queue.size,0);
  reduced.next.fire('click');assert.equal(reduced.card.dataset.frame,'1');
  reduced.toggle.fire('click');assert.equal(reduced.queue.size,1);
  const broken=boot(false,true);await new Promise(setImmediate);
  assert.match(broken.status.textContent,/加载失败/);assert.equal(broken.queue.size,0);
  console.log('player timing, wraparound, pause, step, speed, crop, zoom, visibility, reduced motion, image failure: PASS');
})().catch(error=>{console.error(error);process.exitCode=1;});
'''
        prefix = 'const SOURCE=' + json.dumps(motion.JS) + ';const SEQUENCES=' + json.dumps(motion.collect(frames())) + ';'
        result = subprocess.run(['node'], input=prefix + script, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_stardew_sequences_when_available(self):
        manifest = Path(__file__).resolve().parents[5] / 'examples/stardew/manifest.json'
        if not manifest.exists():
            self.skipTest('Stardew reference not available in standalone pack')
        entries = json.loads(manifest.read_text())['assets']
        sequences = motion.collect(entries)
        self.assertEqual(len(sequences), 5)
        self.assertEqual(sorted(s['total_ms'] for s in sequences), [460, 460, 460, 460, 720])
        for seq in sequences:
            for f in seq['frames']:
                self.assertTrue((manifest.parent / f['png']).is_file())


if __name__ == '__main__':
    unittest.main()
