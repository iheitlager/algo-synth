<script setup lang="ts">
// The three sources (ADR-0005). In the base they all play the engine's test
// voice; the controls marked "soon" arrive with their source's MVP.
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { getEngine, status } from '../audio/engine'
import { Param, Source, type SourceId } from '../audio/params'

interface Card { id: SourceId; name: string; style: string; mvp: string; color: string; knobs: string[] }
const cards: Card[] = [
  { id: Source.Mono, name: 'Mono', style: 'ARP 2600-style semi-modular', mvp: 'MVP 2', color: 'var(--mono)', knobs: ['VCO 1-3', 'Ladder cutoff', 'Resonance', 'Glide', 'Patch'] },
  { id: Source.Wave, name: 'Wave', style: 'PPG-style wavetable', mvp: 'MVP 7', color: 'var(--wave)', knobs: ['Table', 'Wave pos', 'Env → wave', 'Filter', '8-bit'] },
  { id: Source.Drums, name: 'Drums', style: 'Analog drum processor', mvp: 'MVP 6', color: 'var(--drums)', knobs: ['Tune', 'Decay', 'Tone', 'Snap', 'Accent'] },
]
const pads = [
  ['Kick', 36], ['Snare', 38], ['Clap', 39], ['Hat C', 42],
  ['Tom L', 45], ['Hat O', 46], ['Tom H', 50], ['Cowbell', 56],
] as const
const keys = Array.from({ length: 25 }, (_, i) => 48 + i)
const black = (n: number) => [1, 3, 6, 8, 10].includes(n % 12)

const selected = ref<SourceId>(Source.Mono)
const attack = ref(0.005)
const release = ref(0.3)
watch(attack, (v) => getEngine()?.param(Param.Attack, v))
watch(release, (v) => getEngine()?.param(Param.Release, v))

const down = (s: SourceId, n: number) => { selected.value = s; getEngine()?.noteOn(s, n) }
const up = (s: SourceId, n: number) => getEngine()?.noteOff(s, n)

// Computer keyboard: the bottom two rows play C4..E5 on the selected source.
const map = 'awsedftgyhujkolp;'
const held = new Set<string>()
function onKey(e: KeyboardEvent, isDown: boolean) {
  if (e.repeat || e.metaKey || e.ctrlKey || (e.target as HTMLElement).tagName === 'INPUT') return
  const i = map.indexOf(e.key)
  if (i < 0) return
  const n = 60 + i
  if (isDown && !held.has(e.key)) { held.add(e.key); down(selected.value, n) }
  if (!isDown) { held.delete(e.key); up(selected.value, n) }
}
const kd = (e: KeyboardEvent) => onKey(e, true)
const ku = (e: KeyboardEvent) => onKey(e, false)
onMounted(() => { window.addEventListener('keydown', kd); window.addEventListener('keyup', ku) })
onBeforeUnmount(() => { window.removeEventListener('keydown', kd); window.removeEventListener('keyup', ku) })
</script>

<template>
  <section class="pane">
    <div class="pane-head">
      <span>Instruments</span>
      <span class="soon">{{ status.running ? 'keys A–; play the selected source' : 'power on to play' }}</span>
    </div>
    <div class="cards">
      <article
        v-for="c in cards" :key="c.id" class="card"
        :class="{ sel: selected === c.id }" :style="{ '--c': c.color }"
        @click="selected = c.id"
      >
        <header><b>{{ c.name }}</b><span class="style">{{ c.style }}</span></header>
        <div class="knobs">
          <label>Attack <input v-model.number="attack" type="range" min="0.001" max="2" step="0.001" /></label>
          <label>Release <input v-model.number="release" type="range" min="0.005" max="4" step="0.005" /></label>
          <span v-for="k in c.knobs" :key="k" class="knob soon" :title="c.mvp">{{ k }}</span>
        </div>
        <div v-if="c.id === Source.Drums" class="pads">
          <button
            v-for="[name, n] in pads" :key="n" :disabled="!status.running"
            @pointerdown="down(c.id, n)" @pointerup="up(c.id, n)" @pointerleave="up(c.id, n)"
          >{{ name }}</button>
        </div>
        <div v-else class="kbd">
          <span
            v-for="n in keys" :key="n" class="key" :class="{ black: black(n) }"
            @pointerdown="down(c.id, n)" @pointerup="up(c.id, n)" @pointerleave="up(c.id, n)"
          />
        </div>
        <footer class="soon">test voice until {{ c.mvp }}</footer>
      </article>
    </div>
  </section>
</template>

<style scoped>
.cards { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 8px; padding: 8px; }
.card { background: var(--panel-2); border: 1px solid var(--line); border-top: 3px solid var(--c); border-radius: 4px; padding: 10px; display: grid; gap: 10px; align-content: start; cursor: pointer; }
.card.sel { box-shadow: 0 0 0 1px var(--c); }
header { display: flex; align-items: baseline; gap: 8px; }
header b { color: var(--c); font-size: 15px; }
.style { color: var(--muted); font-size: 11px; }
.knobs { display: grid; grid-template-columns: 1fr 1fr; gap: 6px 10px; }
.knobs label { display: grid; gap: 2px; font-size: 11px; color: var(--muted); }
.knob { border: 1px dashed var(--line); border-radius: 3px; padding: 3px 6px; }
.pads { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; }
.pads button { height: 44px; border-color: var(--c); }
.kbd { display: flex; height: 64px; gap: 1px; }
.key { flex: 1; background: #d9dde3; border-radius: 0 0 3px 3px; }
.key.black { background: #2b2f36; height: 62%; }
.key:active { background: var(--c); }
</style>
