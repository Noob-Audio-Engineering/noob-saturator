<script setup>
/**
 * Noob Saturator: the root. Once the manifest is in (`ready`) the panel
 * renders; before that a short status line says what the client is doing. In
 * development the offline manifest makes that immediate.
 *
 * Keyboard: Ctrl+Z / Ctrl+Shift+Z (or Ctrl+Y) undo and redo through the
 * framework's history, Ctrl+B toggles A/B.
 */
import { onBeforeUnmount, onMounted } from 'vue';
import { useNoobVstWebguiFramework } from './composables/useSaturator.js';
import PanelPage from './components/PanelPage.vue';

const { ready, connected, history } = useNoobVstWebguiFramework();

function onKey(e) {
  const t = e.target;
  if (t && (t.tagName === 'INPUT' || t.tagName === 'SELECT' || t.tagName === 'TEXTAREA')) return;
  const mod = e.ctrlKey || e.metaKey;
  const k = e.key.toLowerCase();
  if (mod && k === 'z' && !e.shiftKey) history.undo();
  else if ((mod && k === 'y') || (mod && e.shiftKey && k === 'z')) history.redo();
  else if (mod && k === 'b') history.toggleAB();
  else return;
  e.preventDefault();
}
onMounted(() => window.addEventListener('keydown', onKey));
onBeforeUnmount(() => window.removeEventListener('keydown', onKey));
</script>

<template>
  <PanelPage v-if="ready" />
  <div v-else class="sat">
    <div class="sat__wait">{{ connected ? 'loading the manifest' : 'connecting to the plug-in' }}</div>
  </div>
</template>
