export const KEY_CODES = Object.freeze({
  Space: 0x20, Enter: 0xff0d, Escape: 0xff1b,
  ArrowRight: 0xff53, ArrowLeft: 0xff51, ArrowDown: 0xff54, ArrowUp: 0xff52,
  Digit0: 0x30, Digit1: 0x31, Digit2: 0x32, Digit3: 0x33,
  Digit4: 0x34, Digit5: 0x35, Digit6: 0x36, Digit7: 0x37,
  Digit8: 0x38, Digit9: 0x39,
  KeyA: 0x41, KeyB: 0x42, KeyC: 0x43, KeyD: 0x44, KeyE: 0x45,
  KeyF: 0x46, KeyG: 0x47, KeyH: 0x48, KeyI: 0x49, KeyJ: 0x4a,
  KeyK: 0x4b, KeyL: 0x4c, KeyM: 0x4d, KeyN: 0x4e, KeyO: 0x4f,
  KeyP: 0x50, KeyQ: 0x51, KeyR: 0x52, KeyS: 0x53, KeyT: 0x54,
  KeyU: 0x55, KeyV: 0x56, KeyW: 0x57, KeyX: 0x58, KeyY: 0x59, KeyZ: 0x5a,
});

const aliases = new Map([
  ["up", "ArrowUp"], ["down", "ArrowDown"], ["left", "ArrowLeft"], ["right", "ArrowRight"],
  ["fire", "Space"], ["space", "Space"], ["enter", "Enter"], ["escape", "Escape"],
]);

export function keyCode(key) {
  const name = aliases.get(key) ?? key;
  if (KEY_CODES[name] !== undefined) return KEY_CODES[name];
  if (/^[a-z]$/i.test(key)) return key.toUpperCase().charCodeAt(0);
  if (/^[0-9]$/.test(key)) return key.charCodeAt(0);
  throw new Error(`unknown key: ${key}`);
}

export class Input {
  constructor(wasm) {
    this.wasm = wasm;
    this.downCodes = new Set();
    this.pressedCodes = new Set();
    this.releasedCodes = new Set();
  }

  poll() {
    const count = this.wasm.pxl_input_len();
    const pointer = this.wasm.pxl_input_ptr();
    const view = new DataView(this.wasm.memory.buffer);
    for (let index = 0; index < count; index += 1) {
      const at = pointer + index * 16;
      const kind = view.getUint32(at, true);
      const code = view.getUint32(at + 4, true);
      if (kind === 1) {
        if (!this.downCodes.has(code)) this.pressedCodes.add(code);
        this.downCodes.add(code);
      } else if (kind === 2) {
        this.downCodes.delete(code);
        this.releasedCodes.add(code);
      }
    }
    this.wasm.pxl_input_clear();
  }

  pressed(key) { return this.pressedCodes.has(keyCode(key)); }
  down(key) { return this.downCodes.has(keyCode(key)); }
  released(key) { return this.releasedCodes.has(keyCode(key)); }

  endTick() {
    this.pressedCodes.clear();
    this.releasedCodes.clear();
  }

  reset() {
    this.downCodes.clear();
    this.endTick();
  }
}
