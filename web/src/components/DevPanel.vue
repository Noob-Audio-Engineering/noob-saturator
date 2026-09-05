<script setup>
/**
 * The bench: what the client is connected to, what the manifest says, and
 * every parameter's raw value. It belongs where debugging and demonstrating
 * happen, so it is on under the standalone and in offline design mode and
 * off inside a host; the Bench key in the top bar overrides it either way.
 *
 * The first card is the one that matters, and it is here rather than only in
 * the source: in offline design mode every stream frame on this page is
 * invented by `dev/manifest.js` to give the panel something to be designed
 * against. No figure on the face is a measurement until the dot in the top
 * bar is lit.
 */
import { computed } from 'vue';
import { getClient, useNoobVstWebguiFramework } from '../composables/useSaturator.js';

const { manifest, stats, connected } = useNoobVstWebguiFramework();
const client = getClient();
const offline = computed(() => !!client?.offline);
const params = computed(() => (manifest.value?.params || []).map((p) => client.param(p.id)));
const streams = computed(() => manifest.value?.streams || []);
</script>

<template>
  <section class="bench">
    <div class="bench__grid">
      <div class="bench__card" :class="{ 'is-warn': offline }">
        <h4>Source of the numbers</h4>
        <p class="bench__note">
          <template v-if="offline">
            <b>Offline design mode.</b> Every stream frame on this page is synthesised by
            <code>dev/manifest.js</code> so the panel can be built before the DSP exists. The alias reading,
            the naive comparison, the spectra and the latency figures are invented, and none of them may be
            quoted anywhere. The strip stamps itself accordingly.
          </template>
          <template v-else-if="connected">
            <b>Live.</b> Every figure on the panel comes from the engine's own probes.
          </template>
          <template v-else>Waiting for the plug-in.</template>
        </p>
      </div>
      <div class="bench__card">
        <h4>Connection</h4>
        <dl>
          <dt>url</dt>
          <dd class="wrap">{{ client?.url }}</dd>
          <dt>plug-in</dt>
          <dd>{{ manifest?.name }} {{ manifest?.meta?.version || '' }}</dd>
          <dt>rate</dt>
          <dd>{{ manifest?.meta?.sample_rate }} Hz</dd>
          <dt>rtt</dt>
          <dd>{{ (stats.rttAvgMs || 0).toFixed(1) }} ms · {{ Math.round(stats.fps || 0) }} fps</dd>
        </dl>
      </div>
      <div class="bench__card">
        <h4>Streams</h4>
        <dl>
          <template v-for="s in streams" :key="s.id">
            <dt>{{ s.id }}</dt>
            <dd>{{ s.kind }} · {{ s.capacity }}{{ s.sticky ? ' · sticky' : '' }}</dd>
          </template>
        </dl>
      </div>
    </div>

    <table class="bench__table">
      <thead>
        <tr><th>id</th><th>name</th><th>plain</th><th>norm</th><th>range</th></tr>
      </thead>
      <tbody>
        <tr v-for="p in params" :key="p.id">
          <td>{{ p.id }}</td>
          <td class="dim">{{ p.name }}</td>
          <td>{{ p.format() }}</td>
          <td class="dim">{{ p.norm.toFixed(4) }}</td>
          <td class="dim">{{ p.spec?.labels ? p.spec.labels.join(' / ') : `${p.min} … ${p.max}` }}</td>
        </tr>
      </tbody>
    </table>
  </section>
</template>
