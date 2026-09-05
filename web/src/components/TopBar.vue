<script setup>
/**
 * The bar across the top: what this is, whether it is talking to a plug-in,
 * and the page-level tools (undo, redo, A/B, the development panel). None of
 * it is a parameter; the deck below owns every one of those.
 *
 * The status dot is honest about offline design mode: `connected` stays
 * false while the client is running on the design manifest, so the dot is
 * dark and the word beside it says so.
 */
import { computed } from 'vue';
import { useDebug, useNoobVstWebguiFramework } from '../composables/useSaturator.js';

const { connected, history, historyState, stats } = useNoobVstWebguiFramework();
const debug = useDebug();
const state = computed(() => (connected.value ? 'live' : 'design mode'));
</script>

<template>
  <header class="bar">
    <div class="bar__brand">
      <span class="bar__dot" :class="{ on: connected }" />
      <span class="bar__name">Noob Saturator</span>
      <span class="bar__sub">antialiased waveshaper</span>
    </div>

    <div class="bar__tools">
      <span class="bar__state">{{ state }}</span>
      <span v-if="connected" class="bar__stat tabular">{{ Math.round(stats.rttAvgMs || 0) }} ms</span>
      <button class="key" :disabled="!historyState.canUndo" title="Ctrl+Z" @click="history.undo()">Undo</button>
      <button class="key" :disabled="!historyState.canRedo" title="Ctrl+Shift+Z" @click="history.redo()">Redo</button>
      <button class="key" :class="{ on: historyState.ab === 'B' }" title="Ctrl+B" @click="history.toggleAB()">
        {{ historyState.ab }}
      </button>
      <button class="key" :class="{ on: debug }" @click="debug = !debug">Bench</button>
    </div>
  </header>
</template>
