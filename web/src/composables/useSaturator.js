/**
 * Noob Saturator specifics on top of the generic
 * `@noob-audio-engineering/noob-vst-webgui-framework/vue` bridge: the
 * parameter handles the panel shares, the two measurement streams read as
 * reactive numbers, and the page's one window-size instance.
 *
 * Everything here needs the manifest, so call these only once
 * `useNoobVstWebguiFramework().ready` is true — `App.vue` renders the panel
 * behind `v-if="ready"`. Handles are cached by the framework, so every
 * component shares one subscription per parameter.
 *
 * **Every stream is optional and every reader says so.** The engine half is
 * being written alongside this page; a build that has not got as far as the
 * alias probe should render a panel with one display dark and a line saying
 * which, rather than a blank page or a lie. `has` on each reader is what the
 * components branch on.
 */
import { computed, reactive } from 'vue';
import {
  getClient,
  hasParam,
  hasStream,
  useNoobVstWebguiFramework,
  useParam,
  useStoredRef,
  useStreamFrame,
  useWindowSize,
} from '@noob-audio-engineering/noob-vst-webgui-framework/vue';
import { curveFor } from '../curves.js';

export { getClient, hasParam, hasStream, useParam, useNoobVstWebguiFramework, useStoredRef };

/** Smallest window the panel lays out in, `[width, height]` CSS pixels; `src/plugin.rs` clamps to the same. */
export const WINDOW_MIN = [900, 520];

let win = null;
/**
 * The page's one `useWindowSize` instance (window size, resize requests,
 * fullscreen intent), created on first use from the root component so its
 * listeners live as long as the page. The top bar and the grip share it.
 */
export function useWindow() {
  win ??= useWindowSize({ min: WINDOW_MIN });
  return win;
}

let sat = null;
/**
 * Every parameter handle the panel uses, resolved once, plus the curve entry
 * the `curve` switch points at. A handle for an id the build does not
 * publish is `null` and the control that owns it is not drawn.
 */
export function useSat() {
  if (sat) return sat;
  const p = (id) => (hasParam(id) ? useParam(id) : null);
  const curve = p('curve');
  sat = {
    drive: p('drive'),
    bias: p('bias'),
    curve,
    output: p('output'),
    mix: p('mix'),
    mixLaw: p('mix_law'),
    colorOn: p('color_on'),
    colorBase: p('color_base'),
    colorFreq: p('color_freq'),
    colorQ: p('color_q'),
    colorDepth: p('color_depth'),
    dcBlock: p('dc_block'),
    clipMode: p('clip_mode'),
    clipKnee: p('clip_knee'),
    oversample: p('oversample'),
    /** The equation table entry for the selected curve; see `curves.js`. */
    shape: computed(() => curveFor(curve ? curve.label : '')),
  };
  return sat;
}

/**
 * The alias measurement, as numbers.
 *
 * The engine publishes four: `alias_db`, the aliasing the device is currently
 * producing; `f0_hz`, the fundamental it detected in the input; `confidence`,
 * zero to one, saying how periodic that input is; and `harmonics`, the
 * harmonic content — the wanted distortion.
 *
 * **The measurement is made on the input, not on a test tone**, which is why
 * confidence exists and why it governs the display. With a periodic input,
 * everything that is not at a harmonic of the fundamental is aliasing and the
 * figure means what it says. On a drum loop there is no fundamental to be
 * non-harmonic of, and a number shown there would be a lie. So `usable` gates
 * the readout: below the threshold the panel greys the figure and says why
 * rather than printing something that does not mean anything.
 *
 * **`harmonics` carries no unit in the frozen contract**, so the reader
 * decides from the value and says which it decided. Harmonic energy relative
 * to a fundamental is always negative, and a count of resolved harmonics is
 * always a small positive integer; the two cannot be confused. `harmonicKind`
 * is what the panel labels it with. This is a gap in the contract, not a
 * guess dressed up — the engine has been asked to state it.
 */
export function useAlias() {
  const has = hasStream('alias');
  const frame = has ? useStreamFrame('alias') : { value: null };
  const at = (i, dflt) => computed(() => (frame.value && Number.isFinite(frame.value[i]) ? frame.value[i] : dflt));
  const confidence = at(2, 0);
  const harmonics = at(3, null);
  return reactive({
    has,
    live: computed(() => frame.value != null),
    aliasDb: at(0, null),
    f0: at(1, null),
    confidence,
    harmonics,
    /** Whether the input is periodic enough for the figure to mean anything. */
    usable: computed(() => confidence.value >= CONFIDENCE_FLOOR),
    /** `'db'` when the value reads as energy relative to the fundamental, `'count'` when it reads as a number of harmonics. */
    harmonicKind: computed(() => {
      const v = harmonics.value;
      if (v == null || !Number.isFinite(v)) return 'none';
      return v <= 0 ? 'db' : 'count';
    }),
  });
}

/**
 * Below this the input is not periodic enough for an alias figure to mean
 * anything and the readout says so instead of printing one. Half is the
 * engine's own suggested threshold.
 */
export const CONFIDENCE_FLOOR = 0.5;

/**
 * The dry/wet alignment: what the wet path costs, what the dry path is given,
 * the oversampling factor in force, and the resulting latency in
 * milliseconds.
 *
 * **The two delays are equal by construction.** They are not two independent
 * measurements that happen to agree, and the panel must not be built as
 * though they might diverge — it shows the invariant holding, with the sample
 * counts and the latency that follows from them. `equal` exists so that a
 * build where they somehow differ is visible as the engine fault it would be,
 * not so the panel can offer a pass/fail lamp.
 */
export function useAlign() {
  const has = hasStream('align');
  const frame = has ? useStreamFrame('align') : { value: null };
  const at = (i, dflt) => computed(() => (frame.value && Number.isFinite(frame.value[i]) ? frame.value[i] : dflt));
  const wet = at(0, null);
  const dry = at(1, null);
  return reactive({
    has,
    live: computed(() => frame.value != null),
    wet,
    dry,
    factor: at(2, null),
    latencyMs: at(3, null),
    equal: computed(() => wet.value != null && dry.value != null && Math.abs(wet.value - dry.value) < 1e-6),
  });
}

/**
 * The colour section's forward magnitude curve: 129 points in dB, log-spaced
 * over the range the stream's meta declares. The inverse the display draws
 * beside it is this negated, which is what the topology says it is.
 *
 * `has` is false on a build that does not publish it yet, and the display
 * falls back to computing the pair from the parameters.
 */
export function useColorCurve() {
  const has = hasStream('color');
  const frame = has ? useStreamFrame('color') : { value: null };
  const meta = has ? getClient().stream('color').meta || {} : {};
  const range = Array.isArray(meta.hz_range) && meta.hz_range.length === 2 ? meta.hz_range : [20, 20000];
  return reactive({ has, points: computed(() => frame.value), range });
}

/**
 * Page state that is not a parameter: which display the stage is showing.
 * It lives in the UI store, so it travels with the plug-in and survives the
 * host closing and reopening the editor.
 */
export function useStage() {
  const which = useStoredRef('stage', 'shape');
  return computed({
    get: () => (which.value === 'colour' ? 'colour' : 'shape'),
    set: (v) => (which.value = v === 'colour' ? 'colour' : 'shape'),
  });
}

/**
 * Whether the bench panel is shown. The Bench key in the top bar opens it,
 * and the choice lives in the UI store, so it travels with the plug-in and
 * survives the host closing and reopening the editor.
 *
 * **Off by default, including under the standalone**, which is not what the
 * compressor lab does and the difference is deliberate. That plug-in's debug
 * panel carries controls — its demo source lives there — so a developer needs
 * it open. This one carries a table and a provenance note and nothing else,
 * because every measurement this device makes is already on its face. And the
 * panel has to open without scrolling at the 900 x 520 minimum, which it does
 * not do with a second plate under the deck. A development affordance does not
 * get to break the smallest window the plug-in supports.
 */
export function useDebug() {
  const stored = useStoredRef('debug.shown', false);
  return computed({
    get: () => !!stored.value,
    set: (v) => (stored.value = !!v),
  });
}

/** `-92.4`, or an em dash when there is nothing to show. */
export const dbText = (v, digits = 1) => (v == null || !Number.isFinite(v) ? '—' : v.toFixed(digits));

/** `15 kHz`, `1.5 kHz`, `440 Hz` — trailing zeros trimmed, because `1.00 kHz` on a panel reads as a false precision. */
export function hzText(hz) {
  if (hz == null || !Number.isFinite(hz)) return '—';
  if (hz < 1000) return `${Math.round(hz)} Hz`;
  const k = hz / 1000;
  return `${(Math.round(k * 100) / 100).toString()} kHz`;
}
