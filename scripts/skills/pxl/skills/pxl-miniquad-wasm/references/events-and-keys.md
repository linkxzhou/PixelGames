# Events and keys

## Contents

- Event delivery
- Key and modifier mapping
- Mouse and touch

`gl.js` calls wasm exports, which dispatch `EventHandler`. See `rust-api.md` for the complete callback signatures.

## Key and modifier mapping

Modifier bits are Shift=1, Ctrl=2, Alt=4, Super/logo=8. `KeyMods` exposes `shift`, `ctrl`, `alt`, and `logo` booleans. Unknown sapp key codes become `KeyCode::Unknown`.

| sapp code | KeyCode | Rust value |
|---:|---|---:|
| 32 | `Space` | `0x0020` |
| 39 | `Apostrophe` | `0x0027` |
| 44 | `Comma` | `0x002c` |
| 45 | `Minus` | `0x002d` |
| 46 | `Period` | `0x002e` |
| 47 | `Slash` | `0x002f` |
| 48 | `Key0` | `0x0030` |
| 49 | `Key1` | `0x0031` |
| 50 | `Key2` | `0x0032` |
| 51 | `Key3` | `0x0033` |
| 52 | `Key4` | `0x0034` |
| 53 | `Key5` | `0x0035` |
| 54 | `Key6` | `0x0036` |
| 55 | `Key7` | `0x0037` |
| 56 | `Key8` | `0x0038` |
| 57 | `Key9` | `0x0039` |
| 59 | `Semicolon` | `0x003b` |
| 61 | `Equal` | `0x003d` |
| 65 | `A` | `0x0041` |
| 66 | `B` | `0x0042` |
| 67 | `C` | `0x0043` |
| 68 | `D` | `0x0044` |
| 69 | `E` | `0x0045` |
| 70 | `F` | `0x0046` |
| 71 | `G` | `0x0047` |
| 72 | `H` | `0x0048` |
| 73 | `I` | `0x0049` |
| 74 | `J` | `0x004a` |
| 75 | `K` | `0x004b` |
| 76 | `L` | `0x004c` |
| 77 | `M` | `0x004d` |
| 78 | `N` | `0x004e` |
| 79 | `O` | `0x004f` |
| 80 | `P` | `0x0050` |
| 81 | `Q` | `0x0051` |
| 82 | `R` | `0x0052` |
| 83 | `S` | `0x0053` |
| 84 | `T` | `0x0054` |
| 85 | `U` | `0x0055` |
| 86 | `V` | `0x0056` |
| 87 | `W` | `0x0057` |
| 88 | `X` | `0x0058` |
| 89 | `Y` | `0x0059` |
| 90 | `Z` | `0x005a` |
| 91 | `LeftBracket` | `0x005b` |
| 92 | `Backslash` | `0x005c` |
| 93 | `RightBracket` | `0x005d` |
| 96 | `Apostrophe` | `0x0027` |
| 256 | `Escape` | `0xff1b` |
| 257 | `Enter` | `0xff0d` |
| 258 | `Tab` | `0xff09` |
| 259 | `Backspace` | `0xff08` |
| 260 | `Insert` | `0xff63` |
| 261 | `Delete` | `0xffff` |
| 262 | `Right` | `0xff53` |
| 263 | `Left` | `0xff51` |
| 264 | `Down` | `0xff54` |
| 265 | `Up` | `0xff52` |
| 266 | `PageUp` | `0xff55` |
| 267 | `PageDown` | `0xff56` |
| 268 | `Home` | `0xff50` |
| 269 | `End` | `0xff57` |
| 280 | `CapsLock` | `0xffe5` |
| 281 | `ScrollLock` | `0xff14` |
| 282 | `NumLock` | `0xff7f` |
| 283 | `PrintScreen` | `0xfd1d` |
| 284 | `Pause` | `0xff13` |
| 290 | `F1` | `0xffbe` |
| 291 | `F2` | `0xffbf` |
| 292 | `F3` | `0xffc0` |
| 293 | `F4` | `0xffc1` |
| 294 | `F5` | `0xffc2` |
| 295 | `F6` | `0xffc3` |
| 296 | `F7` | `0xffc4` |
| 297 | `F8` | `0xffc5` |
| 298 | `F9` | `0xffc6` |
| 299 | `F10` | `0xffc7` |
| 300 | `F11` | `0xffc8` |
| 301 | `F12` | `0xffc9` |
| 302 | `F13` | `0xffca` |
| 303 | `F14` | `0xffcb` |
| 304 | `F15` | `0xffcc` |
| 305 | `F16` | `0xffcd` |
| 306 | `F17` | `0xffce` |
| 307 | `F18` | `0xffcf` |
| 308 | `F19` | `0xffd0` |
| 309 | `F20` | `0xffd1` |
| 310 | `F21` | `0xffd2` |
| 311 | `F22` | `0xffd3` |
| 312 | `F23` | `0xffd4` |
| 313 | `F24` | `0xffd5` |
| 320 | `Kp0` | `0xffb0` |
| 321 | `Kp1` | `0xffb1` |
| 322 | `Kp2` | `0xffb2` |
| 323 | `Kp3` | `0xffb3` |
| 324 | `Kp4` | `0xffb4` |
| 325 | `Kp5` | `0xffb5` |
| 326 | `Kp6` | `0xffb6` |
| 327 | `Kp7` | `0xffb7` |
| 328 | `Kp8` | `0xffb8` |
| 329 | `Kp9` | `0xffb9` |
| 330 | `KpDecimal` | `0xffae` |
| 331 | `KpDivide` | `0xffaf` |
| 332 | `KpMultiply` | `0xffaa` |
| 333 | `KpSubtract` | `0xffad` |
| 334 | `KpAdd` | `0xffab` |
| 335 | `KpEnter` | `0xff8d` |
| 336 | `KpEqual` | `0xffbd` |
| 340 | `LeftShift` | `0xffe1` |
| 341 | `LeftControl` | `0xffe3` |
| 342 | `LeftAlt` | `0xffe9` |
| 343 | `LeftSuper` | `0xffeb` |
| 344 | `RightShift` | `0xffe2` |
| 345 | `RightControl` | `0xffe4` |
| 346 | `RightAlt` | `0xffea` |
| 347 | `RightSuper` | `0xffec` |
| 348 | `Menu` | `0xff67` |

## Mouse and touch

Mouse buttons are `Left=0`, `Middle=1`, `Right=2`, and `Unknown=255` in Rust; the browser mapping translates sapp 0/1/2 to Left/Right/Middle. Wheel deltas arrive as `mouse_wheel_event(x, y)`.

Touch phases are sapp 10 Started, 11 Moved, 12 Ended, and 13 Cancelled. The default `touch_event` implementation emulates left-button presses and mouse motion. Focus loss/restoration maps to `window_minimized_event`/`window_restored_event`.
