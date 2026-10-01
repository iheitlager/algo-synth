<script setup lang="ts">
// The three sources (ADR-0005). In the base they all play the engine's test
// voice; the controls marked "soon" arrive with their source's MVP.
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { getEngine, status } from '../audio/engine'
import { NoiseColour, Param, type ParamId, Source, type SourceId, Waveform } from '../audio/params'

interface Card { id: SourceId; name: string; style: string; mvp: string; color: string; knobs: string[] }
const cards: Card[] = [
  { id: Source.Mono, name: 'Mono', style: 'ARP 2600-style semi-modular', mvp: 'MVP 2', color: 'var(--mono)', knobs: ['Ladder cutoff', 'Resonance', 'Glide', 'Patch'] },
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

// Mono's VCOs (spec 004 Req 1). The view only sends values; Rust clamps them.
const waves = Object.entries(Waveform)
const colours = Object.entries(NoiseColour)
const vcos = [
  { n: 1, wave: Param.Vco1Wave, coarse: Param.Vco1Coarse, fine: Param.Vco1Fine, level: Param.Vco1Level, level0: 1 },
  { n: 2, wave: Param.Vco2Wave, coarse: Param.Vco2Coarse, fine: Param.Vco2Fine, level: Param.Vco2Level, level0: 0, sync: Param.Vco2Sync },
  { n: 3, wave: Param.Vco3Wave, coarse: Param.Vco3Coarse, fine: Param.Vco3Fine, level: Param.Vco3Level, level0: 0, sync: Param.Vco3Sync },
]
function send(id: ParamId, e: Event) {
  const t = e.target as HTMLInputElement
  getEngine()?.param(id, t.type === 'checkbox' ? Number(t.checked) : Number(t.value))
}

const down = (s: SourceId, n: number) => { selected.value = s; getEngine()?.noteOn(s, n) }
const up = (s: SourceId, n: number) => getEngine()?.noteOff(s, n)

// Computer keyboard: the bottom two rows play C4..E5 on the selected source.
const map = 'awsedftgyhujkolp;'
const held = new Set<string>()
function onKey(e: KeyboardEvent, isDown: boolean) {
  if (e.repeat || e.metaKey || e.ctrlKey || ['INPUT', 'SELECT'].includes((e.target as HTMLElement).tagName)) return
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
        <div v-if="c.id === Source.Mono" class="vcos">
          <div v-for="v in vcos" :key="v.n" class="vco">
            <b>VCO {{ v.n }}</b>
            <select @change="send(v.wave, $event)">
              <option v-for="[name, id] in waves" :key="id" :value="id">{{ name }}</option>
            </select>
            <label>Coarse <input type="range" min="-24" max="24" step="1" value="0" @input="send(v.coarse, $event)" /></label>
            <label>Fine <input type="range" min="-50" max="50" step="1" value="0" @input="send(v.fine, $event)" /></label>
            <label>Level <input type="range" min="0" max="1" step="0.01" :value="v.level0" @input="send(v.level, $event)" /></label>
            <label v-if="v.sync !== undefined" class="sync"><input type="checkbox" @change="send(v.sync, $event)" /> Sync to 1</label>
          </div>
          <label>Pulse width <input type="range" min="0.05" max="0.95" step="0.01" value="0.5" @input="send(Param.PulseWidth, $event)" /></label>
          <div class="vco">
            <b>Noise</b>
            <select @change="send(Param.NoiseColour, $event)">
              <option v-for="[name, id] in colours" :key="id" :value="id">{{ name }}</option>
            </select>
            <label>Level <input type="range" min="0" max="1" step="0.01" value="0" @input="send(Param.NoiseLevel, $event)" /></label>
          </div>
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
.vcos { display: grid; gap: 6px; font-size: 11px; color: var(--muted); }
.vco { display: grid; grid-template-columns: auto 1fr 1fr; gap: 4px 8px; align-items: center; }
.vco b { color: var(--c); }
.vco label, .vcos > label { display: grid; gap: 2px; }
.vco .sync { display: flex; gap: 4px; align-items: center; }
.knobs label { display: grid; gap: 2px; font-size: 11px; color: var(--muted); }
.knob { border: 1px dashed var(--line); border-radius: 3px; padding: 3px 6px; }
.pads { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; }
.pads button { height: 44px; border-color: var(--c); }
.kbd { display: flex; height: 64px; gap: 1px; }
.key { flex: 1; background: #d9dde3; border-radius: 0 0 3px 3px; }
.key.black { background: #2b2f36; height: 62%; }
.key:active { background: var(--c); }
</style>
