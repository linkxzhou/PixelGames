import test from "node:test";
import assert from "node:assert/strict";
import { mulberry32, withSeededRandom } from "../prng.mjs";
import { positionalArguments, renderSfx, sfxFrameCount } from "../sfx.mjs";
import { loopFrames } from "../bgm.mjs";

test("mulberry32 is stable and scoped to the seeded call", () => {
  const a = mulberry32(7);
  const b = mulberry32(7);
  assert.deepEqual([a(), a(), a()], [b(), b(), b()]);
  const original = Math.random;
  const seeded = withSeededRandom(11, () => Math.random());
  assert.equal(withSeededRandom(11, () => Math.random()), seeded);
  assert.equal(Math.random, original);
  assert.throws(() => withSeededRandom(-1, () => {}), /unsigned 32-bit/);
});

test("named SFX parameters map to the frozen ZzFX positions", () => {
  const args = positionalArguments({ waveform: "square", frequency_hz: 440, decay_sec: 0.02 });
  assert.equal(args.length, 21);
  assert.equal(args[2], 440);
  assert.equal(args[6], 5);
  assert.equal(args[18], 0.02);
  assert.equal(args[0], 1);
  assert.equal(sfxFrameCount({ frequency_hz: 440, sustain_sec: 0.1, release_sec: 0.1 }), 8829);
  assert.throws(() => positionalArguments({ typo: 1 }), /unknown sfx parameter/);
});

test("SFX rendering is deterministic for the same asset seed", async () => {
  const request = {
    seed: 19,
    gain: 0.5,
    max_frames: 88200,
    params: { waveform: "noise", randomness: 0.5, frequency_hz: 220, sustain_sec: 0.02, release_sec: 0.03 },
  };
  const first = await renderSfx(request);
  const second = await renderSfx(request);
  assert.equal(first.channels, 1);
  assert.ok(first.frames > 0);
  assert.deepEqual(first.pcm, second.pcm);
  const changed = await renderSfx({ ...request, seed: 20 });
  assert.notDeepEqual(first.pcm, changed.pcm);
});

test("BGM loops use exact source frames and reject fractional lengths", () => {
  assert.equal(loopFrames(150, 8, 8), 1_128_960);
  assert.throws(() => loopFrames(137, 4, 1), /fractional loop length/);
});
