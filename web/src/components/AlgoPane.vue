<script setup lang="ts">
import type { AlgoLoop, Track } from '../model/song'

const props = defineProps<{ loops: AlgoLoop[]; tracks: Track[] }>()
const trackName = (id: string) => props.tracks.find((t) => t.id === id)?.name ?? id
</script>

<template>
  <section class="pane">
    <div class="pane-head">
      <span>Algo loops</span>
      <span class="soon">generators run in the engine · plan.md M4</span>
    </div>
    <ul class="loops">
      <li v-for="l in loops" :key="l.id" class="loop">
        <div class="row">
          <b>{{ l.id }}</b>
          <span class="gen">{{ l.generator }}</span>
          <span class="params">
            <span v-for="(v, k) in l.params" :key="k">{{ k }}={{ v }}</span>
          </span>
          <span class="tag" :class="l.mode">{{ l.mode }}</span>
        </div>
        <div class="steps">
          <span v-for="(v, i) in l.preview" :key="i" class="step" :style="{ opacity: 0.15 + v * 0.85 }" :class="{ hit: v > 0 }" />
        </div>
        <div class="row meta">
          <span>→ {{ trackName(l.target) }}</span>
          <span>{{ l.scale }}</span>
          <span>seed {{ l.seed }}</span>
        </div>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.loops { list-style: none; margin: 0; padding: 8px; display: grid; gap: 8px; }
.loop { background: var(--panel-2); border: 1px solid var(--line); border-left: 3px solid var(--algo); border-radius: 4px; padding: 8px 10px; display: grid; gap: 6px; }
.row { display: flex; align-items: center; gap: 10px; }
.gen { color: var(--algo); }
.params { display: flex; gap: 8px; color: var(--muted); flex: 1; font-family: ui-monospace, monospace; font-size: 11px; }
.meta { color: var(--muted); font-size: 11px; }
.steps { display: grid; grid-template-columns: repeat(16, 1fr); gap: 3px; }
.step { height: 14px; border-radius: 2px; background: var(--line); }
.step.hit { background: var(--algo); }
.live { color: var(--algo); border-color: var(--algo); }
.frozen { color: var(--hand); }
</style>
