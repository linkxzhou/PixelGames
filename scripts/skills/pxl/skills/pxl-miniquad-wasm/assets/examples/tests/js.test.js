import test from "node:test";
import assert from "node:assert/strict";
import { sum } from "../05-plugin-js-call/plugin.js";
import { FixedStep } from "../06-fixed-step-js-loop/fixed-step.js";

test("05 shared bytes have the same sum in JS", () => {
  assert.equal(sum(new Uint8Array([1, 2, 3, 4])), 10);
});

test("06 fixed step is deterministic and clamps stalls", () => {
  const fixed = new FixedStep();
  let ticks = 0;
  assert.equal(fixed.advance(2, () => { ticks += 1; }), 15);
  assert.equal(ticks, 15);
});
