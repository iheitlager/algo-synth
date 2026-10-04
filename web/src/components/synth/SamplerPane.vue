<script setup lang="ts">
// The sampler's faceplate (#125): the packs `make samples` fetched, the sample store, a
// zone map (key across, velocity up), the selected zone's fields and its waveform with the
// loop markers. The engine parses the WAV bytes, resamples them and holds the zones; this
// forwards files and field changes and draws what the engine reports (ADR-0001).
import { computed, onMounted, ref } from 'vue'
import { clearZones, fetchPacks, loadPack, mapSample, packs, requestZones, sampleStore, setZone, status, zonesOf } from '../../audio/engine'
import { LoopMode, ZoneField } from '../../audio/params'
import { ZONES, keyName, loopOf, peakPath, zoneRect, type Zone } from '../../audio/sampler'
import SampleSlots from './SampleSlots.vue'

const props = defineProps<{ s: number }>()

const zones = computed(() => zonesOf(props.s))
const used = computed(() => zones.value.map((z, i) => [z, i] as const).filter(([z]) => z.sample >= 0))
const selected = ref(-1)
const zone = computed<Zone | null>(() => (used.value.some(([, i]) => i === selected.value) ? (zones.value[selected.value] ?? null) : null))
const sample = computed(() => (zone.value ? (sampleStore.slots[zone.value.sample] ?? null) : null))
const loaded = computed(() => sampleStore.slots.flatMap((info, slot) => (info ? [{ slot, info }] : [])))

onMounted(() => {
  if (!packs.loaded) void fetchPacks()
  requestZones(props.s)
})

const MAP_W = 640
const MAP_H = 127
const WAVE_W = 640
const WAVE_H = 70
const rects = computed(() => used.value.map(([z, i]) => ({ i, z, ...zoneRect(z, MAP_W, MAP_H) })))
const wave = computed(() => (sample.value ? peakPath(sample.value.peaks, WAVE_W, WAVE_H) : ''))
const loop = computed(() => {
  const info = sample.value
  const z = zone.value
  const l = z && info ? loopOf(z, info) : null
  return l && info ? l.map((f) => (f / info.frames) * WAVE_W) : null
})
const rootName = (z: Zone) => (z.root >= 0 ? keyName(z.root) : sample.value ? keyName(sample.value.root) : '—')
const field = (f: number, e: Event, int = false) => {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isFinite(v)) setZone(props.s, selected.value, f, int ? Math.round(v) : v)
}

// A first sample on an empty map plays across the keys.
function onAdded(slot: number) {
  if (used.value.length === 0) mapSample(props.s, slot)
}
function addZone(slot: number) {
  mapSample(props.s, slot)
  const free = zonesOf(props.s).findIndex((z) => z.sample < 0)
  selected.value = free
}
const removeZone = () => { setZone(props.s, selected.value, ZoneField.Sample, -1); selected.value = -1 }
const inUse = (slot: number) => zones.value.some((z) => z.sample === slot)
</script>

<template>
  <div class="sampler">
    <div class="row">
      <div class="box packs">
        <h4>Packs</h4>
        <p v-if="packs.loaded && !packs.list.length" class="hint">None fetched: run <code>make samples</code>.</p>
        <ul v-else>
          <li v-for="p in packs.list" :key="p.id">
            <button :disabled="!status.running || !!sampleStore.busy" :title="`${p.license}. ${p.credit}`" @click="loadPack(s, p)">{{ p.name }}</button>
          </li>
        </ul>
        <p v-if="sampleStore.busy" class="hint" role="status">Loading {{ sampleStore.busy }}</p>
      </div>

      <SampleSlots
        use-label="+ zone" use-title="Play it across the keys" :in-use="inUse" @use="addZone" @added="onAdded"
      />
    </div>

    <div class="box">
      <h4>Zones <small>{{ used.length }} of {{ ZONES }} · key across, velocity up</small>
        <button class="clear" :disabled="!used.length" @click="clearZones(s); selected = -1">Clear zones</button>
      </h4>
      <svg class="map" :viewBox="`0 0 ${MAP_W} ${MAP_H}`" preserveAspectRatio="none" role="img" aria-label="Zone map">
        <g class="octaves">
          <line v-for="o in 10" :key="o" :x1="(o * 12 / 128) * MAP_W" :x2="(o * 12 / 128) * MAP_W" y1="0" :y2="MAP_H" />
        </g>
        <rect
          v-for="r in rects" :key="r.i" class="zone" :class="{ sel: r.i === selected, rel: r.z.release }" :x="r.x" :y="r.y"
          :width="r.w" :height="r.h" @click="selected = r.i"
        >
          <title>{{ sampleStore.slots[r.z.sample]?.name }}: {{ keyName(r.z.keyLo) }}–{{ keyName(r.z.keyHi) }}, velocity {{ r.z.velLo }}–{{ r.z.velHi }}</title>
        </rect>
      </svg>
      <p v-if="!used.length" class="hint">No zones: load a pack, or add a WAV and give it a zone.</p>
    </div>

    <div v-if="zone" class="box edit">
      <h4>Zone {{ selected + 1 }}: {{ sample?.name }} <small>root {{ rootName(zone) }}</small></h4>
      <svg class="wave" :viewBox="`0 0 ${WAVE_W} ${WAVE_H}`" preserveAspectRatio="none" role="img" aria-label="Waveform">
        <path :d="wave" />
        <g v-if="loop"><line :x1="loop[0]" :x2="loop[0]" y1="0" :y2="WAVE_H" /><line :x1="loop[1]" :x2="loop[1]" y1="0" :y2="WAVE_H" /></g>
      </svg>
      <div class="fields">
        <label>Sample
          <select :value="zone.sample" @change="field(ZoneField.Sample, $event, true)">
            <option v-for="{ slot, info } in loaded" :key="slot" :value="slot">{{ info.name }}</option>
          </select>
        </label>
        <label>Key <input type="number" min="0" max="127" :value="zone.keyLo" aria-label="Lowest key" @change="field(ZoneField.KeyLo, $event, true)" />
          – <input type="number" min="0" max="127" :value="zone.keyHi" aria-label="Highest key" @change="field(ZoneField.KeyHi, $event, true)" /></label>
        <label>Velocity <input type="number" min="1" max="127" :value="zone.velLo" aria-label="Lowest velocity" @change="field(ZoneField.VelLo, $event, true)" />
          – <input type="number" min="1" max="127" :value="zone.velHi" aria-label="Highest velocity" @change="field(ZoneField.VelHi, $event, true)" /></label>
        <label>Root <input type="number" min="-1" max="127" :value="zone.root" title="−1 takes the sample's own" @change="field(ZoneField.Root, $event, true)" /></label>
        <label>Tune ct <input type="number" min="-1200" max="1200" :value="zone.tune" @change="field(ZoneField.Tune, $event)" /></label>
        <label>Level <input type="number" min="0" max="2" step="0.05" :value="zone.level" @change="field(ZoneField.Level, $event)" /></label>
        <label>Loop
          <select :value="zone.loop" @change="field(ZoneField.Loop, $event, true)">
            <option :value="LoopMode.Off">Off</option>
            <option :value="LoopMode.Loop">Loop</option>
            <option :value="LoopMode.Sustain">While held</option>
          </select>
        </label>
        <label>Loop frames <input type="number" min="0" :value="zone.loopStart" title="0 and 0 take the sample's own" aria-label="Loop start" @change="field(ZoneField.LoopStart, $event, true)" />
          – <input type="number" min="0" :value="zone.loopEnd" aria-label="Loop end" @change="field(ZoneField.LoopEnd, $event, true)" /></label>
        <label>Round-robin <input type="number" min="1" max="16" :value="zone.seqPos" aria-label="Turn" @change="field(ZoneField.SeqPos, $event, true)" />
          of <input type="number" min="1" max="16" :value="zone.seqLen" aria-label="Turns" @change="field(ZoneField.SeqLen, $event, true)" /></label>
        <label class="check"><input type="checkbox" :checked="zone.release" @change="setZone(s, selected, ZoneField.Release, Number(($event.target as HTMLInputElement).checked))" /> On release</label>
        <button class="del" @click="removeZone">Delete zone</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sampler { display: flex; flex-direction: column; gap: 10px; font-size: 12px; min-width: min(100%, 680px); }
.row { display: flex; flex-wrap: wrap; gap: 10px; }
.box { flex: 1 1 260px; padding: 8px 10px; border: 1px solid var(--trim); border-radius: 4px; background: color-mix(in srgb, var(--plate) 88%, black 12%); }
h4 { margin: 0 0 6px; font-size: 11px; letter-spacing: 0.16em; text-transform: uppercase; color: var(--c); display: flex; align-items: baseline; gap: 8px; }
h4 small { font-weight: 400; letter-spacing: 0.06em; text-transform: none; color: var(--con-silk-dim); }
.clear { margin-left: auto; }
ul { list-style: none; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: 6px; }
button { font: inherit; color: var(--con-silk); background: var(--plate); border: 1px solid var(--trim); border-radius: 3px; padding: 2px 8px; cursor: pointer; }
button:disabled { opacity: 0.45; cursor: default; }
.map, .wave { width: 100%; height: 130px; display: block; background: #0b0d10; border-radius: 3px; }
.wave { height: 70px; }
.octaves line { stroke: #ffffff14; stroke-width: 1; vector-effect: non-scaling-stroke; }
.zone { fill: color-mix(in srgb, var(--c) 45%, transparent); stroke: var(--c); stroke-width: 1; vector-effect: non-scaling-stroke; cursor: pointer; }
.zone.rel { fill: color-mix(in srgb, #e0a040 45%, transparent); stroke: #e0a040; }
.zone.sel { fill: color-mix(in srgb, var(--c) 80%, white 10%); stroke: #fff; stroke-width: 2; }
.wave path { fill: var(--c); opacity: 0.85; }
.wave line { stroke: #fff; stroke-width: 1; stroke-dasharray: 4 3; vector-effect: non-scaling-stroke; }
.fields { display: flex; flex-wrap: wrap; gap: 8px 14px; margin-top: 8px; align-items: center; }
label { display: flex; gap: 4px; align-items: center; color: var(--con-silk-dim); }
input[type='number'] { width: 58px; font: inherit; background: var(--plate); color: var(--con-silk); border: 1px solid var(--trim); border-radius: 3px; padding: 1px 3px; }
select { font: inherit; background: var(--plate); color: var(--con-silk); border: 1px solid var(--trim); border-radius: 3px; max-width: 150px; }
.hint { margin: 4px 0 0; color: var(--con-silk-dim); font-style: italic; }
.err { margin: 4px 0 0; color: #e66; }
code { font-family: var(--con-font-mono); }
</style>
