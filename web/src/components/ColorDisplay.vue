<script setup>
/**
 * The colour curve: the pre-emphasis applied before the shaper, and its
 * inverse applied after it.
 *
 * **The display is Ableton's idea.** Live 12.1 added a colour-curve view for
 * this section, and it is the right expanded view for a device whose colour
 * controls are frequency-dependent drive. We adopt it and say so rather than
 * pretending we thought of it.
 *
 * What the section actually does, which the Live 11 manual never says and the
 * Live 12 manual says exactly once: the curve is applied *before* the shaper
 * and again *inverted* after it. It is not a tone control — the forward
 * filter decides which bands reach the nonlinear part of the curve and the
 * inverse puts the spectrum back, so what survives is the distortion the
 * emphasis created. Both halves are drawn, the forward solid and the inverse
 * dashed, because a reader shown only one will read it as an EQ.
 *
 * The forward curve is the engine's, from the sticky `color` stream: 129
 * points in dB, log-spaced over the range the stream's meta declares. On a
 * build that does not publish it yet the page computes the pair from the
 * parameters instead, and says so.
 *
 * **The inverse trace is the algebraic negation of the forward one.** That
 * the engine's pair actually nulls is a claim about the Rust half and it is
 * tested there against a signal. Nothing on this page measures it, and this
 * display says so rather than implying it has checked.
 *
 * **What is missing, and it is missing from the contract rather than from
 * here.** Ableton draw input and output spectra behind their colour curve and
 * that is the better half of their idea; the frozen stream set has no
 * spectra, so the traces are absent and the caption says why. The drawing
 * code is still behind `hasStream`, so the moment `spec_in` and `spec_out`
 * exist they appear with no further work.
 *
 * The width control states its Q, which is the improvement over Ableton's
 * unit-free 0…1: a filter width nobody can reason about is a defect.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { bandCoefs, bandDb } from '@noob-audio-engineering/noob-vst-webgui-framework/components';
import { useStreamFrame } from '@noob-audio-engineering/noob-vst-webgui-framework/vue';
import { hasStream, useColorCurve, useNoobVstWebguiFramework, useSat } from '../composables/useSaturator.js';

const sat = useSat();
const { manifest } = useNoobVstWebguiFramework();
const colorCurve = useColorCurve();
const hasIn = hasStream('spec_in');
const hasOut = hasStream('spec_out');
const specIn = hasIn ? useStreamFrame('spec_in') : ref(null);
const specOut = hasOut ? useStreamFrame('spec_out') : ref(null);

const MIN_HZ = 20;
const MAX_HZ = 20000;
/** The curve's own vertical range. Base reaches ±36 dB and depth ±24, so the sum can leave the box; it is clipped and the caption says the range. */
const CURVE_DB = 40;
/** The spectra's range, if a build ever publishes any. */
const SPEC_DB = [-108, 0];

const sampleRate = computed(() => manifest.value?.meta?.sample_rate || 48000);
/**
 * The shelf's corner, which is not a control on this device (Ableton has none
 * either — theirs is "very low frequencies"). It is a constant, so the engine
 * publishes it and the display prints it rather than leaving the reader to
 * guess what Base acts on.
 */
const shelfHz = computed(() => manifest.value?.meta?.color_shelf_hz || 120);

const on = computed(() => !sat.colorOn || sat.colorOn.on);
const base = computed(() => (sat.colorBase ? sat.colorBase.plain : 0));
const depth = computed(() => (sat.colorDepth ? sat.colorDepth.plain : 0));
const freq = computed(() => (sat.colorFreq ? sat.colorFreq.plain : 1000));
const q = computed(() => (sat.colorQ ? sat.colorQ.plain : 0.7));

/**
 * The forward magnitude at a frequency: read off the engine's curve when
 * there is one, interpolating between its log-spaced points, and computed
 * from the parameters when there is not.
 */
const forwardDb = computed(() => {
  const pts = colorCurve.points;
  if (pts && pts.length > 1) {
    const [lo, hi] = colorCurve.range;
    const lgLo = Math.log(lo);
    const lgSpan = Math.log(hi) - lgLo;
    const last = pts.length - 1;
    return (hz) => {
      const t = ((Math.log(Math.max(1, hz)) - lgLo) / lgSpan) * last;
      const i = Math.max(0, Math.min(last, Math.floor(t)));
      const j = Math.min(last, i + 1);
      return pts[i] + (pts[j] - pts[i]) * (t - i);
    };
  }
  const sr = sampleRate.value;
  const shelf = bandCoefs('lowshelf', shelfHz.value, on.value ? base.value : 0, 0.707, 1, sr);
  const bell = bandCoefs('peak', freq.value, on.value ? depth.value : 0, q.value, 1, sr);
  return (hz) => bandDb(shelf, hz, sr) + bandDb(bell, hz, sr);
});

const host = ref(null);
const canvas = ref(null);
let raf = 0;
/** Per-bin smoothed dB, one array per spectrum, so a trace decays like an analyser instead of strobing. */
const held = { in: null, out: null };

const css = (name, dflt) => {
  const el = host.value;
  if (!el) return dflt;
  const v = getComputedStyle(el).getPropertyValue(name).trim();
  return v || dflt;
};

/** The smoothed copy of a spectrum frame: instant attack, a release in dB per frame. */
function smooth(key, frame) {
  if (!frame || !frame.length) return held[key];
  let h = held[key];
  if (!h || h.length !== frame.length) {
    h = new Float32Array(frame.length);
    h.fill(-140);
    held[key] = h;
  }
  for (let i = 0; i < frame.length; i++) {
    const v = frame[i];
    h[i] = v > h[i] ? v : h[i] + (v - h[i]) * 0.16;
  }
  return h;
}

function draw() {
  raf = requestAnimationFrame(draw);
  const cv = canvas.value;
  if (!cv) return;
  const dpr = window.devicePixelRatio || 1;
  const w = cv.clientWidth;
  const h = cv.clientHeight;
  if (w < 8 || h < 8) return;
  if (cv.width !== Math.round(w * dpr) || cv.height !== Math.round(h * dpr)) {
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(h * dpr);
  }
  const g = cv.getContext('2d');
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, w, h);

  const padL = hasIn || hasOut ? 30 : 12;
  const padR = 34;
  const padT = 10;
  const padB = 18;
  const pw = Math.max(10, w - padL - padR);
  const ph = Math.max(10, h - padT - padB);
  const lgMin = Math.log(MIN_HZ);
  const lgSpan = Math.log(MAX_HZ) - lgMin;
  const xFor = (hz) => padL + ((Math.log(Math.max(1, hz)) - lgMin) / lgSpan) * pw;
  const freqFor = (x) => Math.exp(lgMin + ((x - padL) / pw) * lgSpan);
  const yCurve = (db) => padT + ph / 2 - (db / CURVE_DB) * (ph / 2);
  const ySpec = (db) => padT + ph - ((db - SPEC_DB[0]) / (SPEC_DB[1] - SPEC_DB[0])) * ph;

  const grid = css('--sat-grid', 'rgba(255,255,255,0.07)');
  const gridStrong = css('--sat-grid-strong', 'rgba(255,255,255,0.16)');
  const dim = css('--sat-text-dim', 'rgba(226,232,222,0.42)');

  // decade grid, with the 1-2-5 marks named
  g.lineWidth = 1;
  g.font = '9px ui-monospace, Consolas, monospace';
  g.textBaseline = 'alphabetic';
  for (const hz of [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]) {
    const x = Math.round(xFor(hz)) + 0.5;
    g.strokeStyle = hz === 1000 ? gridStrong : grid;
    g.beginPath();
    g.moveTo(x, padT);
    g.lineTo(x, padT + ph);
    g.stroke();
    g.fillStyle = dim;
    const label = hz >= 1000 ? `${hz / 1000}k` : `${hz}`;
    g.fillText(label, x - (hz === 20000 ? 14 : 5), padT + ph + 12);
  }
  // the curve's own horizontal grid, every 12 dB, named on the right
  for (let db = -36; db <= 36; db += 12) {
    const y = Math.round(yCurve(db)) + 0.5;
    g.strokeStyle = db === 0 ? gridStrong : grid;
    g.beginPath();
    g.moveTo(padL, y);
    g.lineTo(padL + pw, y);
    g.stroke();
    g.fillStyle = dim;
    g.fillText(db > 0 ? `+${db}` : `${db}`, padL + pw + 5, y + 3);
  }
  // and the spectra's, on the left, only where there are spectra to scale
  if (hasIn || hasOut) {
    g.fillStyle = dim;
    for (let db = -24; db >= -108; db -= 24) g.fillText(`${db}`, 4, ySpec(db) + 3);
  }

  /*
   * Output first and filled, input over it as a bare line. Drawn the other
   * way round the input vanishes: the output is the input plus what the
   * shaper made, so it is never below it, and a filled output painted last
   * covers the trace the display exists to compare against.
   */
  const drawSpec = (data, fill, stroke) => {
    if (!data || data.length < 4) return;
    const bins = data.length;
    const binHz = sampleRate.value / ((bins - 1) * 2);
    g.beginPath();
    let started = false;
    for (let x = 0; x <= pw; x++) {
      const f0 = freqFor(padL + x - 0.5);
      const f1 = freqFor(padL + x + 0.5);
      const k0 = Math.max(1, Math.floor(f0 / binHz));
      const k1 = Math.min(bins - 1, Math.max(k0, Math.ceil(f1 / binHz)));
      let v = -140;
      for (let k = k0; k <= k1; k++) if (data[k] > v) v = data[k];
      const y = Math.max(padT, Math.min(padT + ph, ySpec(v)));
      if (!started) {
        g.moveTo(padL + x, y);
        started = true;
      } else g.lineTo(padL + x, y);
    }
    if (fill) {
      g.save();
      g.lineTo(padL + pw, padT + ph);
      g.lineTo(padL, padT + ph);
      g.closePath();
      g.fillStyle = fill;
      g.fill();
      g.restore();
    }
    if (stroke) {
      g.lineWidth = 1.2;
      g.strokeStyle = stroke;
      g.stroke();
    }
  };
  drawSpec(smooth('out', specOut.value), css('--sat-spec-out-fill', 'rgba(94,226,160,0.10)'), css('--sat-spec-out', 'rgba(94,226,160,0.6)'));
  drawSpec(smooth('in', specIn.value), null, css('--sat-spec-in', 'rgba(150,170,190,0.55)'));

  // the emphasis pair: forward solid, inverse dashed
  const f = forwardDb.value;
  const path = (sign) => {
    g.beginPath();
    for (let x = 0; x <= pw; x++) {
      const db = sign * f(freqFor(padL + x));
      const y = Math.max(padT - 40, Math.min(padT + ph + 40, yCurve(db)));
      if (x === 0) g.moveTo(padL + x, y);
      else g.lineTo(padL + x, y);
    }
  };
  g.save();
  g.beginPath();
  g.rect(padL, padT, pw, ph);
  g.clip();
  g.setLineDash([4, 4]);
  g.lineWidth = 1.4;
  g.strokeStyle = css('--sat-inverse', 'rgba(217,164,65,0.45)');
  path(-1);
  g.stroke();
  g.setLineDash([]);
  g.lineWidth = 2.2;
  g.strokeStyle = on.value ? css('--sat-brass', '#d9a441') : dim;
  path(1);
  g.stroke();
  // where the bell is centred
  if (on.value) {
    const x = xFor(freq.value);
    const y = yCurve(Math.max(-CURVE_DB, Math.min(CURVE_DB, f(freq.value))));
    g.fillStyle = css('--sat-brass', '#d9a441');
    g.beginPath();
    g.arc(x, y, 3.4, 0, Math.PI * 2);
    g.fill();
  }
  g.restore();
}

onMounted(() => {
  raf = requestAnimationFrame(draw);
});
onBeforeUnmount(() => cancelAnimationFrame(raf));
</script>

<template>
  <div ref="host" class="colour">
    <div class="colour__plot">
      <canvas ref="canvas" class="colour__canvas" />
      <div class="colour__key">
        <span class="colour__k colour__k--fwd">forward, pre-shaper</span>
        <span class="colour__k colour__k--inv">inverse, post-shaper</span>
        <span v-if="hasIn" class="colour__k colour__k--in">in</span>
        <span v-if="hasOut" class="colour__k colour__k--out">out</span>
        <span v-if="!on" class="colour__k colour__k--off">colour off</span>
      </div>
    </div>
    <div class="colour__side">
      <div class="colour__head">Frequency-dependent drive, not a tone control</div>
      <p class="colour__note">
        The curve is applied before the shaper and again, inverted, after it. The forward half decides which
        bands reach the nonlinear part of the curve; the inverse puts the spectrum back. What survives is the
        distortion the emphasis created.
      </p>
      <dl class="colour__facts tabular">
        <dt>shelf</dt>
        <dd>low shelf at {{ shelfHz >= 1000 ? (shelfHz / 1000).toFixed(1) + ' kHz' : Math.round(shelfHz) + ' Hz' }}, {{ base >= 0 ? '+' : '' }}{{ base.toFixed(1) }} dB</dd>
        <dt>bell</dt>
        <dd>
          {{ freq >= 1000 ? (freq / 1000).toFixed(2) + ' kHz' : Math.round(freq) + ' Hz' }},
          Q {{ q.toFixed(2) }}, {{ depth >= 0 ? '+' : '' }}{{ depth.toFixed(1) }} dB
        </dd>
        <dt>width</dt>
        <dd>stated as Q, on the control</dd>
      </dl>
      <p class="colour__caption">
        The dashed trace is the algebraic negation of the solid one. That the engine's pair actually nulls is
        tested in the Rust half against a signal; this display has not measured it and does not claim to.
        Curve axis ±{{ CURVE_DB }} dB on the right.
      </p>
      <p v-if="!colorCurve.has" class="colour__warn">
        This build publishes no colour-curve stream, so the pair is the page's own filter arithmetic rather
        than the engine's.
      </p>
      <p v-if="!hasIn || !hasOut" class="colour__gap">
        Ableton draw input and output spectra behind this curve, which is the better half of their idea. The
        engine's stream set has none, so there is nothing to draw behind it here.
      </p>
    </div>
  </div>
</template>
