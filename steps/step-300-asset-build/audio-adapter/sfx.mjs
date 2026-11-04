// SFX adapter: named parameters -> ZzFX positional arguments -> mono float PCM.
//
// ZzFX 1.4.0 creates an AudioContext when the module is evaluated and reads
// Math.random for the optional frequency jitter. Neither is wanted offline, so
// a stub context is installed before the dynamic import and every call runs
// inside `withSeededRandom`.

import { withSeededRandom } from "./prng.mjs";

export const SAMPLE_RATE = 44100;

export const WAVEFORMS = Object.freeze({
  sine: 0,
  triangle: 1,
  saw: 2,
  tan: 3,
  noise: 4,
  square: 5,
});

// name, ZzFX position, default. Order here is the documented parameter table.
const FIELDS = Object.freeze([
  ["volume", 0, 1],
  ["randomness", 1, 0],
  ["frequency_hz", 2, 220],
  ["attack_sec", 3, 0],
  ["sustain_sec", 4, 0],
  ["release_sec", 5, 0.1],
  ["shape_curve", 7, 1],
  ["slide", 8, 0],
  ["delta_slide", 9, 0],
  ["pitch_jump_hz", 10, 0],
  ["pitch_jump_time_sec", 11, 0],
  ["repeat_time_sec", 12, 0],
  ["noise", 13, 0],
  ["modulation", 14, 0],
  ["bit_crush", 15, 0],
  ["delay_sec", 16, 0],
  ["sustain_volume", 17, 1],
  ["decay_sec", 18, 0],
  ["tremolo", 19, 0],
  ["filter_hz", 20, 0],
]);

let zzfxPromise = null;
function loadZzfx() {
  if (!zzfxPromise) {
    if (typeof globalThis.AudioContext === "undefined") {
      globalThis.AudioContext = class StubAudioContext {};
    }
    zzfxPromise = import("./vendor/zzfx/ZzFX.js").then((m) => m.ZZFX);
  }
  return zzfxPromise;
}

export function positionalArguments(params) {
  const known = new Set(["waveform", ...FIELDS.map((f) => f[0])]);
  for (const key of Object.keys(params)) {
    if (!known.has(key)) throw new Error(`unknown sfx parameter '${key}'`);
  }
  const waveform = params.waveform ?? "square";
  if (!Object.hasOwn(WAVEFORMS, waveform)) {
    throw new Error(`unknown waveform '${waveform}'`);
  }
  const args = new Array(21).fill(0);
  args[6] = WAVEFORMS[waveform];
  for (const [name, position, fallback] of FIELDS) {
    const value = params[name] ?? fallback;
    if (typeof value !== "number" || !Number.isFinite(value)) {
      throw new Error(`sfx parameter '${name}' must be a finite number`);
    }
    args[position] = value;
  }
  return args;
}

export function sfxFrameCount(params) {
  const a = positionalArguments(params);
  const attack = a[3] * SAMPLE_RATE || 9;
  return (attack + a[18] * SAMPLE_RATE + a[4] * SAMPLE_RATE + a[5] * SAMPLE_RATE + a[16] * SAMPLE_RATE) | 0;
}

export async function renderSfx({ seed, params, gain, max_frames: maxFrames }) {
  const ZZFX = await loadZzfx();
  ZZFX.sampleRate = SAMPLE_RATE;
  const args = positionalArguments(params);
  if (typeof gain !== "number" || !Number.isFinite(gain) || gain < 0) {
    throw new Error("gain must be a finite non-negative number");
  }
  const expected = sfxFrameCount(params);
  if (expected <= 0) throw new Error("sfx has no samples");
  if (expected > maxFrames) {
    throw new Error(`sfx would render ${expected} frames; limit is ${maxFrames}`);
  }
  const samples = withSeededRandom(seed, () => ZZFX.buildSamples(...args));
  const out = new Float32Array(samples.length);
  for (let i = 0; i < samples.length; i++) out[i] = samples[i] * gain;
  return { channels: 1, frames: out.length, pcm: out };
}
