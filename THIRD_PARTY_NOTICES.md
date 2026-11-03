# Third-party notices

`pxl` reimplements published pixel algorithms and separately vendors the audio synthesizers listed below.

## zfedoran/pixel-sprite-generator

MIT License. Copyright (c) Zachary Fedoran and contributors.

The Bollinger mask pipeline in `pxl-gen` (sample body, mirror, mark 4-neighbor edges, color from a ramp) follows the behavior described by that project: mask values for empty, 50% body, and body-or-border, then a mirrored colored sprite. The Rust code, the integer color ramp, and mask values 3–5 are original. https://github.com/zfedoran/pixel-sprite-generator

## MaartenGr/Sprite-Generator

No license is stated on that repository. Its code was not read into this tree and was not copied. Do not add it.

## Pixelorama

MIT License. Copyright (c) Pixelorama contributors.

`paint_outline` takes the outer/inner idea and the square, diamond, and circle brushes from Pixelorama's outline tool and reimplements them with integer distances. https://github.com/Orama-Interactive/Pixelorama

## Piskel

Apache License 2.0. Copyright (c) Piskel contributors.

`flip`, `rotate90`, and `shift` match the pixel placement of Piskel's `TransformUtils.js` (flip axis names are swapped to match ordinary x/y, rotation pads to a square and keeps the canvas size, a shift samples `src(x-dx, y-dy)`). The Rust functions were written from that behavior, not transcribed from the JavaScript. https://github.com/piskelapp/piskel

## ZzFX 1.4.0

MIT License. Copyright (c) Frank Force and contributors.

The offline SFX adapter uses the vendored `ZzFX.js`; its license and npm integrity are recorded beside it in `audio-adapter/vendor/`. https://github.com/KilledByAPixel/ZzFX

## BeepBox 4.2.0

MIT License. Copyright (c) John Nesky and contributors.

The offline BGM adapter uses the vendored BeepBox synth modules for the frozen JSON subset. `LICENSE.md`, source provenance and npm integrity are recorded beside them. No BeepBox code is shipped in browser game bundles. https://github.com/johnnesky/beepbox

## Battle City UI font

No font binary is shipped in this repository. When present, the native runtime
opens the macOS system file `/System/Library/Fonts/Supplemental/Arial Unicode.ttf`
at run time to draw Chinese labels; that file is part of macOS and is not
redistributed here. Without it the game falls back to macroquad's ASCII font and
English strings. A bundled OFL pixel font can be dropped at
`games/battle-city/runtime/assets/fonts/game.ttf` (same licence terms would need
to be added here).

No GPL or AGPL sources are included.
