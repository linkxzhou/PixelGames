# @pxl/runtime API

Import from the generated `index.js` without a bundler.

```js
import { run, loadPackage } from "./index.js";
```

`run(game, { title, width, height, scale, canvas?, kernel? })` returns a promise resolved after `game.init`. Use `scale: "integer-fit"` or a positive integer. The default canvas id is `glcanvas` and default kernel is `pxl_web.wasm`.

A game implements:

- `init(ctx)`: optionally async. Load assets and call `ctx.loadPackage(package)` to upload sheet textures.
- `update(ctx, input)`: one fixed 60 Hz tick. Use `input.pressed(key)`, `down(key)`, and `released(key)` with names such as `ArrowUp`, `Space`, `KeyW`, and `Digit1`.
- `draw(g)`: once per render frame.

Graphics methods are `clear(0xRRGGBBAA)`, `sprite({ sheet, frame, x, y, w?, h?, flipX?, flipY?, tint? })`, `rect({x,y,w,h}, rgba)`, and `camera(x,y)`.

`loadPackage("assets/")` loads `asset-package.json`, palette, sheet JSON/PNG, and `levels/index.json`. A package provides `sheetFor(assetId)`, `anim(assetId, anim, dir, fps, time)`, and `levels`. Sheet handles support `frame(nameOrIndex)`, `animFrames(sprite, anim, dir)`, and `firstFrame`.

Other exports include `Palette`, `Level`, `FixedStep`, `Input`, `CommandBuffer`, `loadSave`, `writeSave`, `AudioPlayer`, and `NullAudioPlayer`. Type declarations are in `index.d.ts`.

Audio: call `await ctx.audio.preload(package, "assets/")` during `init`, then unlock through the page's sound button. Sim code emits `{tick, eventId, audioId, kind}` and the player consumes it without affecting simulation. Use `playSfx`, `playBgm`, `stopSfx`, `stopBgm`, volume/mute methods, `release`, and `destroy`. SFX concurrency is 16, BGM crossfade briefly uses two voices, decoded cache is 32 MiB, and loop frames are converted with the source sample rate. `NullAudioPlayer.events` records deterministic headless sequences.

Hard constraints: simulation never touches the DOM or wall clock; updates use fixed ticks; rendering uses nearest-neighbor integer scaling; palette size is at most 62; one command buffer is submitted per frame rather than one wasm call per sprite.
