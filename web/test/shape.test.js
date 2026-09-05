/**
 * The curve table, checked against itself.
 *
 * This device's argument includes publishing the equation and the
 * antiderivative of every curve on the panel, which no Ableton document does
 * for any device. A published equation that has drifted away from the code is
 * worse than none, so these tests hold `curves.js` to what its own file
 * comment promises, and to the shape the engine settled: six curves, all odd,
 * all with unit slope through the origin except Gate whose slope there is
 * zero, all with a ceiling of exactly one except Fold which folds.
 *
 * **They assert mathematics, not measurements.** Differentiating `F0`
 * numerically and getting `f` back is a fact about calculus that holds
 * whatever the curve is; so are the slope, the parity, the bound and the
 * pinned constant. None of it is a figure this project produced and then
 * asserted back at itself, which is the failure mode the build contract names.
 * The aliasing target is a different kind of claim entirely and is measured
 * out of tree by the Rust half, not here.
 *
 *   node --test test/shape.test.js
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { CURVES, curveFor, transferAt } from '../src/curves.js';

/** Where the printed forms are sampled. Clip is the only piecewise one and its joins are skipped by position. */
const SAMPLES = [];
for (let x = -2.4; x <= 2.4001; x += 0.05) SAMPLES.push(Math.round(x * 1000) / 1000);
/**
 * The knee widths Clip is checked at, in dB below the ceiling: a true corner,
 * the default, and the widest the control goes. The knee moves both
 * breakpoints, so every one of these is a different pair of joins.
 */
const KNEES_DB = [0, 3, 6, 12, 24];
/** Where `k = 10^(−knee/20)` puts Clip's two joins at a given knee. */
const clipJoins = (db) => {
  const k = 10 ** (-db / 20);
  return k >= 1 ? [1] : [k, 2 - k];
};

test('every printed antiderivative differentiates back to its printed function', () => {
  const h = 1e-5;
  for (const c of CURVES) {
    for (const kneeDb of c.key === 'clip' ? KNEES_DB : [6]) {
      const p = { kneeDb };
      const joins = c.key === 'clip' ? clipJoins(kneeDb) : [];
      for (const x of SAMPLES) {
        if (joins.some((j) => Math.abs(Math.abs(x) - j) < 3 * h)) continue;
        const slope = (c.F0(x + h, p) - c.F0(x - h, p)) / (2 * h);
        assert.ok(
          Math.abs(slope - c.f(x, p)) < 2e-4,
          `${c.label} at knee ${kneeDb} dB: d/dx F₀(${x}) = ${slope}, but f(${x}) = ${c.f(x, p)}`,
        );
      }
    }
  }
});

test('every antiderivative has its constant pinned, so F₀(0) = 0', () => {
  for (const c of CURVES) {
    for (const kneeDb of c.key === 'clip' ? KNEES_DB : [6]) {
      const v = c.F0(0, { kneeDb });
      assert.ok(Math.abs(v) < 1e-12, `${c.label} at knee ${kneeDb} dB: F₀(0) = ${v}`);
    }
  }
});

test('every curve has unit slope through the origin, except Gate whose slope there is zero', () => {
  const h = 1e-6;
  for (const c of CURVES) {
    const slope = (c.f(h) - c.f(-h)) / (2 * h);
    const want = c.key === 'gate' ? 0 : 1;
    assert.ok(Math.abs(slope - want) < 1e-4, `${c.label}: f'(0) = ${slope}, expected ${want}`);
  }
});

test('Gate leaves the origin as 2x³, which is a rounded corner and not a step', () => {
  const gate = CURVES.find((c) => c.key === 'gate');
  // the cubic coefficient, read off three decades of small x
  for (const x of [1e-2, 1e-3, 1e-4]) {
    const ratio = gate.f(x) / x ** 3;
    assert.ok(Math.abs(ratio - 2) < 1e-3, `f(${x}) / x³ = ${ratio}, expected 2`);
  }
  // and it is C¹ there: the slope goes to zero rather than jumping
  const h = 1e-4;
  const left = (gate.f(-h) - gate.f(-2 * h)) / h;
  const right = (gate.f(2 * h) - gate.f(h)) / h;
  assert.ok(Math.abs(left) < 1e-6 && Math.abs(right) < 1e-6, `slopes either side: ${left}, ${right}`);
});

test('every curve is odd, so bias at zero produces no even harmonics', () => {
  for (const c of CURVES) {
    for (const kneeDb of c.key === 'clip' ? KNEES_DB : [6]) {
      for (const x of SAMPLES) {
        const sum = c.f(x, { kneeDb }) + c.f(-x, { kneeDb });
        assert.ok(Math.abs(sum) < 1e-12, `${c.label} at knee ${kneeDb} dB: f(${x}) + f(${-x}) = ${sum}`);
      }
    }
  }
});

test('five curves are bounded by one and monotone; Fold folds instead', () => {
  for (const c of CURVES) {
    let peak = 0;
    let monotone = true;
    let prev = c.f(-8);
    for (let x = -8; x <= 8; x += 0.01) {
      const y = c.f(x);
      peak = Math.max(peak, Math.abs(y));
      if (y < prev - 1e-12) monotone = false;
      prev = y;
    }
    assert.ok(peak <= 1 + 1e-9, `${c.label}: reaches ${peak}`);
    assert.equal(monotone, c.key !== 'fold', `${c.label}: monotone should be ${c.key !== 'fold'}`);
  }
});

test('Clip reaches exactly one at every knee, is linear below it, and is a true corner at 0 dB', () => {
  const clip = CURVES.find((c) => c.key === 'clip');
  for (const kneeDb of KNEES_DB) {
    const p = { kneeDb };
    const k = 10 ** (-kneeDb / 20);
    assert.ok(Math.abs(clip.f(3, p) - 1) < 1e-12, `knee ${kneeDb} dB: f(3) = ${clip.f(3, p)}`);
    assert.ok(Math.abs(clip.f(2 - k, p) - 1) < 1e-9, `knee ${kneeDb} dB: f(2−k) = ${clip.f(2 - k, p)}`);
    // exactly linear below the knee, at every knee setting
    const below = 0.4 * k;
    assert.ok(Math.abs(clip.f(below, p) - below) < 1e-12, `knee ${kneeDb} dB: f(${below}) = ${clip.f(below, p)}`);
  }
  // at 0 dB the slope drops from one to zero across ±1 with nothing in between
  const h = 1e-4;
  const p0 = { kneeDb: 0 };
  const below = (clip.f(1 - h, p0) - clip.f(1 - 2 * h, p0)) / h;
  const above = (clip.f(1 + 2 * h, p0) - clip.f(1 + h, p0)) / h;
  assert.ok(Math.abs(below - 1) < 1e-6, `below the corner the slope is ${below}`);
  assert.ok(Math.abs(above) < 1e-6, `above the corner the slope is ${above}`);
});

test('a curve the page has no equation for is reported as such rather than given someone else’s', () => {
  const unknown = curveFor('Bass Shaper');
  assert.deepEqual(unknown.eq, []);
  assert.deepEqual(unknown.anti, []);
  assert.equal(unknown.label, 'Bass Shaper');
  // and the names the engine publishes do resolve, however they are cased
  assert.equal(curveFor('Warm').key, 'warm');
  assert.equal(curveFor('soft').key, 'soft');
  assert.equal(curveFor('Gate').key, 'gate');
  assert.equal(curveFor('').key, 'unknown');
});

test('the engine’s six curves are all present, in its menu order, each printing both halves', () => {
  assert.deepEqual(
    CURVES.map((c) => c.label),
    ['Warm', 'Round', 'Soft', 'Clip', 'Fold', 'Gate'],
  );
  for (const c of CURVES) {
    assert.ok(c.eq.length > 0, `${c.label} has no transfer function printed`);
    assert.ok(c.anti.length > 0, `${c.label} has no antiderivative printed`);
  }
});

test('bias offsets the shaper without moving the plot off the origin', () => {
  for (const c of CURVES) {
    for (const biasPct of [-80, -30, 0, 45, 90]) {
      const f = transferAt(c, { biasPct });
      assert.ok(Math.abs(f(0)) < 1e-12, `${c.label} at bias ${biasPct} %: f(0) = ${f(0)}`);
    }
  }
});

test('drive is a pre-gain and output a post-gain, so both are free of the curve', () => {
  const c = CURVES.find((x) => x.key === 'soft');
  const plain = transferAt(c, {});
  const driven = transferAt(c, { driveDb: 12 });
  const trimmed = transferAt(c, { outputDb: -6 });
  const g = 10 ** (12 / 20);
  const t = 10 ** (-6 / 20);
  for (const x of [-1.1, -0.3, 0.2, 0.9]) {
    assert.ok(Math.abs(driven(x) - plain(g * x)) < 1e-12);
    assert.ok(Math.abs(trimmed(x) - t * plain(x)) < 1e-12);
  }
});

test('the post clipper is in the drawn transfer, because the engine says its stream carries it', () => {
  const c = CURVES.find((x) => x.key === 'warm');
  const open = transferAt(c, { driveDb: 24, outputDb: 12, clipMode: 0 });
  const hard = transferAt(c, { driveDb: 24, outputDb: 12, clipMode: 2 });
  const soft = transferAt(c, { driveDb: 24, outputDb: 12, clipMode: 1, kneeDb: 6 });
  assert.ok(open(0.9) > 1, 'the test needs a setting that would exceed the ceiling');
  assert.ok(Math.abs(hard(0.9) - 1) < 1e-12, `hard ceiling gave ${hard(0.9)}`);
  assert.ok(soft(0.9) <= 1 + 1e-12, `soft ceiling gave ${soft(0.9)}`);
});
