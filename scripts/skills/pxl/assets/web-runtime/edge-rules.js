// Adjacency rule tables for hand-drawn edge families.
//
// A family draws terrain A with terrain B open on one or more sides: 4 straight edges,
// 4 outer corners and 4 inner corners, plus a plain fill. That is 13 images, not the 47
// reduced neighbour masks `scene-select.js` can produce. This module says exactly which
// masks those 13 cover, so an incomplete family fails loudly instead of drawing the wrong tile.
//
// Mask bits (see scene-select.js): N=1 E=2 S=4 W=8 NE=16 SE=32 SW=64 NW=128. A bit is set
// when that neighbour is the SAME terrain as the cell being drawn.

import { reachableMasks } from "./scene-select.js";

const N = 1, E = 2, S = 4, W = 8, NE = 16, SE = 32, SW = 64, NW = 128;

/** Art name for one mask, or null when the 13 drawn shapes cannot show it exactly. */
/** Open sides in fixed N-E-S-W order, as a string such as "ne" or "sw". */
function openSides(mask) {
  return [["n", N], ["e", E], ["s", S], ["w", W]].filter(([, bit]) => !(mask & bit)).map(([k]) => k).join("");
}

const CORNER_BY_OPEN = { ne: "corner_ne", es: "corner_se", sw: "corner_sw", nw: "corner_nw" };

export function exactShape(mask) {
  const open = openSides(mask);

  if (open.length === 0) {
    const missing = [["ne", NE], ["se", SE], ["sw", SW], ["nw", NW]].filter(([, bit]) => !(mask & bit));
    if (missing.length === 0) return "fill";
    if (missing.length === 1) return `inner_${missing[0][0]}`;
    return null; // two or more inner corners in one cell
  }
  if (open.length === 1) {
    // A single open side: the two corners on the far side must both be same-terrain, otherwise
    // the cell is an edge that also has a notch, which the plain edge shape cannot show.
    const farCorners = { n: SE | SW, e: SW | NW, s: NE | NW, w: NE | SE }[open];
    return (mask & farCorners) === farCorners ? `edge_${open}` : null;
  }
  if (open.length === 2) {
    const corner = CORNER_BY_OPEN[open];
    if (corner === undefined) return null; // opposite sides open: a one-tile-wide strip
    const farCorner = { corner_ne: SW, corner_se: NW, corner_sw: NE, corner_nw: SE }[corner];
    return mask & farCorner ? corner : null;
  }
  return null; // 3 or 4 open sides: peninsula end or island
}

/**
 * Build the rule table for one family. `prefix` is the library family name, for example
 * "edge_grass_dirt"; `fill` is the art id used for the all-same case.
 *
 * With `approximate: false` (default) masks the family cannot draw are simply absent, so
 * `missingMasks` lists them and `resolveTile` throws. With `approximate: true` each gap
 * is mapped to the nearest drawn shape and recorded in `approximated`, which callers must
 * surface in diagnostics; it is never a silent default.
 */
export function buildEdgeRules(prefix, fill, { approximate = false } = {}) {
  const rules = new Map();
  const exact = [];
  const approximated = [];
  const missing = [];
  for (const mask of reachableMasks()) {
    const shape = exactShape(mask);
    if (shape) {
      rules.set(mask, { art: [shape === "fill" ? fill : `${prefix}_${shape}`], exact: true });
      exact.push(mask);
      continue;
    }
    if (!approximate) {
      missing.push(mask);
      continue;
    }
    const near = nearestShape(mask);
    if (near) {
      rules.set(mask, { art: [near === "fill" ? fill : `${prefix}_${near}`], exact: false });
      approximated.push(mask);
    } else {
      missing.push(mask);
    }
  }
  return { rules, exact, approximated, missing };
}

/** Closest drawn shape for a mask the family cannot show exactly. Deterministic. */
export function nearestShape(mask) {
  const open = openSides(mask);
  if (open.length === 0) {
    // Several inner corners: draw the first missing one in fixed N-E-S-W order.
    for (const [name, bit] of [["ne", NE], ["se", SE], ["sw", SW], ["nw", NW]]) {
      if (!(mask & bit)) return `inner_${name}`;
    }
    return "fill";
  }
  if (open.length === 1) return `edge_${open}`;
  if (open.length === 2) {
    // Opposite sides open: keep the side that comes first in N-E-S-W order.
    return CORNER_BY_OPEN[open] ?? `edge_${open[0]}`;
  }
  // Three or four open sides have no honest approximation; leave them as a reported gap.
  return null;
}
