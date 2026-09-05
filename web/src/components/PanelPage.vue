<script setup>
/**
 * The panel: the top bar, the alias strip, the stage, the alignment line,
 * the deck, and the bench when it is up.
 *
 * **The alias strip is never behind a tab.** It is the whole argument for
 * this device existing, so it sits above the stage at every window size and
 * the two curve displays share the space below it. The transfer display and
 * the colour display are one stage with two panes: a window with room for
 * both shows both and drops the tab keys, and a small one shows the pane the
 * key selects. That switch is pure CSS on the window size, so nothing
 * measures anything and nothing re-lays-out on a resize tick.
 *
 * The order down the page follows the signal — shape, then the colour pair
 * around it, then the output stage — and the alignment line sits between the
 * displays and the deck because it is a statement about the mix control
 * immediately below it.
 */
import { ResizeGrip } from '@noob-audio-engineering/noob-vst-webgui-framework/vue';
import TopBar from './TopBar.vue';
import AliasReadout from './AliasReadout.vue';
import ShapeDisplay from './ShapeDisplay.vue';
import ColorDisplay from './ColorDisplay.vue';
import AlignBar from './AlignBar.vue';
import Deck from './Deck.vue';
import DevPanel from './DevPanel.vue';
import { WINDOW_MIN, useDebug, useStage, useWindow } from '../composables/useSaturator.js';

const stage = useStage();
const debug = useDebug();
useWindow();
</script>

<template>
  <div class="sat">
    <TopBar />
    <main class="sat__body">
      <!--
        The two measurements share one plate, because they are one argument:
        this much aliasing, and a dry path aligned to the wet one. Neither is
        ever behind a tab.
      -->
      <div class="measure">
        <AliasReadout />
        <AlignBar />
      </div>

      <div class="stage" :class="`stage--${stage}`">
        <div class="stage__tabs">
          <button class="tab" :class="{ on: stage === 'shape' }" @click="stage = 'shape'">Transfer</button>
          <button class="tab" :class="{ on: stage === 'colour' }" @click="stage = 'colour'">Colour</button>
        </div>
        <div class="stage__panes">
          <section class="pane pane--shape">
            <h2 class="pane__cap">Transfer curve<span>the signal drawn on the shape, after Ableton</span></h2>
            <ShapeDisplay />
          </section>
          <section class="pane pane--colour">
            <h2 class="pane__cap">Colour curve<span>pre-emphasis, with the spectra behind it, after Ableton</span></h2>
            <ColorDisplay />
          </section>
        </div>
      </div>

      <Deck />
      <DevPanel v-if="debug" />
    </main>
    <ResizeGrip class="sat__grip" :min="WINDOW_MIN" />
  </div>
</template>
