export function parseHex(raw) {
  const hex = raw.startsWith("#") ? raw.slice(1) : raw;
  if (!/^(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(hex)) {
    throw new Error(`expected #RRGGBB or #RRGGBBAA, got ${raw}`);
  }
  return [
    parseInt(hex.slice(0, 2), 16), parseInt(hex.slice(2, 4), 16),
    parseInt(hex.slice(4, 6), 16), hex.length === 8 ? parseInt(hex.slice(6, 8), 16) : 255,
  ];
}

export class Palette {
  constructor(document) {
    if (!Array.isArray(document.colors) || document.colors.length === 0) {
      throw new Error("palette carries no colors");
    }
    if (document.colors.length > 62) throw new Error("palette exceeds 62 colors");
    this.id = document.id ?? "";
    this.colors = document.colors.map(parseHex);
  }

  get(index) { return this.colors[index] ?? [0, 0, 0, 0]; }

  rgba(indexed, width, height) {
    const out = new Uint8Array(width * height * 4);
    for (let index = 0; index < width * height; index += 1) {
      out.set(this.get(indexed[index]), index * 4);
    }
    return out;
  }
}
