export const COMMAND_WORDS = 16;
export const COMMAND_BYTES = COMMAND_WORDS * 4;

export class CommandBuffer {
  constructor(wasm, capacity = 16384) {
    this.wasm = wasm;
    this.capacity = capacity;
    this.pointer = wasm.pxl_alloc(capacity * COMMAND_BYTES);
    wasm.pxl_set_commands(this.pointer, capacity);
    this.count = 0;
    this.view = new DataView(wasm.memory.buffer);
  }

  refresh() {
    if (this.view.buffer !== this.wasm.memory.buffer) {
      this.view = new DataView(this.wasm.memory.buffer);
    }
  }

  reset() {
    this.count = 0;
  }

  offset() {
    if (this.count >= this.capacity) throw new Error("command buffer capacity exceeded");
    return this.pointer + this.count++ * COMMAND_BYTES;
  }

  u32(offset, value) {
    this.view.setUint32(offset, value >>> 0, true);
  }

  f32(offset, value) {
    this.view.setFloat32(offset, Math.fround(value), true);
  }

  clear(rgba) {
    this.refresh();
    const at = this.offset();
    this.view.setUint32(at, 0, true);
    this.u32(at, 1);
    this.u32(at + 4, rgba);
  }

  rect(x, y, w, h, rgba) {
    this.refresh();
    const at = this.offset();
    this.view.setUint32(at, 0, true);
    this.u32(at, 2);
    [x, y, w, h].forEach((value, index) => this.f32(at + 4 + index * 4, value));
    this.u32(at + 20, rgba);
  }

  sprite(texture, src, dst, flipX, flipY, tint) {
    this.refresh();
    const at = this.offset();
    this.view.setUint32(at, 0, true);
    this.u32(at, 3);
    this.u32(at + 4, texture);
    [...src, ...dst].forEach((value, index) => this.f32(at + 8 + index * 4, value));
    this.u32(at + 40, (flipX ? 1 : 0) | (flipY ? 2 : 0));
    this.u32(at + 44, tint);
  }

  camera(x, y) {
    this.refresh();
    const at = this.offset();
    this.view.setUint32(at, 0, true);
    this.u32(at, 4);
    this.f32(at + 4, x);
    this.f32(at + 8, y);
  }

  light(x, y, radius, rgba, levels = 4) {
    if (!Number.isInteger(levels) || levels < 1 || levels > 8) {
      throw new RangeError("light levels must be in 1..=8");
    }
    this.refresh();
    const at = this.offset();
    this.view.setUint32(at, 5, true);
    this.f32(at + 4, x);
    this.f32(at + 8, y);
    this.f32(at + 12, radius);
    this.u32(at + 16, rgba);
    this.u32(at + 20, levels);
  }

  submit() {
    this.wasm.pxl_submit(this.count);
  }
}
