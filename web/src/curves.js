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
 * The antiderivative is printed beside the function because it is the thing
 * that makes the device work. First-order antiderivative antialiasing
 * evaluates `(F₀(x[n]) − F₀(x[n−1])) / (x[n] − x[n−1])` instead of `f(x[n])`,
 * so a curve is only usable here if its `F₀` is elementary. All six are, which
 * is why the menu costs nothing to extend and why there is no quality mode to
 * switch off.
 *
 * **`F0`, not `F1`, and the constant is not free.** It is the same object —
 * the first antiderivative — written with the integration constant chosen so
 * `F₀(0) = 0`. Parker et al. recommend that as a precision improvement that
 * costs nothing: the difference quotient above subtracts two nearby values of
 * `F₀`, so keeping the function small near the operating point keeps the
 * cancellation from eating mantissa. Every entry below satisfies it and the
 * test checks the derivative rather than trusting the algebra.
 *
 * **The set, the order and the forms are the engine's.** Warm, Round, Soft,
 * Clip, Fold, Gate, in order of hardness, with Soft the default. `curve`'s
 * labels come from the manifest and the page follows them; this table is keyed
 * by the label's first word and looked up with [`curveFor`], which falls back
 * to a curve with **no** equation rather than to the wrong one.
 *
 * Shared shape: every curve is odd, every one has unit slope through the
 * origin **except Gate, whose slope there is deliberately zero**, and every
 * one has a ceiling of exactly one except Fold, which folds instead.
 *
 * **These are the page's copies and they are never the source of a number
 * while an engine is connected.** All of the mathematics belongs in Rust; the
 * page renders what the engine publishes. What lives here is the printed
 * equations, and a stand-in used only to draw a curve when no `transfer`
 * stream exists — a state the display announces on screen. `test/shape.test.js`
 * differentiates every printed antiderivative numerically and checks it comes
 * back to the printed function, so the equation on the face cannot drift from
 * the curve the face draws.
 *
 * `f` and `F0` take `(x, p)` where `p` carries the curve-shaping parameters —
 * only Clip reads one, its knee in decibels below the ceiling.
 */

/** Clip's knee, in dB below the ceiling; a missing one is the parameter's default. */
const kneeDbOf = (p) => Math.min(24, Math.max(0, p && typeof p.kneeDb === 'number' ? p.kneeDb : 6));
/**
 * Where the knee starts, as a fraction of the ceiling: the engine's
 * `w = 1 − 10^(−knee/20)` and `k = 1 − w`, which is just `10^(−knee/20)`.
 * At 0 dB it is 1 and the curve is a true hard clip; at 24 dB it is 0.063 and
 * almost the whole range is knee.
 */
const kneeK = (p) => 10 ** (-kneeDbOf(p) / 20);

const WARM = {
  key: 'warm',
  label: 'Warm',
  sub: 'the arctangent',
  eq: ['f(x) = (2∕π)·arctan(πx∕2)'],
  anti: ['F₀(x) = (2∕π)·[ x·arctan(πx∕2) − (1∕π)·ln(1 + (πx∕2)²) ]'],
  note: 'The gentlest of the six. Nonlinear everywhere, so it colours quietly at any level rather than waiting for a threshold, and it gives up its slope more slowly than the hyperbolic tangent does.',
  f: (x) => (2 / Math.PI) * Math.atan((Math.PI * x) / 2),
  F0: (x) => {
    const a = Math.PI / 2;
    return (2 / Math.PI) * (x * Math.atan(a * x) - (1 / Math.PI) * Math.log(1 + a * a * x * x));
  },
};

const ROUND = {
  key: 'round',
  label: 'Round',
  sub: 'the algebraic one',
  eq: ['f(x) = x ∕ √(1 + x²)'],
  anti: ['F₀(x) = √(1 + x²) − 1'],
  note: 'One square root and a divide, and the tidiest antiderivative on the menu. Approaches its ceiling but never reaches it, so heavy drive compresses rather than squares off.',
  f: (x) => x / Math.sqrt(1 + x * x),
  F0: (x) => Math.sqrt(1 + x * x) - 1,
};

const SOFT = {
  key: 'soft',
  label: 'Soft',
  sub: 'the hyperbolic tangent',
  eq: ['f(x) = tanh x'],
  anti: ['F₀(x) = ln cosh x'],
  note: 'The default, and the one most people mean by saturation. Harder into its ceiling than Warm or Round and still smooth everywhere.',
  f: (x) => Math.tanh(x),
  // `ln cosh x` written so it does not overflow: `cosh(750)` is already Infinity.
  F0: (x) => Math.abs(x) + Math.log((1 + Math.exp(-2 * Math.abs(x))) / 2),
};

/*
 * The one curve on the menu with a parameter, and the parameter is in decibels
 * below the ceiling rather than a unit-free fraction — the same argument that
 * put a Q on the colour width.
 *
 * With `k = 10^(−knee/20)`: exactly linear to `k`, a quadratic knee from `k` to
 * `2 − k`, and exactly one above that. Slope matches at both joins, so the
 * curve is C¹ wherever the knee is open, and at a knee of 0 dB the middle
 * branch has no domain left and what remains is a true hard clip with a slope
 * discontinuity at ±1.
 *
 * `C` is the constant that carries `F₀` across the second join; it is written
 * out rather than folded in because the value is what makes the two branches
 * meet, and a reader checking the panel's equation should be able to see it.
 */
const CLIP = {
  key: 'clip',
  label: 'Clip',
  sub: 'a corner you can open',
  eq: [
    'f(x) = x    for |x| ≤ k',
    'f(x) = sgn x·( |x| − (|x|−k)² ∕ 4(1−k) )    through the knee',
    'f(x) = sgn x    for |x| ≥ 2 − k,   with k = 10^(−knee∕20)',
  ],
  anti: [
    'F₀(x) = x²∕2',
    'F₀(x) = x²∕2 − (|x|−k)³ ∕ 12(1−k)    through the knee',
    'F₀(x) = C + (|x| − (2−k)),   C = (2−k)²∕2 − ⅔(1−k)²    above it',
  ],
  note: 'At a knee of 0 dB this is a true slope discontinuity, which is the hardest thing on this menu to antialias and the reason the readout is worth watching here. Opening the knee rounds the corner and the aliasing falls with it.',
  f: (x, p) => {
    const k = kneeK(p);
    const a = Math.abs(x);
    if (a <= k) return x;
    if (a >= 2 - k) return Math.sign(x);
    const u = a - k;
    return Math.sign(x) * (a - (u * u) / (4 * (1 - k)));
  },
  F0: (x, p) => {
    const k = kneeK(p);
    const a = Math.abs(x);
    if (a <= k) return (x * x) / 2;
    if (a >= 2 - k) {
      const c = (2 - k) ** 2 / 2 - (2 / 3) * (1 - k) ** 2;
      return c + (a - (2 - k));
    }
    const u = a - k;
    return (x * x) / 2 - (u * u * u) / (12 * (1 - k));
  },
};

const FOLD = {
  key: 'fold',
  label: 'Fold',
  sub: 'the wavefolder',
  eq: ['f(x) = sin x'],
  anti: ['F₀(x) = 1 − cos x'],
  note: 'The exception to the ceiling: past the turnover the output folds back instead of flattening, and the harmonic series stops being bounded. A wavefolder is the standard aliasing testbench, so this curve decides whether the target is met.',
  f: (x) => Math.sin(x),
  F0: (x) => 1 - Math.cos(x),
};

/*
 * The exception to the unit-slope rule, and deliberately so. Gate is the
 * answer to a Damp control: it flattens towards the origin, so quiet material
 * is held down rather than passed.
 *
 * **The corner is rounded, and the shape was chosen for the antialiasing
 * rather than for the tone.** Near zero the difference of the two hyperbolic
 * tangents expands as `2x³` — `2(x − x³/3) − (2x − 8x³/3) = 2x³` — so the
 * curve leaves the origin with zero slope and zero curvature and no
 * discontinuity anywhere. A discontinuity in the function itself is the single
 * case first-order antialiasing handles worst, which is exactly what a
 * hard-gated dead zone would have given.
 *
 * The engine writes it as the family `(tanh x − a·tanh(x/a))/(1 − a)` with
 * `a = 1/2`. That reduces exactly to the printed form, and the panel prints
 * one of them rather than both.
 */
const GATE = {
  key: 'gate',
  label: 'Gate',
  sub: 'flat near the origin',
  eq: ['f(x) = 2·tanh x − tanh 2x'],
  anti: ['F₀(x) = 2·ln cosh x − ½·ln cosh 2x'],
  note: 'Slope zero at the origin, so it holds quiet material down and only opens as the level comes up. It flattens as 2x³ rather than stepping, which is what keeps it antialiasable at first order.',
  f: (x) => 2 * Math.tanh(x) - Math.tanh(2 * x),
  F0: (x) => 2 * lnCosh(x) - 0.5 * lnCosh(2 * x),
};

/** `ln cosh x` without the overflow: `cosh(750)` is already Infinity. */
function lnCosh(x) {
  const a = Math.abs(x);
  return a + Math.log((1 + Math.exp(-2 * a)) / 2);
}

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
  F0: (x) => lnCosh(x),
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
 * The device's transfer, **as a stand-in only**: what the page draws when no
 * engine is publishing a `transfer` stream. All of the mathematics belongs in
 * Rust, so whenever a stream exists the display uses it and this is not
 * called; when it is, the display says on screen that the curve is the page's
 * own arithmetic rather than the engine's.
 *
 * Drive in, the curve, the output trim, then the post clipper — the same chain
 * the engine's stream declares, with the same two exclusions. Colour is
 * excluded because it is frequency-dependent and a transfer plot has no
 * frequency axis; mix is excluded because it is not a shape.
 *
 * Bias is a percentage of the clipping point, and this only removes the DC an
 * offset creates: `f(x + b) − f(b)` keeps zero in mapping to zero out. It does
 * not rescale by the slope at the bias point, because the law that does would
 * divide by zero for Gate, whose slope at the origin is zero by design.
 *
 * @param {typeof SOFT} curve
 * @param {{ driveDb?: number, biasPct?: number, outputDb?: number, clipMode?: number, kneeDb?: number }} at
 * @returns {(x: number) => number}
 */
export function transferAt(curve, { driveDb = 0, biasPct = 0, outputDb = 0, clipMode = 0, kneeDb = 6 } = {}) {
  const g = 10 ** (driveDb / 20);
  const trim = 10 ** (outputDb / 20);
  const bias = biasPct / 100;
  const p = { kneeDb };
  const offset = curve.f(bias, p);
  const ceiling =
    clipMode === 1 ? (v) => CLIP.f(v, p) : clipMode === 2 ? (v) => Math.min(1, Math.max(-1, v)) : (v) => v;
  return (x) => ceiling(trim * (curve.f(g * x + bias, p) - offset));
}
