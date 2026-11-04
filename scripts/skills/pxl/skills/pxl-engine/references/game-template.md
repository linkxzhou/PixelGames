# Minimal game template

The build converts `main.js` to `game.js` and supplies `index.html`:

```html
<canvas id="glcanvas" tabindex="0"></canvas>
<script src="mq_js_bundle.js"></script>
<script type="module" src="game.js"></script>
```

`main.js`:

```js
import { run } from "./index.js";
import { Sim } from "./sim.js";

const game = {
  init(ctx) { this.sim = new Sim(); },
  update(_ctx, input) { this.sim.step({ right: input.down("ArrowRight") }); },
  draw(g) {
    g.clear(0x101018ff);
    g.rect({ x: this.sim.x, y: 16, w: 32, h: 32 }, 0xe8c040ff);
  },
};
run(game, { title: "Example", width: 320, height: 240, scale: "integer-fit" });
```

`sim.js` exports plain data and a fixed-step method. Do not import DOM APIs, call `Date.now`, read `performance.now`, fetch files, or use frame-rate-dependent movement in it. This keeps the same module runnable under Node headless replay.

Put a replay in `replay/playtest.json` and a Node runner in `headless.js`; battle-city provides the reference report and PNG capture.
