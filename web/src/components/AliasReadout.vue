<script setup>
/**
 * The one display this device needs and Ableton does not have.
 *
 * With a periodic input, everything that is not at a harmonic of the
 * fundamental is aliasing. Showing that number live is the cheapest possible
 * demonstration of the whole argument: turn the drive up and it stays put
 * while the harmonic content beside it climbs. Ableton put a spectrum
 * analyser inside Saturator in the Live 12.1 redesign and still do not show
 * the defect their own manual admits to.
 *
 * **The two numbers move in opposite directions and that is the argument.**
 * Both are measurements of this device — the wanted distortion rising with
 * drive, the unwanted staying where it was — so nothing here depends on a
 * counterfactual, and nothing here is a comparison with anyone else's device.
 * Nobody has measured Ableton's Saturator, so no figure on this panel is a
 * margin over theirs and no wording implies one.
 *
 * **Three conditions are printed rather than left for a user to discover**,
 * because each of them makes a number stop meaning what it looks like:
 *
 * 1. *Periodicity.* The reading is taken from the signal actually passing,
 *    not from a probe tone, so on a drum loop there is no fundamental to be
 *    non-harmonic of. Below the threshold the figure greys, the bar turns,
 *    and the strip says so. A headline feature that lies on real material is
 *    worse than no headline feature.
 * 2. *Nyquist.* A fundamental whose harmonics all lie above Nyquist has no
 *    harmonic distortion left in band — at 15 kHz that is every one of them —
 *    so the harmonic field sits at its floor by construction. That is
 *    arithmetic, not a fault, and the panel says which.
 * 3. *The floor.* Below the engine's measured harmonic floor the display
 *    cannot tell a working shaper from a wire, so the floor is on the face
 *    rather than in a comment.
 *
 * In offline design mode every figure here is invented by `dev/manifest.js`,
 * so the strip stamps itself DESIGN MODE and marks the numbers. A screenshot
 * of the mock must not be readable as a bench figure.
 */
import { computed } from 'vue';
import { Timeline } from '@noob-audio-engineering/noob-vst-webgui-framework/vue';
import { CONFIDENCE_FLOOR, dbText, getClient, hzText, useAlias } from '../composables/useSaturator.js';

const alias = useAlias();
const offline = computed(() => !!getClient()?.offline);

/** How full the periodicity bar is, and where the threshold tick sits on it. */
const confidencePct = computed(() => `${Math.round(Math.min(1, Math.max(0, alias.confidence)) * 100)}%`);
const floorPct = `${CONFIDENCE_FLOOR * 100}%`;

/** The harmonic reading, or the short reason there is not one. */
const harmonicText = computed(() => {
  if (!alias.has || alias.harmonic == null) return '—';
  if (!alias.harmonicInBand) return 'above Nyquist';
  if (alias.harmonicAtFloor) return `at floor ${dbText(alias.harmonicFloor)} dB`;
  return `${dbText(alias.harmonic)} dBc`;
});
/** Whether that cell is a measurement or an explanation, so it can be dimmed. */
const harmonicMuted = computed(() => alias.has && (!alias.harmonicInBand || alias.harmonicAtFloor));

/**
 * The line under the figures. It carries the claims wording normally and the
 * arithmetic when one of the two conditions has taken the harmonic reading
 * away — which is the moment a user most needs to be told why, so it replaces
 * the standing line rather than being added below it.
 */
const foot = computed(() => {
  if (alias.has && !alias.harmonicInBand) {
    return `Every harmonic of ${hzText(alias.f0)} lies above Nyquist, so there is none left in band to measure. The aliasing figure is unaffected.`;
  }
  if (alias.has && alias.harmonicAtFloor) {
    return `The harmonic floor is ${dbText(alias.harmonicFloor)} dB; below it this display cannot tell a working shaper from a wire.`;
  }
  return "Both figures are of this device. Nobody has measured Ableton's Saturator, so nothing here is a margin over it.";
});
const footNote = computed(() => alias.has && (!alias.harmonicInBand || alias.harmonicAtFloor));

/*
 * The history: the aliasing against the harmonic content, both of this device,
 * over the last ten seconds. Sweep the drive and the brass trace climbs while
 * the green one does not.
 *
 * **The chart is built once, on the page's first render, and never rebuilt.**
 * The framework's `Timeline` takes its series in `onMounted` and only there: a
 * chart mounted later by a `v-if`, or replaced by a changed `:key`, draws its
 * grid and no traces at all, with nothing in the console. Both were tried
 * while wiring this up. The series are therefore fixed here, which the
 * contract now allows — `harmonic_db` carries an explicit unit, so there is
 * nothing left to wait for a frame to tell us.
 */
const series = [
  { stream: 'alias', index: 3, unit: 'raw', range: [-120, 0], color: '#d9a441', width: 1.4, label: 'harmonics' },
  { stream: 'alias', index: 0, unit: 'raw', range: [-120, 0], color: '#5ee2a0', width: 1.8, label: 'aliasing' },
];
</script>

<template>
  <section class="alias" :class="{ 'is-mock': offline, 'is-idle': alias.has && !alias.usable }">
    <div class="alias__main">
      <div class="alias__cap">
        <span>Alias</span>
        <span v-if="offline" class="alias__stamp">design mode · synthetic</span>
      </div>
      <div v-if="alias.has" class="alias__big" :class="{ 'is-idle': !alias.usable }">
        <span class="alias__num tabular">{{ dbText(alias.aliasDb) }}</span>
        <span class="alias__unit">dBc</span>
      </div>
      <div v-else class="alias__big alias__big--dark"><span class="alias__num">—</span></div>
      <div class="alias__cond">
        <template v-if="!alias.has">no alias measurement in this build</template>
        <template v-else-if="alias.usable">
          measured on the signal passing · everything off a harmonic of {{ hzText(alias.f0) }}
        </template>
        <template v-else>
          the input is not periodic enough to measure · this needs a tone
        </template>
      </div>
    </div>

    <div class="alias__compare">
      <div class="alias__row">
        <span class="alias__k">harmonic content, wanted</span>
        <span class="alias__v tabular" :class="harmonicMuted ? 'alias__v--muted' : 'alias__v--thd'">{{ harmonicText }}</span>
      </div>
      <div class="alias__row">
        <span class="alias__k">input fundamental</span>
        <span class="alias__v tabular">{{ alias.has ? hzText(alias.f0) : '—' }}</span>
      </div>
      <div class="alias__row alias__row--bar">
        <span class="alias__k">periodicity</span>
        <span class="alias__bar" :class="{ 'is-under': !alias.usable }">
          <i class="alias__fill" :style="{ width: confidencePct }" />
          <i class="alias__floor" :style="{ left: floorPct }" />
        </span>
        <span class="alias__v tabular">{{ alias.has ? alias.confidence.toFixed(2) : '—' }}</span>
      </div>
      <p class="alias__foot" :class="{ 'is-note': footNote }">{{ foot }}</p>
    </div>

    <div class="alias__history">
      <div class="alias__histcap">
        <span>last 10 s · sweep the drive</span>
        <!--
          The key is here rather than inside the chart. The framework's own
          legend sits top-right, which is exactly where the harmonic trace goes
          once the drive is up, so at the settings this display exists to show,
          the labels were underneath the line.
        -->
        <span class="alias__key alias__key--ours">aliasing</span>
        <span class="alias__key alias__key--thd">harmonics</span>
      </div>
      <div class="alias__histbox">
        <Timeline
          v-if="alias.has"
          :series="series"
          :seconds="10"
          :grid-series="1"
          :grid-step="24"
          :time-ticks="false"
          :legend="false"
        />
        <div v-else class="alias__dark">no alias measurement in this build</div>
      </div>
    </div>
  </section>
</template>
