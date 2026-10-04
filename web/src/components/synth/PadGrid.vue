<script setup lang="ts">
// The pad sampler's faceplate (#125): the kits `make samples` fetched, the sample store, a 4 x 4
// grid of pads to play and select (pad 1 at the bottom left, as on an MPC), and the selected
// pad's settings with its waveform. The engine holds the pads and plays them (padsampler.rs);
// this forwards hits, files and field changes and draws what it reports (ADR-0001).
import { computed, onMounted, ref } from 'vue'
import { clearPads, fetchPacks, getEngine, loadKit, packs, padsOf, requestPads, sampleStore, setPad, status } from '../../audio/engine'
import { PadField } from '../../audio/params'
import { PADS, PAD_FIRST_NOTE, PAD_ROWS, keyName, peakPath, type Pad } from '../../audio/sampler'
import SampleSlots from './SampleSlots.vue'

const props = defineProps<{ s: number }>()

const pads = computed(() => padsOf(props.s))
const selected = ref(0)
const pad = computed<Pad | undefined>(() => pads.value[selected.value])
const sample = computed(() => (pad.value && pad.value.sample >= 0 ? (sampleStore.slots[pad.value.sample] ?? null) : null))
const loaded = computed(() => sampleStore.slots.flatMap((info, slot) => (info ? [{ slot, info }] : [])))
const WAVE_W = 640
const WAVE_H = 60
const wave = computed(() => (sample.value ? peakPath(sample.value.peaks, WAVE_W, WAVE_H) : ''))
const label = (i: number) => {
  const slot = pads.value[i]?.sample ?? -1
  return slot >= 0 ? (sampleStore.slots[slot]?.name ?? '?') : ''
}

onMounted(() => {
  if (!packs.loaded) void fetchPacks()
  requestPads(props.s)
})

// Playing a pad: the engine decides what a hit does (one-shot, choke, fade on release).
const lit = ref(new Set<number>())
function hit(i: number) {
  selected.value = i
  lit.value.add(i)
  getEngine()?.noteOn(props.s, PAD_FIRST_NOTE + i, 0.9)
}
function lift(i: number) {
  if (!lit.value.delete(i)) return
  getEngine()?.noteOff(props.s, PAD_FIRST_NOTE + i)
}

const field = (f: number, e: Event) => {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isFinite(v)) setPad(props.s, selected.value, f, v)
}
// Putting a sample on the selected pad; a first sample added goes on a pad that has none.
const assign = (slot: number) => setPad(props.s, selected.value, PadField.Sample, slot)
const onAdded = (slot: number) => { if (pad.value && pad.value.sample < 0) assign(slot) }
const inUse = (slot: number) => pads.value.some((p) => p.sample === slot)
const CHOKE_COLOURS = ['', '#e8554a', '#e8a33a', '#7cc46a', '#4fb3c9', '#6f86e0', '#b57ce0', '#e07cb5', '#a0a0a0']
</script>

<template>
  <div class="pads">
    <div class="row">
      <div class="box kits">
        <h4>Kits</h4>
        <p v-if="packs.loaded && !packs.kits.length" class="hint">None fetched: run <code>make samples</code>.</p>
        <ul v-else>
          <li v-for="k in packs.kits" :key="k.id">
            <button :disabled="!status.running || !!sampleStore.busy" :title="`${k.license}. ${k.credit}`" @click="loadKit(s, k)">{{ k.name }}</button>
          </li>
        </ul>
        <p v-if="sampleStore.busy" class="hint" role="status">Loading {{ sampleStore.busy }}</p>
        <button class="clear" :disabled="!status.running" @click="clearPads(s)">Clear pads</button>
      </div>
      <SampleSlots use-label="→ pad" :use-title="`Put it on pad ${selected + 1}`" :in-use="inUse" @use="assign" @added="onAdded" />
    </div>

    <div class="box">
      <h4>Pads <small>notes {{ keyName(PAD_FIRST_NOTE) }}–{{ keyName(PAD_FIRST_NOTE + PADS - 1) }}</small></h4>
      <div class="grid" role="group" aria-label="Pads">
        <template v-for="row in PAD_ROWS" :key="row[0]">
          <button
            v-for="i in row" :key="i" class="pad" :class="{ sel: i === selected, lit: lit.has(i), empty: (pads[i]?.sample ?? -1) < 0 }"
            :aria-label="`Pad ${i + 1}`" :aria-pressed="i === selected" :disabled="!status.running"
            @pointerdown="hit(i)" @pointerup="lift(i)" @pointerleave="lift(i)" @pointercancel="lift(i)"
          >
            <b>{{ i + 1 }}</b>
            <span class="nm">{{ label(i) }}</span>
            <i v-if="(pads[i]?.choke ?? 0) > 0" class="choke" :style="{ background: CHOKE_COLOURS[pads[i]?.choke ?? 0] }" :title="`Choke group ${pads[i]?.choke}`" />
          </button>
        </template>
      </div>
    </div>

    <div v-if="pad" class="box edit">
      <h4>Pad {{ selected + 1 }}: {{ sample?.name ?? 'no sample' }} <small>note {{ keyName(PAD_FIRST_NOTE + selected) }}</small></h4>
      <svg v-if="sample" class="wave" :viewBox="`0 0 ${WAVE_W} ${WAVE_H}`" preserveAspectRatio="none" role="img" aria-label="Waveform"><path :d="wave" /></svg>
      <div class="fields">
        <label>Sample
          <select :value="pad.sample" @change="field(PadField.Sample, $event)">
            <option :value="-1">none</option>
            <option v-for="{ slot, info } in loaded" :key="slot" :value="slot">{{ info.name }}</option>
          </select>
        </label>
        <label>Tune st <input type="number" min="-24" max="24" step="1" :value="pad.tune" @change="field(PadField.Tune, $event)" /></label>
        <label>Level <input type="number" min="0" max="2" step="0.05" :value="pad.level" @change="field(PadField.Level, $event)" /></label>
        <label>Pan <input type="number" min="-1" max="1" step="0.1" :value="pad.pan" title="−1 left, 1 right" @change="field(PadField.Pan, $event)" /></label>
        <label>Decay s <input type="number" min="0" max="10" step="0.05" :value="pad.decay" title="0 plays the sample out" @change="field(PadField.Decay, $event)" /></label>
        <label>Choke <input type="number" min="0" max="8" step="1" :value="pad.choke" title="Pads of the same group cut each other off; 0 is none" @change="field(PadField.Choke, $event)" /></label>
        <label>Vel → level <input type="number" min="0" max="1" step="0.1" :value="pad.velLevel" @change="field(PadField.VelLevel, $event)" /></label>
        <label>Vel → start <input type="number" min="0" max="1" step="0.1" :value="pad.velStart" title="How much a soft hit starts later in the sample" @change="field(PadField.VelStart, $event)" /></label>
        <label class="check"><input type="checkbox" :checked="pad.oneShot" @change="setPad(s, selected, PadField.OneShot, Number(($event.target as HTMLInputElement).checked))" /> One-shot</label>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pads { display: flex; flex-direction: column; gap: 10px; font-size: 12px; min-width: min(100%, 680px); }
.row { display: flex; flex-wrap: wrap; gap: 10px; }
.box { flex: 1 1 260px; padding: 8px 10px; border: 1px solid var(--trim); border-radius: 4px; background: color-mix(in srgb, var(--plate) 88%, black 12%); }
h4 { margin: 0 0 6px; font-size: 11px; letter-spacing: 0.16em; text-transform: uppercase; color: var(--c); display: flex; align-items: baseline; gap: 8px; }
h4 small { font-weight: 400; letter-spacing: 0.06em; text-transform: none; color: var(--con-silk-dim); }
ul { list-style: none; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: 6px; }
button { font: inherit; color: var(--con-silk); background: var(--plate); border: 1px solid var(--trim); border-radius: 3px; padding: 2px 8px; cursor: pointer; }
button:disabled { opacity: 0.45; cursor: default; }
.clear { margin-top: 8px; }
.grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; max-width: 460px; }
.pad {
  position: relative; aspect-ratio: 1.3; display: flex; flex-direction: column; align-items: flex-start; justify-content: space-between;
  padding: 6px 8px; border-radius: 6px; text-align: left; touch-action: none; user-select: none;
  background: linear-gradient(180deg, color-mix(in srgb, var(--plate) 70%, white 10%), color-mix(in srgb, var(--plate) 80%, black 20%));
  box-shadow: 0 2px 0 #0008, 0 1px 0 #ffffff14 inset;
}
.pad b { font-size: 14px; color: var(--con-silk); }
.pad .nm { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 10px; color: var(--con-silk-dim); }
.pad.empty { opacity: 0.6; }
.pad.sel { outline: 2px solid var(--c); }
.pad.lit { background: color-mix(in srgb, var(--c) 70%, var(--plate)); box-shadow: 0 0 0 #0008, 0 0 14px color-mix(in srgb, var(--c) 60%, transparent); transform: translateY(1px); }
.choke { position: absolute; top: 7px; right: 7px; width: 8px; height: 8px; border-radius: 50%; }
.wave { width: 100%; height: 60px; display: block; background: #0b0d10; border-radius: 3px; }
.wave path { fill: var(--c); opacity: 0.85; }
.fields { display: flex; flex-wrap: wrap; gap: 8px 14px; margin-top: 8px; align-items: center; }
label { display: flex; gap: 4px; align-items: center; color: var(--con-silk-dim); }
input[type='number'] { width: 58px; font: inherit; background: var(--plate); color: var(--con-silk); border: 1px solid var(--trim); border-radius: 3px; padding: 1px 3px; }
select { font: inherit; background: var(--plate); color: var(--con-silk); border: 1px solid var(--trim); border-radius: 3px; max-width: 150px; }
.hint { margin: 4px 0 0; color: var(--con-silk-dim); font-style: italic; }
code { font-family: var(--con-font-mono); }
</style>
