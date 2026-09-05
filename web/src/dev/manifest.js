/**
 * Design-time manifest for Noob Saturator: what the plug-in publishes,
 * described up front so the page can be built and looked at before the DSP
 * exists. Only development builds load it (see `main.js`), and the client
 * hands over to the real server the moment `/ws` answers.
 *
 * **Ids, ranges, defaults, labels and stream layouts are the engine's
 * contract**, mirrored here exactly — fifteen parameters and six streams. If
 * a line here disagrees with `src/dsp/mod.rs`, this file is the one that is
 * wrong.
 *
 * **Everything the frame generators produce is invented, and it is the only
 * arithmetic on this page that is allowed to be.** All of the mathematics
 * belongs in Rust; the page renders published streams. What is below exists so
 * the panel has something to be designed against before the engine is
 * connected, and it is not a measurement of anything. No number produced here
 * may be quoted anywhere. The page knows: while the client is in offline mode
 * the readout is stamped DESIGN MODE and every figure on it is marked, so a
 * screenshot of this cannot be mistaken for a bench figure.
 */
import { getClient } from '@noob-audio-engineering/noob-vst-webgui-framework/vue';
import { CURVES, curveFor, transferAt } from '../curves.js';

/** The plain value of a parameter, read from the (offline) client at frame time. */
function plain(id, fallback = 0) {
  try {
    const p = getClient().param(id);
    return p ? p.plain : fallback;
  } catch {
    return fallback;
  }
}
/** The step index of a labelled parameter. */
const step = (id, fallback = 0) => Math.round(plain(id, fallback));

/**
 * Give every labelled parameter the range the plug-in publishes for it: 0 to
 * the number of steps less one. The framework's offline mock derives the
 * default's normalized position from `min` and `max`, and a labelled
 * parameter left without them is read as 0 to 1, so any default past the
 * first step would land on the last one.
 */
const stepped = (list) => list.map((p) => (p.labels && p.max == null ? { min: 0, max: p.labels.length - 1, ...p } : p));

const SAMPLE_RATE = 48000;
const NYQUIST = SAMPLE_RATE / 2;
/** `transfer`: 256 points over −1.5 … +1.5, so the fold's turn and the clipper's plateau land inside the picture. */
const TRANSFER_POINTS = 256;
const TRANSFER_RANGE = [-1.5, 1.5];
/** `color`: 129 log-spaced points from 20 Hz to 20 kHz. */
const COLOR_POINTS = 129;
const COLOR_RANGE = [20, 20000];
/** The spectra: 2048 bins in dB, a full-scale sine reading 0 dB. */
const FFT_SIZE = 4096;
const SPEC_BINS = 2048;
/**
 * Two numbers the panel prints, so they are the engine's to state and the
 * page reads them from meta rather than holding a copy. The DC filter is two
 * sections, not one: a biased shape rectifies, and the engine measured 39 %
 * of full scale of offset at maximum bias.
 */
const DC_CORNER_HZ = 5;
const DC_SECTIONS = 2;
const SHELF_HZ = 150;
/**
 * The measured floors of the two readings through a linear path. The harmonic
 * floor is the level below which the display cannot tell a working shaper
 * from a wire, which is why it belongs on the face rather than in a comment.
 */
const ALIAS_FLOOR_DB = -135.0;
const HARMONIC_FLOOR_DB = -105.6;
/** The oversampling factors behind the four labels. */
const OS_FACTOR = [2, 4, 8, 16];

const CURVE_LABELS = CURVES.map((c) => c.label);
/** The curve entry the `curve` parameter currently points at. */
const activeCurve = () => curveFor(CURVE_LABELS[step('curve', 2)] || CURVE_LABELS[2]);

/**
 * Invented alias behaviour, one row per curve: where the antialiased path sits
 * at a modest drive, and how far it drifts as the drive is wound up.
 *
 * The ordering across curves is the only part with reasoning behind it, and it
 * is the dossier's: a wavefolder is the worst case, a hard corner next, and a
 * smooth monotone curve the easiest. The magnitudes are made up.
 */
const ALIAS = {
  warm: { base: -104, drift: 0.06 },
  round: { base: -102, drift: 0.07 },
  soft: { base: -99, drift: 0.08 },
  clip: { base: -86, drift: 0.14 },
  fold: { base: -82, drift: 0.19 },
  gate: { base: -95, drift: 0.1 },
};
/** What each oversampling step is worth on top of the antiderivative scheme, in dB. Invented. */
const OS_GAIN = [0, 9, 16, 21];

export const offline = {
  name: 'noob-saturator',
  meta: {
    vendor: 'Noob Audio Engineering',
    version: 'dev',
    sample_rate: SAMPLE_RATE,
    standalone: true,
    dc_corner_hz: DC_CORNER_HZ,
    dc_sections: DC_SECTIONS,
    color_shelf_hz: SHELF_HZ,
  },

  params: stepped([
    // The shaper. Drive's range is Ableton's, deliberately: it is generous and
    // symmetric, there is nothing wrong with it, and matching it makes an A/B
    // honest.
    { id: 'drive', name: 'Drive', min: -36, max: 36, default: 0, unit: 'dB', group: 'shaper' },
    // Ours, and free under first-order antialiasing — `f(x+b)` has `F₀(x+b)`,
    // so an offset costs nothing at all. A percentage of the clipping point,
    // because a unit-free −1…1 on a front panel is the fault this plug-in is
    // built to complain about.
    { id: 'bias', name: 'Bias', min: -100, max: 100, default: 0, unit: '%', group: 'shaper' },
    { id: 'curve', name: 'Curve', labels: CURVE_LABELS, default: 2, group: 'shaper' },
    // Symmetric, unlike theirs, which only attenuates and so cannot put back
    // the level a heavy drive took.
    { id: 'output', name: 'Output', min: -36, max: 36, default: 0, unit: 'dB', group: 'shaper' },
    { id: 'mix', name: 'Dry/Wet', min: 0, max: 100, default: 100, unit: '%', group: 'shaper' },
    // Both laws, defaulting to the linear one so an A/B against a linear
    // crossfade elsewhere is comparable.
    { id: 'mix_law', name: 'Mix Law', labels: ['Linear', 'Equal Power'], default: 0, group: 'shaper' },

    // Colour: the pre-emphasis pair, applied forward before the shaper and
    // inverted after it. The topology is Ableton's and it is a good one; what
    // is ours is that the width is a stated Q — and it publishes as one, so a
    // generic host view reads "0.70 Q" outside our own editor too.
    { id: 'color_on', name: 'Color', toggle: true, default: 1, group: 'color' },
    { id: 'color_base', name: 'Color Base', min: -36, max: 36, default: 0, unit: 'dB', group: 'color' },
    { id: 'color_freq', name: 'Color Freq', min: 30, max: 18500, default: 1000, unit: 'Hz', taper: 'log', group: 'color' },
    { id: 'color_q', name: 'Colour Width', min: 0.1, max: 10, default: 0.7, unit: 'Q', taper: 'log', group: 'color' },
    { id: 'color_depth', name: 'Color Depth', min: -24, max: 24, default: 0, unit: 'dB', group: 'color' },

    // On by default, and on the panel rather than in a context menu. A DC
    // blocker in front of a nonlinearity is not a preference.
    { id: 'dc_block', name: 'Pre DC Filter', toggle: true, default: 1, group: 'output' },
    { id: 'clip_mode', name: 'Post Clip', labels: ['Off', 'Soft', 'Hard'], default: 0, group: 'output' },
    // Decibels below the ceiling, not a fraction: the same argument that put a
    // Q on the colour width.
    { id: 'clip_knee', name: 'Clip Knee', min: 0, max: 24, default: 6, unit: 'dB', group: 'output' },
    // Not a quality switch: every setting here is antialiased. It buys
    // headroom above what the antiderivative scheme already gives, and the
    // readout says what the setting you are holding reaches.
    { id: 'oversample', name: 'Oversample', labels: ['2x', '4x', '8x', '16x'], default: 1, group: 'output' },
  ]),

  streams: [
    { id: 'meter', name: 'Meter', kind: 'meter', capacity: 4, channels: 2, meta: { layout: 'in_l,in_r,out_l,out_r' } },
    /*
     * The one display Ableton does not have. Measured on the signal actually
     * passing rather than on a probe tone, so `confidence` says how periodic
     * that signal is and `f0_hz` is the fundamental the meter found. The
     * `units` array is explicit so nothing downstream ever has to infer one.
     */
    {
      id: 'alias',
      name: 'Alias',
      kind: 'raw',
      capacity: 4,
      channels: 1,
      meta: {
        layout: 'alias_db,f0_hz,confidence,harmonic_db',
        units: ['dB', 'Hz', 'ratio', 'dB'],
        mode: 'signal',
        alias_floor_db: ALIAS_FLOOR_DB,
        harmonic_floor_db: HARMONIC_FLOOR_DB,
      },
    },
    {
      id: 'transfer',
      name: 'Transfer',
      kind: 'curve',
      capacity: TRANSFER_POINTS,
      channels: 1,
      sticky: true,
      meta: { in_range: TRANSFER_RANGE, includes: 'drive,bias,curve,post-clip,output', excludes: 'color,mix' },
    },
    {
      id: 'color',
      name: 'Color curve',
      kind: 'curve',
      capacity: COLOR_POINTS,
      channels: 1,
      sticky: true,
      meta: { hz_range: COLOR_RANGE, spacing: 'log', unit: 'dB', direction: 'forward' },
    },
    { id: 'spec_in', name: 'Input spectrum', kind: 'spectrum', capacity: SPEC_BINS, channels: 1, meta: { sample_rate: SAMPLE_RATE, fft_size: FFT_SIZE, db: true } },
    { id: 'spec_out', name: 'Output spectrum', kind: 'spectrum', capacity: SPEC_BINS, channels: 1, meta: { sample_rate: SAMPLE_RATE, fft_size: FFT_SIZE, db: true } },
    /*
     * The dry/wet alignment. The two delays are equal by construction, so the
     * generator computes one and publishes it twice rather than inventing two
     * numbers that might differ. `kernel_frac` is the antialiasing kernel's
     * half-sample, deliberately not folded into the reported figure.
     */
    {
      id: 'latency',
      name: 'Alignment',
      kind: 'raw',
      capacity: 5,
      channels: 1,
      sticky: true,
      meta: { layout: 'wet_samples,dry_samples,reported_samples,sample_rate,kernel_frac' },
    },
  ],

  frames: {
    /*
     * The measurement, in two numbers that move in opposite directions.
     * `alias_db` barely shifts as the drive comes up; `harmonic_db` — the
     * wanted distortion, second order and above against the fundamental —
     * climbs steeply. That contrast is the whole argument and it needs no
     * counterfactual, because both numbers are of this device.
     *
     * The invented source wanders between a steady tone and something
     * percussive, and its fundamental wanders with it, so both states the
     * readout has to handle — low confidence, and a fundamental high enough
     * that every harmonic is above Nyquist — actually happen while the page is
     * being designed rather than only in a host.
     */
    alias: (t) => {
      const c = ALIAS[activeCurve().key] || ALIAS.soft;
      const drive = plain('drive');
      const hot = Math.max(0, drive);
      const os = OS_GAIN[step('oversample', 1)] ?? 9;
      const aliasDb = Math.max(
        ALIAS_FLOOR_DB,
        c.base - os + hot * c.drift + 0.5 * Math.sin(t * 1.7) + 0.25 * Math.sin(t * 5.3),
      );
      const confidence = Math.min(1, Math.max(0, 0.62 + 0.45 * Math.sin(t * 0.21)));
      /*
       * The fundamental climbs to 15 kHz and back on a slow cycle. Above
       * Nyquist/2 there is no harmonic left in band to measure, so the field
       * drops to its floor by construction — which is exactly the state the
       * panel has to explain rather than let a user read as a broken meter.
       */
      const f0 = 110 + 14890 * Math.max(0, Math.sin(t * 0.09)) ** 3;
      const bias = Math.abs(plain('bias')) / 100;
      const wanted = Math.min(-1.5, -60 + Math.max(0, drive + 6) * 1.15 + bias * 11);
      const harmonicDb = 2 * f0 >= NYQUIST ? HARMONIC_FLOOR_DB : Math.max(HARMONIC_FLOOR_DB, wanted);
      return [aliasDb, f0, confidence, harmonicDb];
    },

    latency: (() => {
      let last = -1;
      return () => {
        const i = step('oversample', 1);
        if (i === last) return null;
        last = i;
        const samples = 16 * (i + 1);
        return [samples, samples, samples, SAMPLE_RATE, 0.5];
      };
    })(),

    /*
     * The curve as the engine would draw it, republished when anything that
     * changes its shape changes — which includes the post clipper and its
     * knee, since the engine's stream declares them as included.
     */
    transfer: (() => {
      let last = '';
      return () => {
        const curve = activeCurve();
        const at = {
          driveDb: plain('drive'),
          biasPct: plain('bias'),
          outputDb: plain('output'),
          clipMode: step('clip_mode'),
          kneeDb: plain('clip_knee', 6),
        };
        const stamp = `${curve.key}:${at.driveDb.toFixed(2)}:${at.biasPct.toFixed(2)}:${at.outputDb.toFixed(2)}:${at.clipMode}:${at.kneeDb.toFixed(2)}`;
        if (stamp === last) return null;
        last = stamp;
        const f = transferAt(curve, at);
        const [lo, hi] = TRANSFER_RANGE;
        const out = new Float32Array(TRANSFER_POINTS);
        for (let i = 0; i < TRANSFER_POINTS; i++) out[i] = f(lo + ((hi - lo) * i) / (TRANSFER_POINTS - 1));
        return out;
      };
    })(),

    /*
     * The colour section's forward magnitude, log-spaced: a low shelf about
     * the published corner in series with a bell whose width follows the Q the
     * control states.
     */
    color: (() => {
      let last = '';
      return () => {
        const on = plain('color_on', 1) >= 0.5;
        const base = on ? plain('color_base') : 0;
        const depth = on ? plain('color_depth') : 0;
        const freq = plain('color_freq', 1000);
        const q = Math.max(0.1, plain('color_q', 0.7));
        const stamp = `${on}:${base.toFixed(2)}:${depth.toFixed(2)}:${freq.toFixed(1)}:${q.toFixed(3)}`;
        if (stamp === last) return null;
        last = stamp;
        const [lo, hi] = COLOR_RANGE;
        const ratio = hi / lo;
        const out = new Float32Array(COLOR_POINTS);
        for (let i = 0; i < COLOR_POINTS; i++) {
          const hz = lo * ratio ** (i / (COLOR_POINTS - 1));
          const shelf = base / (1 + (hz / SHELF_HZ) ** 2);
          const bell = depth * Math.exp(-((Math.log2(hz / freq) * q * 1.2) ** 2));
          out[i] = shelf + bell;
        }
        return out;
      };
    })(),

    spec_in: () => spectrum(false),
    spec_out: () => spectrum(true),

    /*
     * A bass-heavy loop with a syncopated accent, so the operating band on the
     * transfer display moves the way real material does instead of breathing
     * evenly. Linear peak, 1.0 = 0 dBFS, two channels in and two out.
     */
    meter: (t) => {
      const beat = Math.max(0, Math.sin(t * 2 * Math.PI * 1.85)) ** 6;
      const off = Math.max(0, Math.sin(t * 2 * Math.PI * 1.85 - 2.1)) ** 10;
      const inL = 0.12 + 0.62 * beat + 0.25 * off;
      const inR = inL * 0.94;
      const f = transferAt(activeCurve(), {
        driveDb: plain('drive'),
        biasPct: plain('bias'),
        outputDb: plain('output'),
        clipMode: step('clip_mode'),
        kneeDb: plain('clip_knee', 6),
      });
      const mix = plain('mix', 100) / 100;
      const law = step('mix_law');
      const [dryG, wetG] = law === 1 ? [Math.cos((mix * Math.PI) / 2), Math.sin((mix * Math.PI) / 2)] : [1 - mix, mix];
      const wet = (v) => Math.min(4, Math.abs(dryG * v + wetG * f(v)));
      return [inL, inR, wet(inL), wet(inR)];
    },
  },
  timeoutMs: 1200,
};

/*
 * A plausible spectrum: a tilted noise floor with a bass fundamental and a run
 * of narrow partials over it, drawn the way an analyser draws them — a line
 * per harmonic with a two-bin skirt, not a plateau. The output copy adds the
 * harmonic series the shaper would have made, growing with drive, and stamps
 * the colour section's shape on it, since what that display is there to show is
 * which bands the forward filter sends into the nonlinear part of the curve.
 *
 * Invented, like everything else in this file.
 */
const BIN_HZ = SAMPLE_RATE / FFT_SIZE;
const F0 = 110;
/** The source's own partials, in dB relative to full scale. */
const PARTIALS = [-10, -16, -21, -26, -31, -36, -41, -46];
let phase = 0;

function spectrum(wet) {
  phase += 0.06;
  const drive = plain('drive');
  const on = plain('color_on', 1) >= 0.5;
  const base = on ? plain('color_base') : 0;
  const depth = on ? plain('color_depth') : 0;
  const freq = plain('color_freq', 1000);
  const q = Math.max(0.1, plain('color_q', 0.7));
  const out = new Float32Array(SPEC_BINS);

  for (let k = 1; k < SPEC_BINS; k++) {
    const hz = k * BIN_HZ;
    out[k] = -86 - 2.6 * Math.log2(hz / 100) + 2.5 * Math.sin(k * 0.15 + phase) + 1.8 * Math.sin(k * 0.031 - phase * 0.7);
  }

  /** Lay one narrow line on the trace, with the skirt an analyser's window would give it. */
  const line = (hz, db) => {
    const c = hz / BIN_HZ;
    for (let k = Math.max(1, Math.floor(c) - 2); k <= Math.min(SPEC_BINS - 1, Math.ceil(c) + 2); k++) {
      const d = k - c;
      const v = db - 22 * d * d;
      if (v > out[k]) out[k] = v;
    }
  };
  for (let n = 1; n <= PARTIALS.length; n++) line(n * F0, PARTIALS[n - 1] + 1.5 * Math.sin(phase + n));

  if (wet) {
    const hot = Math.max(0, drive + 6);
    for (let n = 2; n <= 200; n++) {
      const hz = n * F0;
      if (hz > NYQUIST - BIN_HZ) break;
      line(hz, -34 + hot * 0.7 - 5.5 * Math.log2(n) + 1.2 * Math.sin(phase * 1.7 + n * 0.9));
    }
    for (let k = 1; k < SPEC_BINS; k++) {
      const hz = k * BIN_HZ;
      const shelf = base / (1 + (hz / SHELF_HZ) ** 2);
      const bell = depth * Math.exp(-((Math.log2(hz / freq) * q * 1.2) ** 2));
      out[k] += shelf + bell;
    }
  }
  for (let k = 1; k < SPEC_BINS; k++) out[k] = Math.max(-130, Math.min(0, out[k]));
  out[0] = out[1];
  return out;
}
