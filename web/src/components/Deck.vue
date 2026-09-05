<script setup>
/**
 * The control deck: the whole parameter set, in three groups that follow the
 * signal — the shaper, the colour pair around it, and the output stage.
 *
 * The knobs are ours (`SatKnob.vue`); the switches are the framework's
 * `Segmented` and `Toggle`, which ship unstyled on purpose and are dressed
 * here in `style.css` as the panel's own lit keys. Behaviour generic, look
 * local, which is the rule.
 *
 * **Every control on this deck states its unit, and none of them states it
 * here.** Bias is a percentage of the clipping point, the colour width is a Q,
 * the clip knee is decibels below the ceiling — all three arrived unit-free
 * and the engine has since given all three a unit, so the handle formats the
 * value and this file renders it. That is the better outcome by a distance:
 * the improvement over Ableton's unit-free colour width now survives in a
 * host's generic parameter view, not only on our own face.
 *
 * The second lines that remain say things a unit cannot: the DC filter states
 * its corner and how many sections it is, the knee says which of the two
 * things it is currently shaping, and the oversampling ratio says that every
 * one of its settings is antialiased, because it is not a quality switch and
 * must not be read as one. There is no quality mode on this device at all.
 */
import { computed } from 'vue';
import { Segmented, Toggle } from '@noob-audio-engineering/noob-vst-webgui-framework/vue';
import SatKnob from './SatKnob.vue';
import { useNoobVstWebguiFramework, useSat } from '../composables/useSaturator.js';

const sat = useSat();
const { manifest } = useNoobVstWebguiFramework();

/**
 * The DC filter's corner and how many sections it is, both published by the
 * engine because the panel prints them. It is two filters rather than one
 * because a biased shape rectifies, and one section does not take out as much
 * offset as maximum bias creates.
 */
const dcHz = computed(() => manifest.value?.meta?.dc_corner_hz ?? null);
const dcSections = computed(() => manifest.value?.meta?.dc_sections ?? 1);
const dcHint = computed(() => {
  if (!dcHz.value) return 'corner not published';
  return dcSections.value > 1 ? `${dcSections.value} × ${dcHz.value} Hz` : `${dcHz.value} Hz corner`;
});

/**
 * The knee acts in two places: it opens the post clipper's corner in Soft
 * mode, and it is the Clip curve's own knee parameter. So it is live whenever
 * either of those is selected, and dark when neither is — and the hint says
 * which of them is holding it open, because a control that is sometimes live
 * for reasons the panel does not explain is the same defect as a unit-free
 * one.
 */
const clipStep = computed(() => (sat.clipMode ? sat.clipMode.index : 0));
const curveIsClip = computed(() => sat.shape.value.key === 'clip');
const kneeOff = computed(() => clipStep.value !== 1 && !curveIsClip.value);
const kneeHint = computed(() => {
  if (curveIsClip.value && clipStep.value === 1) return 'the curve and the ceiling';
  if (curveIsClip.value) return 'the Clip curve';
  if (clipStep.value === 1) return 'the soft ceiling';
  if (clipStep.value === 2) return 'hard has no knee';
  return 'no knee in use';
});

const colourOff = computed(() => !!sat.colorOn && !sat.colorOn.on);

</script>

<template>
  <section class="deck">
    <div class="deck__group deck__group--shape">
      <h3 class="deck__head">Shaper</h3>
      <div class="deck__row">
        <SatKnob v-if="sat.drive" :p="sat.drive" label="Drive" :size="64" />
        <SatKnob v-if="sat.bias" :p="sat.bias" label="Bias" :size="52" hint="offset, of the ceiling" />
        <div v-if="sat.curve" class="deck__stack">
          <span class="deck__cap">Curve</span>
          <Segmented :p="sat.curve" class="keys keys--curve" />
          <span class="deck__hint">equation on the display</span>
        </div>
        <SatKnob v-if="sat.output" :p="sat.output" label="Output" :size="52" />
        <div class="deck__stack deck__stack--mix">
          <SatKnob v-if="sat.mix" :p="sat.mix" label="Dry/Wet" :size="52" />
          <Segmented v-if="sat.mixLaw" :p="sat.mixLaw" class="keys keys--law" />
        </div>
      </div>
    </div>

    <div class="deck__group deck__group--colour" :class="{ 'is-off': colourOff }">
      <h3 class="deck__head">
        Colour
        <Toggle v-if="sat.colorOn" :p="sat.colorOn" variant="button" class="keys keys--lamp">on</Toggle>
      </h3>
      <div class="deck__row">
        <SatKnob v-if="sat.colorBase" :p="sat.colorBase" label="Base" :size="48" :disabled="colourOff" hint="drive for the lows" />
        <SatKnob v-if="sat.colorFreq" :p="sat.colorFreq" label="Freq" :size="48" :disabled="colourOff" />
        <SatKnob v-if="sat.colorQ" :p="sat.colorQ" label="Width" :size="48" :disabled="colourOff" />
        <SatKnob v-if="sat.colorDepth" :p="sat.colorDepth" label="Depth" :size="48" :disabled="colourOff" hint="drive for the band" />
      </div>
    </div>

    <div class="deck__group deck__group--out">
      <h3 class="deck__head">Output stage</h3>
      <div class="deck__row">
        <div v-if="sat.dcBlock" class="deck__stack">
          <span class="deck__cap">DC filter</span>
          <Toggle :p="sat.dcBlock" variant="button" class="keys keys--lamp">on</Toggle>
          <span class="deck__hint">{{ dcHint }}</span>
        </div>
        <div v-if="sat.clipMode" class="deck__stack">
          <span class="deck__cap">Post clip</span>
          <Segmented :p="sat.clipMode" class="keys" />
          <span class="deck__hint">inside the antialiasing</span>
        </div>
        <SatKnob v-if="sat.clipKnee" :p="sat.clipKnee" label="Knee" :size="46" :disabled="kneeOff" :hint="kneeHint" />
        <div v-if="sat.oversample" class="deck__stack">
          <span class="deck__cap">Oversample</span>
          <Segmented :p="sat.oversample" class="keys" />
          <span class="deck__hint">every setting is antialiased</span>
        </div>
      </div>
    </div>
  </section>
</template>
