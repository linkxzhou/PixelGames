// BGM adapter: a validated BeepBox JSON song -> stereo float PCM for exactly
// one loop. Frozen findings from the A0 spike (plan/v5.0/web-js-engine-plan.md
// section 14.2):
//
//  * beepbox 4.2.0 imports in Node without a DOM; only activateAudio() touches
//    `window`, and this adapter never calls it.
//  * Four places read Math.random (noise phase, supersaw). The stream is
//    re-seeded at the start of every loop pass so each pass is bit-identical.
//  * Pitch instruments reach a bit-identical steady state after one pass; the
//    adapter renders `warmup` passes, discards them, and keeps the next one.
//  * Loop length is the integer round of 2646000*beatsPerBar*bars/bpm. BPM that
//    does not give an integer frame count is rejected by the Rust validator,
//    because beepbox accumulates fractional ticks and the seam would drift.

import { withSeededRandom } from "./prng.mjs";
import { SAMPLE_RATE } from "./sfx.mjs";

let beepboxPromise = null;
function loadBeepbox() {
  if (!beepboxPromise) beepboxPromise = import("./vendor/beepbox/index.js");
  return beepboxPromise;
}

export function loopFrames(bpm, beatsPerBar, bars) {
  const exact = (SAMPLE_RATE * 60 * beatsPerBar * bars) / bpm;
  if (!Number.isInteger(exact)) {
    throw new Error(`bpm ${bpm} with ${beatsPerBar} beats/bar gives a fractional loop length ${exact}`);
  }
  return exact;
}

export async function renderBgm({ seed, song, loop_bars: loopBars, warmup, gain, max_frames: maxFrames }) {
  const { Song, Synth } = await loadBeepbox();
  const bpm = song.beatsPerMinute;
  const beatsPerBar = song.beatsPerBar;
  const frames = loopFrames(bpm, beatsPerBar, loopBars);
  if (frames > maxFrames) {
    throw new Error(`bgm loop is ${frames} frames; limit is ${maxFrames}`);
  }
  if (!Number.isInteger(warmup) || warmup < 1 || warmup > 4) {
    throw new Error("warmup must be an integer from 1 to 4");
  }
  if (typeof gain !== "number" || !Number.isFinite(gain) || gain < 0) {
    throw new Error("gain must be a finite non-negative number");
  }

  const parsed = new Song(JSON.stringify(song));
  if (parsed.loopStart !== 0 || parsed.loopLength !== loopBars) {
    throw new Error(`song loop is ${parsed.loopStart}+${parsed.loopLength} bars; expected 0+${loopBars}`);
  }
  if (parsed.barCount !== loopBars) {
    throw new Error(`song has ${parsed.barCount} bars; expected ${loopBars}`);
  }

  const synth = new Synth(parsed);
  synth.samplesPerSecond = SAMPLE_RATE;
  synth.computeDelayBufferSizes();
  synth.loopRepeatCount = -1;
  synth.volume = 1;
  synth.isPlayingSong = true;

  const left = new Float32Array(frames);
  const right = new Float32Array(frames);
  const chunk = 4096;
  const scratchL = new Float32Array(chunk);
  const scratchR = new Float32Array(chunk);

  withSeededRandom(seed, () => {
    synth.warmUpSynthesizer(parsed);
    synth.snapToStart();
  });

  for (let pass = 0; pass <= warmup; pass++) {
    // Re-seed per pass so noise-family voices restart from the same stream.
    withSeededRandom(seed, () => {
      for (let off = 0; off < frames; off += chunk) {
        const n = Math.min(chunk, frames - off);
        scratchL.fill(0, 0, n);
        scratchR.fill(0, 0, n);
        synth.synthesize(scratchL, scratchR, n, true);
        if (pass === warmup) {
          left.set(scratchL.subarray(0, n), off);
          right.set(scratchR.subarray(0, n), off);
        }
      }
    });
  }
  synth.isPlayingSong = false;

  const pcm = new Float32Array(frames * 2);
  for (let i = 0; i < frames; i++) {
    pcm[2 * i] = left[i] * gain;
    pcm[2 * i + 1] = right[i] * gain;
  }
  return { channels: 2, frames, pcm };
}
