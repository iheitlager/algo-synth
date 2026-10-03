<script setup lang="ts">
// The Mono synth (spec 004): an ARP 2600-style semi-modular voice.
import { onBeforeUnmount, onMounted } from 'vue'
import { getEngine, params, status } from '../audio/engine'
import {
  ModDest, ModSource, NoiseColour, NotePriority, Param, type ParamId, Preset, type PresetId, Waveform,
} from '../audio/params'

const keys = Array.from({ length: 25 }, (_, i) => 48 + i)
const black = (n: number) => [1, 3, 6, 8, 10].includes(n % 12)

// Mono's controls (spec 004). The view sends values and shows what the
// engine reports back (`params`), so a preset moves the sliders.
const val = (id: ParamId) => params.values[id] ?? 0
const waves = Object.entries(Waveform)
const colours = Object.entries(NoiseColour)
const presets = Object.entries(Preset)
const priorities = Object.entries(NotePriority)
const modSources = Object.entries(ModSource)
const modDests = Object.entries(ModDest)
// The patch: 8 overrides (source → destination, amount), spec 004 Req 7.
const patchSlots = Array.from({ length: 8 }, (_, i) => {
  const key = (field: string) => Param[`Patch${i + 1}${field}` as keyof typeof Param]
  return { n: i + 1, source: key('Source'), dest: key('Dest'), amount: key('Amount') }
})
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
// The LFO rate slider too: 0..1 is 0.01..50 Hz.
const lfoHz = (t: number) => 0.01 * 5000 ** t
const lfoPos = (hz: number) => (hz > 0 ? Math.log(hz / 0.01) / Math.log(5000) : 0)
function sendLfoRate(e: Event) {
  getEngine()?.param(Param.LfoRate, lfoHz(Number((e.target as HTMLInputElement).value)))
}
function sendCutoff(e: Event) {
  getEngine()?.param(Param.Cutoff, cutoffHz(Number((e.target as HTMLInputElement).value)))
}
function send(id: ParamId, e: Event) {
  const t = e.target as HTMLInputElement | HTMLSelectElement
  getEngine()?.param(id, t instanceof HTMLInputElement && t.type === 'checkbox' ? Number(t.checked) : Number(t.value))
}

const down = (n: number) => getEngine()?.noteOn(n)
const up = (n: number) => getEngine()?.noteOff(n)

// Computer keyboard: the bottom two rows play C4..E5.
const map = 'awsedftgyhujkolp;'
const held = new Set<string>()
function onKey(e: KeyboardEvent, isDown: boolean) {
  if (e.repeat || e.metaKey || e.ctrlKey || ['INPUT', 'SELECT'].includes((e.target as HTMLElement).tagName)) return
  const i = map.indexOf(e.key)
  if (i < 0) return
  const n = 60 + i
  if (isDown && !held.has(e.key)) { held.add(e.key); down(n) }
  if (!isDown) { held.delete(e.key); up(n) }
}
const kd = (e: KeyboardEvent) => onKey(e, true)
const ku = (e: KeyboardEvent) => onKey(e, false)
onMounted(() => { window.addEventListener('keydown', kd); window.addEventListener('keyup', ku) })
onBeforeUnmount(() => { window.removeEventListener('keydown', kd); window.removeEventListener('keyup', ku) })
</script>

<template>
  <section class="pane">
    <div class="pane-head">
      <span>Synths</span>
      <span class="soon">{{ status.running ? 'keys A–; play' : 'power on to play' }}</span>
    </div>
    <div class="cards">
      <article class="card sel">
        <header><b>Mono</b><span class="style">ARP 2600-style semi-modular</span></header>
        <div class="knobs">
          <label v-for="a in adsr" :key="a.id">{{ a.name }}
            <input
              type="range" :min="a.min" :max="a.max" :step="a.step" :value="val(a.id)" :disabled="!status.running"
              @input="send(a.id, $event)"
            />
          </label>
        </div>
        <fieldset class="vcos" :disabled="!status.running">
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
            <b>Keys</b>
            <select :value="val(Param.Priority)" title="Which held key sounds" @change="send(Param.Priority, $event)">
              <option v-for="[name, id] in priorities" :key="id" :value="id">{{ name }}</option>
            </select>
            <label class="sync"><input type="checkbox" :checked="val(Param.Legato) >= 0.5" @change="send(Param.Legato, $event)" /> Legato</label>
            <label>Glide <input type="range" min="0" max="2" step="0.01" :value="val(Param.Glide)" @input="send(Param.Glide, $event)" /></label>
          </div>
          <div class="vco">
            <b>Ladder</b>
            <label>Cutoff <input type="range" min="0" max="1" step="0.001" :value="cutoffPos(val(Param.Cutoff))" @input="sendCutoff" /></label>
            <label>Resonance <input type="range" min="0" max="1" step="0.01" :value="val(Param.Resonance)" @input="send(Param.Resonance, $event)" /></label>
            <span />
            <label>Drive <input type="range" min="0" max="1" step="0.01" :value="val(Param.Drive)" @input="send(Param.Drive, $event)" /></label>
          </div>
          <div class="vco">
            <b>LFO</b>
            <select :value="val(Param.LfoWave)" @change="send(Param.LfoWave, $event)">
              <option v-for="[name, id] in waves" :key="id" :value="id">{{ name === 'Pulse' ? 'Square' : name }}</option>
            </select>
            <label>Rate <input type="range" min="0" max="1" step="0.001" :value="lfoPos(val(Param.LfoRate))" @input="sendLfoRate" /></label>
          </div>
          <div class="vco">
            <b>AR</b>
            <label>Attack <input type="range" min="0.001" max="2" step="0.001" :value="val(Param.ArAttack)" @input="send(Param.ArAttack, $event)" /></label>
            <label>Release <input type="range" min="0.001" max="4" step="0.001" :value="val(Param.ArRelease)" @input="send(Param.ArRelease, $event)" /></label>
          </div>
          <div class="vco">
            <b>Normal</b>
            <label>Env → cutoff <input type="range" min="-1" max="1" step="0.01" :value="val(Param.EnvCutoff)" @input="send(Param.EnvCutoff, $event)" /></label>
            <label>Key track <input type="range" min="0" max="1" step="0.01" :value="val(Param.KeyTrack)" @input="send(Param.KeyTrack, $event)" /></label>
            <span />
            <label>Vibrato <input type="range" min="0" max="1" step="0.01" :value="val(Param.Vibrato)" @input="send(Param.Vibrato, $event)" /></label>
            <label>Mod wheel <input type="range" min="0" max="1" step="0.01" :value="val(Param.ModWheel)" @input="send(Param.ModWheel, $event)" /></label>
          </div>
          <p class="normalled">
            Normalled: VCO 1–3 + noise → ladder → VCA · ADSR → cutoff, VCA · key → pitch, cutoff · LFO × wheel → pitch.
            A patch slot replaces the normals to its destination.
          </p>
          <div v-for="slot in patchSlots" :key="slot.n" class="patch">
            <span class="soon">{{ slot.n }}</span>
            <select :value="val(slot.source)" @change="send(slot.source, $event)">
              <option v-for="[name, id] in modSources" :key="id" :value="id">{{ name }}</option>
            </select>
            <span class="soon">→</span>
            <select :value="val(slot.dest)" @change="send(slot.dest, $event)">
              <option v-for="[name, id] in modDests" :key="id" :value="id">{{ name }}</option>
            </select>
            <input type="range" min="-1" max="1" step="0.01" :value="val(slot.amount)" :title="`amount ${val(slot.amount).toFixed(2)}`" @input="send(slot.amount, $event)" />
          </div>
        </fieldset>
        <div class="kbd">
          <span
            v-for="n in keys" :key="n" class="key" :class="{ black: black(n) }"
            @pointerdown="down(n)" @pointerup="up(n)" @pointerleave="up(n)"
          />
        </div>
      </article>
    </div>
  </section>
</template>

<style scoped>
.cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(380px, 1fr)); gap: 8px; padding: 8px; }
.card { --c: var(--mono); background: var(--panel-2); border: 1px solid var(--line); border-top: 3px solid var(--c); border-radius: 4px; padding: 10px; display: grid; gap: 10px; align-content: start; cursor: pointer; }
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
.normalled { margin: 0; line-height: 1.4; }
.patch { display: grid; grid-template-columns: 1.2em 1fr auto 1fr 1fr; gap: 4px 6px; align-items: center; }
.preset { display: flex; gap: 8px; align-items: center; }
.knobs label { display: grid; gap: 2px; font-size: 11px; color: var(--muted); }
.kbd { display: flex; height: 64px; gap: 1px; }
.key { flex: 1; background: #d9dde3; border-radius: 0 0 3px 3px; }
.key.black { background: #2b2f36; height: 62%; }
.key:active { background: var(--c); }
</style>
