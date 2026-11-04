// Deterministic scene selection for prototype-library assets.
//
// Everything here is a pure function of frozen inputs: a world seed, stable
// object ids, integer coordinates and a rule table. Nothing reads the camera,
// the frame counter, load order or Math.random, so the same map always selects
// the same art. The runtime consumes these results; it never invents pixels.

const MASK32 = 0xffffffffn;

/** FNV-1a over UTF-8 bytes, 32-bit. Stable across platforms and engines. */
export function hash32(text) {
  let h = 0x811c9dc5;
  const bytes = new TextEncoder().encode(text);
  for (const byte of bytes) {
    h ^= byte;
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}

/** Final avalanche so neighbouring inputs do not produce neighbouring outputs. */
function mix(value) {
  let h = BigInt(value >>> 0);
  h ^= h >> 16n;
  h = (h * 0x85ebca6bn) & MASK32;
  h ^= h >> 13n;
  h = (h * 0xc2b2ae35n) & MASK32;
  h ^= h >> 16n;
  return Number(h & MASK32);
}

/**
 * Stable integer in [0, count) for one placement.
 * `seed` is the frozen world seed; `id` names the object or tile family.
 */
export function pick(seed, id, x, y, count) {
  if (!Number.isInteger(count) || count < 1) throw new Error(`pick: count must be a positive integer, got ${count}`);
  if (!Number.isInteger(x) || !Number.isInteger(y)) throw new Error(`pick: coordinates must be integers, got ${x},${y}`);
  return mix(hash32(`${BigInt(seed)}|${id}|${x}|${y}`)) % count;
}

/**
 * Choose among variants with weights while avoiding an immediate repeat of the
 * left or upper neighbour, when another choice exists. `chosen` is a Map of
 * already-decided cells keyed "x,y"; scan order must therefore be fixed
 * (row-major), which `selectGrid` guarantees.
 */
export function pickAvoidingRepeat(seed, id, x, y, count, chosen) {
  if (count === 1) return 0;
  const left = chosen.get(`${x - 1},${y}`);
  const up = chosen.get(`${x},${y - 1}`);
  const first = pick(seed, id, x, y, count);
  const blocked = new Set([left, up].filter((value) => value !== undefined));
  if (!blocked.has(first) || blocked.size >= count) return first;
  // Walk forward from the hashed choice to the next allowed variant. The walk
  // depends only on frozen inputs, so it is as stable as the first pick.
  for (let step = 1; step < count; step += 1) {
    const candidate = (first + step) % count;
    if (!blocked.has(candidate)) return candidate;
  }
  return first;
}

/** Row-major selection over a rectangular grid of variant counts. */
export function selectGrid(seed, id, width, height, countAt) {
  const chosen = new Map();
  const out = new Array(width * height);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const count = countAt(x, y);
      const index = count > 0 ? pickAvoidingRepeat(seed, id, x, y, count, chosen) : -1;
      chosen.set(`${x},${y}`, index);
      out[y * width + x] = index;
    }
  }
  return out;
}

// --- terrain adjacency -------------------------------------------------------

/** Bit order for the 8-neighbourhood: N, E, S, W, NE, SE, SW, NW. */
const OFFSETS = [
  [0, -1], [1, 0], [0, 1], [-1, 0],
  [1, -1], [1, 1], [-1, 1], [-1, -1],
];

/**
 * Reduce a cell to the shape of its boundary against one other terrain.
 *
 * `same(x, y)` says whether a cell belongs to the terrain being drawn. A
 * diagonal neighbour only matters when both orthogonal neighbours beside it are
 * the same terrain, otherwise the edge already covers it. This is the standard
 * 47-tile reduction, expressed as a mask rather than a lookup of 47 images.
 */
export function neighbourMask(same, x, y) {
  const [n, e, s, w, ne, se, sw, nw] = OFFSETS.map(([dx, dy]) => same(x + dx, y + dy));
  let mask = 0;
  if (n) mask |= 1;
  if (e) mask |= 2;
  if (s) mask |= 4;
  if (w) mask |= 8;
  if (ne && n && e) mask |= 16;
  if (se && s && e) mask |= 32;
  if (sw && s && w) mask |= 64;
  if (nw && n && w) mask |= 128;
  return mask;
}

/** Human-readable shape of a mask, used in diagnostics and rule tables. */
export function describeMask(mask) {
  const edges = [];
  if (mask & 1) edges.push("N");
  if (mask & 2) edges.push("E");
  if (mask & 4) edges.push("S");
  if (mask & 8) edges.push("W");
  const corners = [];
  if (mask & 16) corners.push("NE");
  if (mask & 32) corners.push("SE");
  if (mask & 64) corners.push("SW");
  if (mask & 128) corners.push("NW");
  return `${edges.join("") || "-"}${corners.length ? `+${corners.join(",")}` : ""}`;
}

/** The 47 distinct masks that can occur after the diagonal reduction. */
export function reachableMasks() {
  const seen = new Set();
  for (let bits = 0; bits < 256; bits += 1) {
    const n = bits & 1, e = (bits >> 1) & 1, s = (bits >> 2) & 1, w = (bits >> 3) & 1;
    let mask = bits & 0xf;
    if (((bits >> 4) & 1) && n && e) mask |= 16;
    if (((bits >> 5) & 1) && s && e) mask |= 32;
    if (((bits >> 6) & 1) && s && w) mask |= 64;
    if (((bits >> 7) & 1) && n && w) mask |= 128;
    seen.add(mask);
  }
  return [...seen].sort((a, b) => a - b);
}

/**
 * Resolve a rule table to an art id for one cell.
 *
 * `rules` maps a mask to `{ art: string[] }`. A missing mask is an error, never
 * a silent fallback to a plain tile: a gap in the rule table is a gap in the
 * art, and it should be visible.
 */
export function resolveTile(rules, mask, context = "tile") {
  const rule = rules.get(mask);
  if (!rule || !Array.isArray(rule.art) || rule.art.length === 0) {
    throw new Error(`${context}: no art for adjacency ${describeMask(mask)} (mask ${mask})`);
  }
  return rule;
}

/** Masks a rule table fails to cover, so incomplete libraries are reported up front. */
export function missingMasks(rules) {
  return reachableMasks().filter((mask) => {
    const rule = rules.get(mask);
    return !rule || !Array.isArray(rule.art) || rule.art.length === 0;
  });
}

// --- depth ordering ----------------------------------------------------------

/**
 * Draw order for objects with a base line. Objects sort by the world y of
 * their `sortY` (the row where they touch the ground), not by the top of their
 * sprite, so a tall tree behind a person is drawn first and the person's head
 * overlaps the trunk only when they are actually in front of it.
 *
 * Ties break by x and then id so the order never depends on array order.
 */
export function depthOrder(objects) {
  return [...objects].sort((a, b) => {
    const ay = a.y + a.sortY;
    const by = b.y + b.sortY;
    if (ay !== by) return ay - by;
    if (a.x !== b.x) return a.x - b.x;
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
  });
}

/**
 * Split an object into draw calls by layer so a roof or canopy can pass over an
 * actor that stands behind the building. `layers` come from the prototype.
 * Back layers sort with the object's base line; front layers sort one row in
 * front of everything that shares that base line.
 */
export function layeredDrawList(objects, layersOf) {
  const calls = [];
  for (const object of depthOrder(objects)) {
    const layers = layersOf(object);
    if (!layers.length) {
      calls.push({ id: object.id, layer: "whole", order: object.y + object.sortY });
      continue;
    }
    for (const layer of layers) {
      const bias = layer.layer === "front" ? 0.5 : 0;
      calls.push({ id: object.id, layer: layer.id, order: object.y + object.sortY + bias });
    }
  }
  return calls.sort((a, b) => a.order - b.order || (a.id < b.id ? -1 : a.id > b.id ? 1 : a.layer < b.layer ? -1 : 1));
}

/** True when `actor` stands behind the object's base line and so is overdrawn by its front layer. */
export function isBehind(actor, object) {
  return actor.y + (actor.sortY ?? 0) < object.y + object.sortY;
}
