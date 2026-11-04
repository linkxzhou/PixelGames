export class FixedStep {
  constructor(dt = 1 / 60, maxSteps = 15) { this.dt = dt; this.maxSteps = maxSteps; this.accumulator = 0; }
  advance(frameTime, update) {
    this.accumulator = Math.min(this.accumulator + Math.max(0, frameTime), this.dt * this.maxSteps);
    let steps = 0;
    while (this.accumulator >= this.dt && steps < this.maxSteps) { update(this.dt); this.accumulator -= this.dt; steps += 1; }
    return steps;
  }
}
