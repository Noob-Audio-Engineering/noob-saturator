/**
 * The six shaping curves, as the panel prints them.
 *
 * **Every curve on this device ships with its transfer function and its first
 * antiderivative printed on the face.** That is the point of the table: no
 * Ableton document publishes an equation for any curve in any device, and the
 * curves in Saturator have never been specified anywhere, so an entire
 * section of the dossier behind this plug-in is inference about shapes nobody
 * can check. Ours are ours, they are on the panel, and anybody can check them.
 *
 * The first antiderivative is printed beside the function because it is the
 * thing that makes the device work. First-order antiderivative antialiasing
 * evaluates `(F₁(x[n]) − F₁(x[n−1])) / (x[n] − x[n−1])` instead of `f(x[n])`,
 * so a curve is only usable here if its `F₁` is elementary. All six are,
 * which is why the menu costs nothing to extend and why there is no quality
 * mode to switch off.
 *
 * **The set and the order are the engine's**, frozen: Warm, Round, Soft,
 * Clip, Fold, Gate, in order of hardness, with Soft the default. `curve`'s
 * labels come from the manifest and the page follows them; this table is
 * keyed by the label's first word and looked up with [`curveFor`], which
 * falls back to a curve with **no** equation rather than to the wrong one.
 *
 * Shared shape, also the engine's: every curve is odd, every one has unit
 * slope through the origin **except Gate, whose slope there is deliberately
 * zero**, and every one has a ceiling of exactly one except Fold, which folds
 * instead of having a ceiling.
 *
 * **Each entry carries `F1` as code as well as `anti` as print, and that is
 * what `test/shape.test.js` is for.** It differentiates the antiderivative
 * numerically and checks it comes back to `f`, so the equation on the panel
 * cannot drift away from the function the panel draws. A device whose whole
 * argument is that it publishes its equations has to be able to show that the
 * published ones are right.
 *
 * `f` and `F1` take `(x, p)` where `p` carries the curve-shaping parameters —
 * only Clip reads one, its knee. These are the page's copies, for drawing the
 * curve when the engine is not there to publish it; the audio path's are in
 * the Rust half and the two are checked against each other by the engine's
 * own tests, not by this file.
 */

/** Everything below treats a missing knee as the middle of its range, which is the parameter's default. */
const kneeOf = (p) => Math.min(1, Math.max(0, p && typeof p.knee === 'number' ? p.knee : 0.5));

const WARM = {
  key: 'warm',
  label: 'Warm',
  sub: 'the arctangent',
  eq: ['f(x) = (2∕π)·arctan(πx∕2)'],
  anti: ['F₁(x) = (2∕π)·[ x·arctan(πx∕2) − (1∕π)·ln(1 + (πx∕2)²) ]'],
  note: 'The gentlest of the six. Nonlinear everywhere, so it colours quietly at any level rather than waiting for a threshold, and it gives up its slope more slowly than the hyperbolic tangent does.',
  f: (x) => (2 / Math.PI) * Math.atan((Math.PI * x) / 2),
  F1: (x) => {
    const a = Math.PI / 2;
    return (2 / Math.PI) * (x * Math.atan(a * x) - (1 / Math.PI) * Math.log(1 + a * a * x * x));
  },
};

const ROUND = {
  key: 'round',
  label: 'Round',
  sub: 'the algebraic one',
  eq: ['f(x) = x ∕ √(1 + x²)'],
  anti: ['F₁(x) = √(1 + x²) − 1'],
  note: 'One square root and a divide, and the tidiest antiderivative on the menu. Approaches its ceiling but never reaches it, so heavy drive compresses rather than squares off.',
  f: (x) => x / Math.sqrt(1 + x * x),
  F1: (x) => Math.sqrt(1 + x * x) - 1,
};

const SOFT = {
  key: 'soft',
  label: 'Soft',
  sub: 'the hyperbolic tangent',
  eq: ['f(x) = tanh x'],
  anti: ['F₁(x) = ln cosh x'],
  note: 'The default, and the one most people mean by saturation. Harder into its ceiling than Warm or Round and still smooth everywhere.',
  f: (x) => Math.tanh(x),
  // `ln cosh x` written so it does not overflow: `cosh(750)` is already Infinity.
  F1: (x) => Math.abs(x) + Math.log((1 + Math.exp(-2 * Math.abs(x))) / 2),
};

/*
 * The one curve on the menu with a parameter. `knee` runs 0 to 1: at zero it
 * is a true hard clip with a slope discontinuity at ±1, and it opens into a
 * quadratic knee as the control comes up. The linear stretch below the knee
 * is exactly linear, the ceiling above it is exactly one, and the two are
 * joined with a matching slope, so the curve is C¹ everywhere the knee is
 * open.
 *
 * With `t = 1 − k`: linear to `t`, the parabola from `t` to `t + 2k`, flat
 * after. The parabola turns over at `t + 2k`, which is exactly where it is
 * cut off, so the branch order matters and the code takes the branches in
 * that order rather than relying on a `min`.
 */
const CLIP = {
  key: 'clip',
  label: 'Clip',
  sub: 'a corner you can open',
  eq: [
    'f(x) = x    for |x| ≤ t',
    'f(x) = sgn x·( |x| − (|x|−t)² ∕ 4k )    through the knee',
    'f(x) = sgn x    for |x| ≥ t + 2k,   with t = 1 − k',
  ],
  anti: [
    'F₁(x) = x²∕2',
    'F₁(x) = x²∕2 − (|x|−t)³ ∕ 12k    through the knee',
    'F₁(x) = |x| − 1∕2 − k²∕6    above it',
  ],
  note: 'At knee zero this is a true slope discontinuity, which is the hardest thing on this menu to antialias and the reason the readout is worth watching here. Opening the knee rounds the corner and the aliasing falls with it.',
  f: (x, p) => {
    const k = kneeOf(p);
    const a = Math.abs(x);
    const s = Math.sign(x);
    if (k < 1e-6) return Math.min(1, Math.max(-1, x));
    const t = 1 - k;
    if (a <= t) return x;
    if (a >= t + 2 * k) return s;
    const u = a - t;
    return s * (a - (u * u) / (4 * k));
  },
  F1: (x, p) => {
    const k = kneeOf(p);
    const a = Math.abs(x);
    if (k < 1e-6) return a >= 1 ? a - 0.5 : (x * x) / 2;
    const t = 1 - k;
    if (a <= t) return (x * x) / 2;
    if (a >= t + 2 * k) return a - 0.5 - (k * k) / 6;
    const u = a - t;
    return (x * x) / 2 - (u * u * u) / (12 * k);
  },
};

const FOLD = {
  key: 'fold',
  label: 'Fold',
  sub: 'the wavefolder',
  eq: ['f(x) = (2∕π)·sin(πx∕2)'],
  anti: ['F₁(x) = (4∕π²)·(1 − cos(πx∕2))'],
  note: 'The exception to the ceiling: past the turnover the output folds back instead of flattening, and the harmonic series stops being bounded. A wavefolder is the standard aliasing testbench, so this curve decides whether the target is met.',
  f: (x) => (2 / Math.PI) * Math.sin((Math.PI * x) / 2),
  F1: (x) => (4 / (Math.PI * Math.PI)) * (1 - Math.cos((Math.PI * x) / 2)),
};

/*
 * The exception to the unit-slope rule, and deliberately so: Gate flattens
 * towards the origin, so its slope there is zero and quiet material is pushed
 * down rather than passed.
 *
 * **The corner is rounded, not a step.** A curve that flattens near the
 * origin is the one case first-order antialiasing handles worst if the
 * flattening is a true discontinuity; rounding it is the standard remedy, and
 * `x²/(1+x²)` is C¹ at the origin with a slope of exactly zero there.
 */
const GATE = {
  key: 'gate',
  label: 'Gate',
  sub: 'flat near the origin',
  eq: ['f(x) = sgn x · x² ∕ (1 + x²)'],
  anti: ['F₁(x) = |x| − arctan |x|'],
  note: 'Slope zero at the origin, so it holds quiet material down and only opens as the level comes up. The corner is rounded rather than a step, which is what keeps it antialiasable at first order.',
  f: (x) => (Math.sign(x) * (x * x)) / (1 + x * x),
  F1: (x) => Math.abs(x) - Math.atan(Math.abs(x)),
};

/** Every curve the page knows an equation for, in the engine's menu order. */
export const CURVES = [WARM, ROUND, SOFT, CLIP, FOLD, GATE];

/** A curve the page has no equation for: the display says so rather than printing someone else's. */
const UNKNOWN = {
  key: 'unknown',
  label: '',
  sub: '',
  eq: [],
  anti: [],
  note: 'This build publishes a curve the panel has no equation for. The equation belongs on the face, so this is a defect in the page and not a choice.',
  f: (x) => Math.tanh(x),
  F1: (x) => Math.abs(x) + Math.log((1 + Math.exp(-2 * Math.abs(x))) / 2),
};

/**
 * The curve entry for a manifest label, matched case-insensitively on the
 * first word so `Soft`, `soft` and `Soft (tanh)` all land on the same one.
 * @param {string} label
 * @returns {typeof SOFT}
 */
export function curveFor(label) {
  if (!label) return UNKNOWN;
  const head = String(label).trim().toLowerCase().split(/[\s(]/)[0];
  return CURVES.find((c) => c.key === head) || { ...UNKNOWN, label };
}

/**
 * The device's transfer, as the *page* computes it when the engine is not
 * there to publish one: drive in, the curve, the output trim, then the post
 * clipper — the same chain the engine's `transfer` stream declares, and with
 * the same two exclusions. Colour is excluded because it is
 * frequency-dependent and a transfer plot has no frequency axis; mix is
 * excluded because it is not a shape. The display says both out loud.
 *
 * **Bias is an offset into the shaper and the page only removes the DC it
 * creates.** `f(x + b) − f(b)` keeps zero in mapping to zero out, so the plot
 * stays anchored, and it does not rescale by the slope at the bias point: the
 * tube-preamp law that does divide by `f'(b)` would blow this drawing up for
 * Gate, whose slope at the origin is zero by design. What the audio path
 * actually does is the engine's to decide and the engine publishes the
 * result on the `transfer` stream, which is what the display prefers.
 *
 * @param {typeof SOFT} curve
 * @param {{ driveDb?: number, bias?: number, outputDb?: number, clipMode?: number, knee?: number }} at
 * @returns {(x: number) => number}
 */
export function transferAt(curve, { driveDb = 0, bias = 0, outputDb = 0, clipMode = 0, knee = 0.5 } = {}) {
  const g = 10 ** (driveDb / 20);
  const trim = 10 ** (outputDb / 20);
  const p = { knee };
  const offset = curve.f(bias, p);
  const ceiling = clipMode === 1 ? (v) => CLIP.f(v, p) : clipMode === 2 ? (v) => Math.min(1, Math.max(-1, v)) : (v) => v;
  return (x) => ceiling(trim * (curve.f(g * x + bias, p) - offset));
}
