// Deterministic PRNG shared by the adapter. mulberry32 is a pure function of the
// 32-bit seed, so a render never depends on call order outside its own stream.
// Per-asset seeds are derived on the Rust side (pxl-audio `derive_seed`) and
// arrive here as a plain u32.

export function mulberry32(seed) {
  let a = seed >>> 0;
  return function next() {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// Replace Math.random for the duration of `fn` with a seeded stream. The
// vendored synthesizers read Math.random in a handful of places; this is the
// single injection point that makes their output reproducible.
export function withSeededRandom(seed, fn) {
  if (!Number.isInteger(seed) || seed < 0 || seed > 0xffffffff) {
    throw new Error("seed must be an unsigned 32-bit integer");
  }
  const original = Math.random;
  Math.random = mulberry32(seed);
  try {
    return fn();
  } finally {
    Math.random = original;
  }
}
