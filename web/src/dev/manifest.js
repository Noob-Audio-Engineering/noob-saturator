/**
 * Design-time manifest for Noob Saturator: what the plug-in publishes,
 * described up front so the page can be built and looked at before the DSP
 * exists. Only development builds load it (see `main.js`), and the client
 * hands over to the real server the moment `/ws` answers.
 *
 * **Ids, ranges, defaults, labels and stream layouts are the engine's frozen
 * contract**, mirrored here exactly — fifteen parameters and five streams. If
 * a line here disagrees with `src/dsp/mod.rs`, this file is the one that is
 * wrong.
 *
 * **Everything the frame generators produce is invented.** It is shaped to
 * move the way the real thing should move so the panel can be designed
 * against it — the alias figure holds while the harmonic figure climbs with
 * drive, the confidence falls when the source stops being a tone, the
 * transfer curve bends. None of it is a measurement of anything and no number
 * here may be quoted anywhere. The page knows: while the client is in offline
 * mode the readout is stamped DESIGN MODE and every figure on it is marked,
 * so a screenshot of this cannot be mistaken for a bench figure.
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
/** `transfer`: 257 points over −1 … +1, the engine's own length and range. */
const TRANSFER_POINTS = 257;
const TRANSFER_RANGE = [-1, 1];
/** `color`: 129 log-spaced points from 20 Hz to 20 kHz, the engine's own. */
const COLOR_POINTS = 129;
const COLOR_RANGE = [20, 20000];
/** The DC blocker's corner and the colour shelf's, both printed on the panel and therefore the engine's to state. */
const DC_CORNER_HZ = 10;
const SHELF_HZ = 120;
/** The oversampling factors behind the four labels, so `align` can publish the real one. */
const OS_FACTOR = [2, 4, 8, 16];

const CURVE_LABELS = CURVES.map((c) => c.label);
/** The curve entry the `curve` parameter currently points at. */
const activeCurve = () => curveFor(CURVE_LABELS[step('curve', 2)] || CURVE_LABELS[2]);

/**
 * Invented alias behaviour, one row per curve: where the antialiased path
 * sits at a modest drive, and how far it drifts as the drive is wound up.
 *
 * The ordering across curves is the only part of this with reasoning behind
 * it, and it is the dossier's: a wavefolder is the worst case, a hard corner
 * next, and a smooth monotone curve the easiest. The magnitudes are made up.
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
    // Two constants the panel prints, so they must come from the engine. These
    // are placeholders and the real manifest replaces them on connection.
    dc_corner_hz: DC_CORNER_HZ,
    color_shelf_hz: SHELF_HZ,
  },

  params: stepped([
    // The shaper. Drive's range is Ableton's, deliberately: it is generous and
    // symmetric, there is nothing wrong with it, and matching it makes an A/B
    // honest.
    { id: 'drive', name: 'Drive', min: -36, max: 36, default: 0, unit: 'dB', group: 'shaper' },
    // Ours, and free under first-order antialiasing — `f(x+b)` has `F₁(x+b)`,
    // so an offset costs nothing at all. Asymmetry is what makes a saturator
    // sound like a valve rather than a clipper.
    { id: 'bias', name: 'Bias', min: -1, max: 1, default: 0, group: 'shaper' },
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
    // is ours is that the width is a stated Q.
    { id: 'color_on', name: 'Color', toggle: true, default: 1, group: 'color' },
    { id: 'color_base', name: 'Color Base', min: -36, max: 36, default: 0, unit: 'dB', group: 'color' },
    { id: 'color_freq', name: 'Color Freq', min: 30, max: 18500, default: 1000, unit: 'Hz', taper: 'log', group: 'color' },
    { id: 'color_q', name: 'Color Q', min: 0.1, max: 10, default: 0.7, taper: 'log', group: 'color' },
    { id: 'color_depth', name: 'Color Depth', min: -24, max: 24, default: 0, unit: 'dB', group: 'color' },

    // On by default, and on the panel rather than in a context menu. A DC
    // blocker in front of a nonlinearity is not a preference.
    { id: 'dc_block', name: 'Pre DC Filter', toggle: true, default: 1, group: 'output' },
    { id: 'clip_mode', name: 'Post Clip', labels: ['Off', 'Soft', 'Hard'], default: 0, group: 'output' },
    { id: 'clip_knee', name: 'Clip Knee', min: 0, max: 1, default: 0.5, group: 'output' },
    // Not a quality switch: every setting here is antialiased. It buys
    // headroom above what the antiderivative scheme already gives, and the
    // readout says what the setting you are holding reaches.
    { id: 'oversample', name: 'Oversample', labels: ['2x', '4x', '8x', '16x'], default: 1, group: 'output' },
  ]),

  streams: [
    { id: 'meter', name: 'Meter', kind: 'meter', capacity: 4, channels: 2, meta: { layout: 'in_l,in_r,out_l,out_r' } },
    // The one display Ableton does not have. `confidence` says how periodic
    // the input is, and the readout is only meaningful near one.
    { id: 'alias', name: 'Alias', kind: 'raw', capacity: 4, channels: 1, meta: { layout: 'alias_db,f0_hz,confidence,harmonics' } },
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
    { id: 'align', name: 'Alignment', kind: 'raw', capacity: 4, channels: 1, meta: { layout: 'wet_delay_samples,dry_delay_samples,oversample_factor,latency_ms' } },
  ],

  frames: {
    /*
     * The measurement, in two numbers that move in opposite directions.
     * `alias_db` barely shifts as the drive comes up; `harmonics` — the
     * wanted distortion, relative to the fundamental — climbs steeply. That
     * contrast is the whole argument and it needs no counterfactual, because
     * both numbers are measurements of this device.
     *
     * `confidence` follows a made-up source that drifts between a steady tone
     * and something percussive, so the greyed-out state the readout has to
     * handle actually happens while the page is being designed rather than
     * only in a host.
     */
    alias: (t) => {
      const c = ALIAS[activeCurve().key] || ALIAS.soft;
      const drive = plain('drive');
      const hot = Math.max(0, drive);
      const os = OS_GAIN[step('oversample', 1)] ?? 9;
      const aliasDb = Math.max(-140, c.base - os + hot * c.drift + 0.5 * Math.sin(t * 1.7) + 0.25 * Math.sin(t * 5.3));
      // the source wanders in and out of being a tone, on a slow cycle
      const confidence = Math.min(1, Math.max(0, 0.62 + 0.45 * Math.sin(t * 0.21)));
      // harmonic energy relative to the fundamental: rises with drive, and
      // with bias, which adds the even orders an offset always makes
      const bias = Math.abs(plain('bias'));
      const harmonics = Math.min(-1.5, -60 + Math.max(0, drive + 6) * 1.15 + bias * 11);
      const f0 = 110 * (1 + 0.4 * Math.max(0, Math.sin(t * 0.13)));
      return [aliasDb, f0, confidence, harmonics];
    },

    /*
     * Sticky, republished only when the oversampler moves, because that is
     * the only thing that changes it. The two delays are equal by
     * construction — that is the claim — so the generator computes one and
     * publishes it twice rather than inventing two numbers that might differ.
     */
    align: (() => {
      let last = -1;
      return () => {
        const i = step('oversample', 1);
        if (i === last) return null;
        last = i;
        const factor = OS_FACTOR[i] ?? 4;
        const samples = 16 * (i + 1);
        return [samples, samples, factor, (samples / SAMPLE_RATE) * 1000];
      };
    })(),

    /*
     * The curve as the engine would draw it, republished when anything that
     * changes its shape changes — which now includes the post clipper, since
     * the engine's stream declares it as included.
     */
    transfer: (() => {
      let last = '';
      return () => {
        const curve = activeCurve();
        const at = {
          driveDb: plain('drive'),
          bias: plain('bias'),
          outputDb: plain('output'),
          clipMode: step('clip_mode'),
          knee: plain('clip_knee', 0.5),
        };
        const stamp = `${curve.key}:${at.driveDb.toFixed(2)}:${at.bias.toFixed(3)}:${at.outputDb.toFixed(2)}:${at.clipMode}:${at.knee.toFixed(3)}`;
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
     * The colour section's forward magnitude, log-spaced. A low shelf in
     * series with a peaking bell, computed here the way the engine computes
     * it in the audio path; the page draws the inverse as the negation of
     * whatever arrives.
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
          // a first-order low shelf about the published corner — full lift at
          // DC, half of it at the corner, gone above — and a bell whose width
          // follows the Q the control states
          const shelf = base / (1 + (hz / SHELF_HZ) ** 2);
          const bell = depth * Math.exp(-((Math.log2(hz / freq) * q * 1.2) ** 2));
          out[i] = shelf + bell;
        }
        return out;
      };
    })(),

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
        bias: plain('bias'),
        outputDb: plain('output'),
        clipMode: step('clip_mode'),
        knee: plain('clip_knee', 0.5),
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
