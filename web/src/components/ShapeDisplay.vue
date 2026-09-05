<script setup>
/**
 * The transfer curve, with the signal drawn live against it and the equation
 * printed beside it.
 *
 * **The signal on the curve is Ableton's idea and they did it first.** They
 * added it to Saturator in Live 12.1, having not had it in Live 11, and it
 * is the right answer to "what should a saturator's display show": the
 * shape, and where on the shape the music currently is. We adopt it and say
 * so, here and in `web/README.md`. What is ours is the equation, because no
 * Ableton document prints one for any curve in any device.
 *
 * The plot is square and both axes carry the same range, so the dashed unity
 * line is a true 45 degrees. A transfer plot whose unity line is not
 * diagonal is misreading the one thing it exists to show, and the square is
 * the whole of the fix.
 *
 * The curve comes from the engine's sticky `transfer` stream when there is
 * one, and is computed here from `curves.js` when there is not, so the panel
 * is never blank while the DSP half is being written. The lit segment is the
 * span the input peak currently reaches, from the `meter` stream.
 *
 * Colour and mix are excluded from the plot, and the caption says so:
 * colour is frequency-dependent and this display has no frequency axis, and
 * a mix is not a shape.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { getClient, hasStream, useSat } from '../composables/useSaturator.js';
import { useStreamFrame } from '@noob-audio-engineering/noob-vst-webgui-framework/vue';
import { transferAt } from '../curves.js';

const sat = useSat();
const hasTransfer = hasStream('transfer');
const hasMeter = hasStream('meter');
const transfer = hasTransfer ? useStreamFrame('transfer') : ref(null);
const meter = hasMeter ? useStreamFrame('meter') : ref(null);

/** The input span the plot covers, from the stream's own meta where there is one. */
const range = computed(() => {
  if (hasTransfer) {
    const m = getClient().stream('transfer').meta || {};
    if (Array.isArray(m.in_range) && m.in_range.length === 2) return m.in_range;
  }
  return [-1, 1];
});

/** The curve as `y` values over `range`: the engine's, or this page's own while there is no engine. */
const points = computed(() => {
  if (transfer.value && transfer.value.length > 1) return transfer.value;
  const f = transferAt(sat.shape.value, {
    driveDb: sat.drive ? sat.drive.plain : 0,
    bias: sat.bias ? sat.bias.plain : 0,
    outputDb: sat.output ? sat.output.plain : 0,
    clipMode: sat.clipMode ? sat.clipMode.index : 0,
    knee: sat.clipKnee ? sat.clipKnee.plain : 0.5,
  });
  const [lo, hi] = range.value;
  const n = 257;
  const out = new Float32Array(n);
  for (let i = 0; i < n; i++) out[i] = f(lo + ((hi - lo) * i) / (n - 1));
  return out;
});

const host = ref(null);
const canvas = ref(null);
let raf = 0;
/**
 * The input peak follows the meter with a decay, so the lit band reads as a
 * level rather than flickering between blocks. The frozen meter layout is
 * `in_l, in_r, out_l, out_r` as linear peaks, so the band is the louder of
 * the two input channels; there is no RMS in the stream and the display does
 * not invent one.
 */
let peak = 0;

const css = (name, dflt) => {
  const el = host.value;
  if (!el) return dflt;
  const v = getComputedStyle(el).getPropertyValue(name).trim();
  return v || dflt;
};

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

  // ballistics: instant attack, a slow release, so the band reads as a level
  const targetPeak = meter.value ? Math.max(Math.abs(meter.value[0]), Math.abs(meter.value[1])) : 0;
  peak = targetPeak > peak ? targetPeak : peak + (targetPeak - peak) * 0.06;

  const pad = 22;
  const x0 = pad;
  const y0 = pad;
  const side = Math.min(w, h) - pad * 2;
  const [lo, hi] = range.value;
  const span = hi - lo || 1;
  const px = (v) => x0 + ((v - lo) / span) * side;
  const py = (v) => y0 + side - ((v - lo) / span) * side;

  const grid = css('--sat-grid', 'rgba(255,255,255,0.07)');
  const gridStrong = css('--sat-grid-strong', 'rgba(255,255,255,0.16)');
  const trace = css('--sat-trace', '#5ee2a0');
  const dim = css('--sat-text-dim', 'rgba(226,232,222,0.42)');

  // grid, at quarter units
  g.lineWidth = 1;
  g.strokeStyle = grid;
  g.beginPath();
  for (let v = Math.ceil(lo * 4) / 4; v <= hi + 1e-6; v += 0.25) {
    const gx = Math.round(px(v)) + 0.5;
    const gy = Math.round(py(v)) + 0.5;
    g.moveTo(gx, y0);
    g.lineTo(gx, y0 + side);
    g.moveTo(x0, gy);
    g.lineTo(x0 + side, gy);
  }
  g.stroke();
  // the axes through zero
  g.strokeStyle = gridStrong;
  g.beginPath();
  g.moveTo(Math.round(px(0)) + 0.5, y0);
  g.lineTo(Math.round(px(0)) + 0.5, y0 + side);
  g.moveTo(x0, Math.round(py(0)) + 0.5);
  g.lineTo(x0 + side, Math.round(py(0)) + 0.5);
  g.stroke();
  // unity, dashed, corner to corner
  g.save();
  g.setLineDash([3, 4]);
  g.strokeStyle = dim;
  g.beginPath();
  g.moveTo(px(lo), py(lo));
  g.lineTo(px(hi), py(hi));
  g.stroke();
  g.restore();

  const ys = points.value;
  const n = ys.length;
  const xAt = (i) => lo + (span * i) / (n - 1);

  // the curve, dim over its whole length
  g.lineWidth = 1.6;
  g.strokeStyle = css('--sat-trace-dim', 'rgba(94,226,160,0.35)');
  g.beginPath();
  for (let i = 0; i < n; i++) {
    const X = px(xAt(i));
    const Y = py(ys[i]);
    if (i === 0) g.moveTo(X, Y);
    else g.lineTo(X, Y);
  }
  g.stroke();

  // the span the signal is actually using, lit, with a fill under it
  const band = Math.min(Math.abs(hi), Math.max(peak, 1e-4));
  const iFrom = Math.max(0, Math.floor(((-band - lo) / span) * (n - 1)));
  const iTo = Math.min(n - 1, Math.ceil(((band - lo) / span) * (n - 1)));
  if (iTo > iFrom) {
    g.save();
    g.beginPath();
    g.moveTo(px(xAt(iFrom)), py(0));
    for (let i = iFrom; i <= iTo; i++) g.lineTo(px(xAt(i)), py(ys[i]));
    g.lineTo(px(xAt(iTo)), py(0));
    g.closePath();
    g.fillStyle = css('--sat-trace-fill', 'rgba(94,226,160,0.10)');
    g.fill();
    g.restore();

    g.lineWidth = 2.4;
    g.strokeStyle = trace;
    g.beginPath();
    for (let i = iFrom; i <= iTo; i++) {
      const X = px(xAt(i));
      const Y = py(ys[i]);
      if (i === iFrom) g.moveTo(X, Y);
      else g.lineTo(X, Y);
    }
    g.stroke();
  }

  // where the peak sits, both ends of the lit span
  const dot = (v) => {
    const i = Math.max(0, Math.min(n - 1, Math.round(((v - lo) / span) * (n - 1))));
    g.beginPath();
    g.arc(px(xAt(i)), py(ys[i]), 3.1, 0, Math.PI * 2);
    g.fill();
  };
  if (peak > 1e-3) {
    g.fillStyle = trace;
    dot(band);
    dot(-band);
  }

  // Axis captions and the four ends. The plot is the unit square, so ±1 is
  // its own border and needs naming rather than marking: full scale in, full
  // scale out, and every curve's ceiling.
  g.fillStyle = dim;
  g.font = '9px ui-monospace, Consolas, monospace';
  g.textBaseline = 'alphabetic';
  g.fillText('in', x0 + side - 12, y0 + side + 14);
  g.save();
  g.translate(x0 - 8, y0 + 10);
  g.rotate(-Math.PI / 2);
  g.fillText('out', -18, 0);
  g.restore();
  g.fillText('+1', x0 + side - 15, py(0) + 12);
  g.fillText('−1', x0 + 3, py(0) + 12);
  g.fillText('+1', px(0) + 5, y0 + 10);
  g.fillText('−1', px(0) + 5, y0 + side - 3);
}

/*
 * One animation loop, which also picks up size changes: the canvas is
 * measured every frame against its backing store, so a resize needs no
 * observer of its own. The loop has to run anyway — the lit span follows the
 * meter and moves between stream frames.
 */
onMounted(() => {
  raf = requestAnimationFrame(draw);
});
onBeforeUnmount(() => cancelAnimationFrame(raf));

const shape = computed(() => sat.shape.value);
</script>

<template>
  <div ref="host" class="shape">
    <div class="shape__plot">
      <canvas ref="canvas" class="shape__canvas" />
    </div>
    <div class="shape__side">
      <div class="shape__name">
        <span class="shape__curve">{{ shape.label || '—' }}</span>
        <span class="shape__sub">{{ shape.sub }}</span>
      </div>
      <!--
        The equations are lines rather than one string, because Clip is a
        one-parameter knee family and takes two of them to write honestly.
        Compressing it onto one line would have meant either a horizontal
        scrollbar through an equation or a notation nobody reads.
      -->
      <div class="shape__eq">
        <template v-if="shape.eq.length">
          <div v-for="(line, i) in shape.eq" :key="'f' + i" class="shape__eqline">{{ line }}</div>
          <div v-for="(line, i) in shape.anti" :key="'F' + i" class="shape__eqline shape__eqline--anti">{{ line }}</div>
        </template>
        <div v-else class="shape__eqline shape__eqline--none">no equation published for this curve</div>
      </div>
      <p class="shape__note">{{ shape.note }}</p>
      <p class="shape__why">
        The antiderivative is on the panel because it is what makes the device work: the shaper evaluates
        <span class="tabular">(F₁(xₙ) − F₁(xₙ₋₁)) ⁄ (xₙ − xₙ₋₁)</span> in place of <span class="tabular">f(xₙ)</span>.
      </p>
      <p class="shape__caption">
        Output against input, with drive, bias, the post clipper and the output trim. The lit span is the
        input peak's reach. Colour is excluded — it is frequency-dependent and this plot has no frequency
        axis — and so is mix.
      </p>
      <p v-if="!hasTransfer" class="shape__warn">
        This build publishes no transfer stream, so the curve is the page's own arithmetic rather than the
        engine's.
      </p>
    </div>
  </div>
</template>
