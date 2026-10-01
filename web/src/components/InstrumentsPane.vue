<script setup lang="ts">
// The three sources (ADR-0005). Mono has its own voice (spec 004); Wave and
// Drums play first timbres until their MVP. Controls marked "soon" arrive
// with the MVP named on the card.
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { getEngine, params, status } from '../audio/engine'
import { NoiseColour, Param, type ParamId, Preset, type PresetId, Source, type SourceId, Waveform } from '../audio/params'

interface Card { id: SourceId; name: string; style: string; mvp: string; color: string; knobs: string[] }
const cards: Card[] = [
  { id: Source.Mono, name: 'Mono', style: 'ARP 2600-style semi-modular', mvp: 'MVP 5', color: 'var(--mono)', knobs: ['Glide', 'Patch'] },
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

// Mono's controls (spec 004). The view sends values and shows what the
// engine reports back (`params`), so a preset moves the sliders.
const val = (id: ParamId) => params.values[id] ?? 0
const waves = Object.entries(Waveform)
const colours = Object.entries(NoiseColour)
const presets = Object.entries(Preset)
const vcos = [
  { n: 1, wave: Param.Vco1Wave, coarse: Param.Vco1Coarse, fine: Param.Vco1Fine, level: Param.Vco1Level },
  { n: 2, wave: Param.Vco2Wave, coarse: Param.Vco2Coarse, fine: Param.Vco2Fine, level: Param.Vco2Level, sync: Param.Vco2Sync },
  { n: 3, wave: Param.Vco3Wave, coarse: Param.Vco3Coarse, fine: Param.Vco3Fine, level: Param.Vco3Level, sync: Param.Vco3Sync },
]
const adsr = [
  { id: Param.AdsrAttack, name: 'Attack', min: 0.001, max: 2, step: 0.001 },
  { id: Param.AdsrDecay, name: 'Decay', min: 0.001, max: 4, step: 0.001 },
  { id: Param.AdsrSustain, name: 'Sustain', min: 0, max: 1, step: 0.01 },
  { id: Param.AdsrRelease, name: 'Release', min: 0.001, max: 4, step: 0.001 },
]
function loadPreset(e: Event) {
  const select = e.target as HTMLSelectElement
  if (select.value !== '') getEngine()?.preset(Number(select.value) as PresetId)
  // Hand the keys back to the computer keyboard.
  select.blur()
}
// The cutoff slider is exponential: 0..1 is 20 Hz..20 kHz.
const cutoffHz = (t: number) => 20 * 1000 ** t
const cutoffPos = (hz: number) => (hz > 0 ? Math.log(hz / 20) / Math.log(1000) : 0)
function sendCutoff(e: Event) {
  getEngine()?.param(Param.Cutoff, cutoffHz(Number((e.target as HTMLInputElement).value)))
}
function send(id: ParamId, e: Event) {
  const t = e.target as HTMLInputElement | HTMLSelectElement
  getEngine()?.param(id, t instanceof HTMLInputElement && t.type === 'checkbox' ? Number(t.checked) : Number(t.value))
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
          <template v-if="c.id === Source.Mono">
            <label v-for="a in adsr" :key="a.id">{{ a.name }}
              <input
                type="range" :min="a.min" :max="a.max" :step="a.step" :value="val(a.id)" :disabled="!status.running"
                @input="send(a.id, $event)"
              />
            </label>
          </template>
          <template v-else>
            <label>Attack <input v-model.number="attack" type="range" min="0.001" max="2" step="0.001" /></label>
            <label>Release <input v-model.number="release" type="range" min="0.005" max="4" step="0.005" /></label>
          </template>
          <span v-for="k in c.knobs" :key="k" class="knob soon" :title="c.mvp">{{ k }}</span>
        </div>
        <fieldset v-if="c.id === Source.Mono" class="vcos" :disabled="!status.running">
          <label class="preset">Preset
            <select :disabled="!status.running" @change="loadPreset">
              <option value="">—</option>
              <option v-for="[name, id] in presets" :key="id" :value="id">{{ name }}</option>
            </select>
          </label>
          <div v-for="v in vcos" :key="v.n" class="vco">
            <b>VCO {{ v.n }}</b>
            <select :value="val(v.wave)" @change="send(v.wave, $event)">
              <option v-for="[name, id] in waves" :key="id" :value="id">{{ name }}</option>
            </select>
            <label>Coarse <input type="range" min="-24" max="24" step="1" :value="val(v.coarse)" @input="send(v.coarse, $event)" /></label>
            <label>Fine <input type="range" min="-50" max="50" step="1" :value="val(v.fine)" @input="send(v.fine, $event)" /></label>
            <label>Level <input type="range" min="0" max="1" step="0.01" :value="val(v.level)" @input="send(v.level, $event)" /></label>
            <label v-if="v.sync !== undefined" class="sync"><input type="checkbox" :checked="val(v.sync) >= 0.5" @change="send(v.sync, $event)" /> Sync to 1</label>
          </div>
          <label>Pulse width <input type="range" min="0.05" max="0.95" step="0.01" :value="val(Param.PulseWidth)" @input="send(Param.PulseWidth, $event)" /></label>
          <div class="vco">
            <b>Noise</b>
            <select :value="val(Param.NoiseColour)" @change="send(Param.NoiseColour, $event)">
              <option v-for="[name, id] in colours" :key="id" :value="id">{{ name }}</option>
            </select>
            <label>Level <input type="range" min="0" max="1" step="0.01" :value="val(Param.NoiseLevel)" @input="send(Param.NoiseLevel, $event)" /></label>
          </div>
          <div class="vco">
            <b>Ladder</b>
            <label>Cutoff <input type="range" min="0" max="1" step="0.001" :value="cutoffPos(val(Param.Cutoff))" @input="sendCutoff" /></label>
            <label>Resonance <input type="range" min="0" max="1" step="0.01" :value="val(Param.Resonance)" @input="send(Param.Resonance, $event)" /></label>
            <span />
            <label>Drive <input type="range" min="0" max="1" step="0.01" :value="val(Param.Drive)" @input="send(Param.Drive, $event)" /></label>
          </div>
        </fieldset>
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
        <footer v-if="c.id !== Source.Mono" class="soon">first timbre until {{ c.mvp }}</footer>
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
.vcos { border: 0; margin: 0; padding: 0; min-width: 0; display: grid; gap: 6px; font-size: 11px; color: var(--muted); }
.vco { display: grid; grid-template-columns: auto 1fr 1fr; gap: 4px 8px; align-items: center; }
.vco b { color: var(--c); }
.vco label, .vcos > label { display: grid; gap: 2px; }
.vco .sync { display: flex; gap: 4px; align-items: center; }
.preset { display: flex; gap: 8px; align-items: center; }
.knobs label { display: grid; gap: 2px; font-size: 11px; color: var(--muted); }
.knob { border: 1px dashed var(--line); border-radius: 3px; padding: 3px 6px; }
.pads { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; }
.pads button { height: 44px; border-color: var(--c); }
.kbd { display: flex; height: 64px; gap: 1px; }
.key { flex: 1; background: #d9dde3; border-radius: 0 0 3px 3px; }
.key.black { background: #2b2f36; height: 62%; }
.key:active { background: var(--c); }
</style>
