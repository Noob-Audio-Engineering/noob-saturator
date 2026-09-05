<script setup>
/**
 * The dry/wet alignment: what the wet path costs, what the dry path is given,
 * what was reported to the host, and the latency that follows.
 *
 * It makes the second claim visible without a plot. Ableton's Saturator sums a
 * delayed wet path against an undelayed dry one, so the mix combs; ours delays
 * the dry path to match, at every setting and every sample rate.
 *
 * **This is an invariant, not a test, and it is drawn as one.** The two delays
 * are equal by construction — the engine derives one from the other — so there
 * is no lamp here that could read as a pass which might one day fail. The line
 * states the identity and puts the numbers on it. The only reason `equal` is
 * checked at all is that a build where they somehow differ is an engine fault
 * worth seeing, and that case says "engine fault" rather than dressing itself
 * up as a routine comparison that came out the other way.
 *
 * The antialiasing kernel's fractional half-sample is deliberately not folded
 * into the reported figure, so it is shown beside it as excluded rather than
 * left off the panel altogether.
 *
 * **Why this says nothing about why theirs combs.** Two measured dips in a
 * sibling Ableton device are consistent with an uncompensated delay, and the
 * arithmetic is sound, but a decimator's passband droop with no delay mismatch
 * at all would give the same dips, and the two have different fixes. Our
 * design is immune to both. Somebody else's inference is not ours to state as
 * established, so this line states our own arrangement and stops there.
 */
import { computed } from 'vue';
import { useAlign, useSat } from '../composables/useSaturator.js';

const align = useAlign();
const sat = useSat();
const sa = (v) => (v == null || !Number.isFinite(v) ? '—' : `${v.toFixed(0)} sa`);
/** The oversampling ratio in force, from the control rather than from the stream. */
const factor = computed(() => (sat.oversample ? sat.oversample.label : null));
const ms = computed(() => (align.latencyMs == null ? null : align.latencyMs.toFixed(2)));
</script>

<template>
  <section class="align" :class="{ 'is-fault': align.has && align.live && !align.equal, 'is-dark': !align.has }">
    <span class="align__cap">Dry / wet alignment</span>
    <template v-if="align.has">
      <span class="align__claim">
        the dry path is delayed to match the wet path
        <template v-if="!align.equal"> — <b>engine fault: it is not</b></template>
      </span>
      <span class="align__pair"><i>wet</i><b class="tabular">{{ sa(align.wet) }}</b></span>
      <span class="align__eq">=</span>
      <span class="align__pair"><i>dry</i><b class="tabular">{{ sa(align.dry) }}</b></span>
      <span v-if="factor" class="align__pair"><i>oversampling</i><b class="tabular">{{ factor }}</b></span>
      <span class="align__pair align__pair--wide">
        <i>reported</i>
        <b class="tabular">{{ sa(align.reported) }}<span v-if="ms != null" class="align__ms"> · {{ ms }} ms</span></b>
      </span>
      <span class="align__excl">
        antialiasing kernel {{ align.kernelFrac ? `+${align.kernelFrac.toFixed(1)} sa` : '—' }}, deliberately excluded
      </span>
    </template>
    <span v-else class="align__excl">this build publishes no alignment stream</span>
  </section>
</template>
