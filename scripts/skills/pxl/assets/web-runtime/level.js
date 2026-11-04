export class Level {
  constructor(document, knownAssets = new Set()) {
    if (document.grid.length !== document.grid_h) throw new Error(`${document.id}: grid_h mismatch`);
    this.id = document.id;
    this.name = document.name ?? document.id;
    this.w = document.grid_w;
    this.h = document.grid_h;
    this.spawn = [...document.spawn];
    this.exit = [...document.exit];
    this.seed = BigInt(document.seed ?? 0);
    this.defs = new Map((document.tile_semantics ?? []).map((def) => [def.tile_id, def]));
    this.tiles = [];
    for (const row of document.grid) {
      if ([...row].length !== this.w) throw new Error(`${document.id}: grid_w mismatch`);
      for (const id of row) {
        const def = this.defs.get(id);
        if (!def) throw new Error(`${document.id}: tile ${id} has no semantics`);
        if (def.behaviour !== "floor" && knownAssets.size && !knownAssets.has(id)) {
          throw new Error(`${document.id}: tile ${id} is not delivered`);
        }
        this.tiles.push(id);
      }
    }
    const base = (document.placements ?? []).find((placement) => placement.entity_id === "hq_base");
    this.base = base ? [...base.pos] : null;
  }

  inBounds(x, y) { return x >= 0 && y >= 0 && x < this.w && y < this.h; }
  at(x, y) { return this.inBounds(x, y) ? this.tiles[y * this.w + x] : "f"; }
  setAt(x, y, id) { if (this.inBounds(x, y)) this.tiles[y * this.w + x] = id; }
  blocksTankAt(x, y) { return !this.inBounds(x, y) || Boolean(this.defs.get(this.at(x, y))?.blocks); }
  blocksBulletAt(x, y) { return !this.inBounds(x, y) || this.defs.get(this.at(x, y))?.behaviour === "wall"; }
  isBreakableAt(x, y) { return this.inBounds(x, y) && this.at(x, y) === "b"; }
  isGrassAt(x, y) { return this.inBounds(x, y) && this.at(x, y) === "g"; }
}

export function cloneLevel(level) {
  return Object.assign(Object.create(Object.getPrototypeOf(level)), level, {
    spawn: [...level.spawn], exit: [...level.exit], base: level.base ? [...level.base] : null,
    defs: new Map(level.defs), tiles: [...level.tiles],
  });
}
