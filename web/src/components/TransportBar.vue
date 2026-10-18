<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import {
  clearAll, engineBuild, getEngine, meter, opened, openFiles, params, pauseSong, playSong, power, saveSong, song, songChanged, songPosition,
  status, stopSong, view,
} from '../audio/engine'
import { canPick, isSongFile, pickOpen } from '../audio/songfile'
import { bounce, cancelExport, exportWav, FORMATS } from '../audio/bounce'
import { assistant } from '../audio/assistlink'
import { details, page } from '../audio/buildinfo'
import { Param } from '../audio/params'
import { midi } from '../audio/midiin'

// The performance counter: worklet time per block against the budget
// (plan.md "Performance budget"; tools/bench.mjs uses the same figure).
const BUDGET = 0.25
const pct = (x: number) => `${(100 * x).toFixed(1)}%`

// Master gain is global: the engine's value, so a setup moves the slider.
const gain = () => params.values[0]?.[Param.MasterGain] ?? 0.5
const sendGain = (e: Event) => getEngine()?.param(0, Param.MasterGain, Number((e.target as HTMLInputElement).value))

// Oscilloscope from the AnalyserNode (ADR-0003): drawing only, no audio work.
const scope = ref<HTMLCanvasElement | null>(null)
let raf = 0
function draw() {
  const eng = getEngine()
  const c = scope.value
  if (eng && c) {
    const g = c.getContext('2d')
    const data = new Float32Array(eng.analyser.fftSize)
    eng.analyser.getFloatTimeDomainData(data)
    if (g) {
      g.clearRect(0, 0, c.width, c.height)
      g.strokeStyle = '#f0a23b'
      g.lineWidth = 1.5
      g.beginPath()
      for (let i = 0; i < data.length; i++) {
        const x = (i / data.length) * c.width
        const y = (0.5 - (data[i] ?? 0) * 0.45) * c.height
        if (i === 0) g.moveTo(x, y)
        else g.lineTo(x, y)
      }
      g.stroke()
    }
  }
  raf = requestAnimationFrame(draw)
}
async function onPower() {
  await power()
  if (!raf) draw()
}
onBeforeUnmount(() => cancelAnimationFrame(raf))

// Start over (#325): nothing of it can be undone, so ask first.
function openLab() {
  window.open(`${import.meta.env.BASE_URL}spectral-lab.html`, 'algo-synth-spectral-lab', 'popup,width=960,height=820')
}
async function onNew() {
  if (!window.confirm('Start over? The song, every synth and the mix are discarded.')) return
  await onPower()
  clearAll()
}

// Export WAV (#705): the picked format starts the bounce; the picker shows its label again.
function onExport(e: Event) {
  const select = e.target as HTMLSelectElement
  const format = FORMATS[Number(select.value)]
  select.value = ''
  if (format) void exportWav(format)
}

// A MIDI file, a setup (.synths.json), a song (.song), or several at once (#41, #105).
async function onFile(e: Event) {
  const input = e.target as HTMLInputElement
  const files = Array.from(input.files ?? [])
  input.value = ''
  if (!files.length) return
  await openFiles(files)
  await onPower()
}
// Where the browser can save in place (#465), Open… keeps the song file's handle for Save.
const picking = canPick()
async function onPick() {
  const picked = await pickOpen()
  if (!picked.length) return
  await openFiles(picked.map((p) => p.file), picked.find((p) => isSongFile(p.file.name))?.handle ?? null)
  await onPower()
}
const saveTitle = () =>
  !songChanged.value ? 'Nothing changed since the song was opened or saved'
  : picking && opened.name ? `Save the song into ${opened.name}`
  : picking ? 'Save the song as a .song file'
  // Without the File System Access API (Brave turns it off) Save can't write into the file it opened.
  : 'Download the song as .song text; this browser cannot save into the opened file (Brave: enable brave://flags/#file-system-access-api)'

// Which build is running (#197): the page's version and the engine's, with the commits in the details.
const buildText = () => details(engineBuild)
const CHANGELOG = 'https://github.com/iheitlager/algo-synth/blob/main/CHANGELOG.md'
const copied = ref(false)
// The bar hides its overflow, so the details float as `fixed`, placed under the summary when opened.
const popAt = ref({ top: '0px', left: '0px' })
function placeBuild(e: Event) {
  const r = (e.target as HTMLDetailsElement).querySelector('summary')!.getBoundingClientRect()
  popAt.value = { top: `${r.bottom + 6}px`, left: `${r.left}px` }
}
async function copyBuild() {
  try {
    await navigator.clipboard.writeText(buildText())
    copied.value = true
    setTimeout(() => (copied.value = false), 1500)
  } catch {
    // No clipboard here (an insecure origin, say): the text is on show anyway.
  }
}
// The one transport (ADR-0022): play and pause the song, stop back to the top.
const toggle = () => (song.playing ? pauseSong() : playSong())
// The scene playing now, in an arrangement.
const scene = computed(() => (song.entry >= 0 ? song.scenes[song.arrange[song.entry] ?? -1] : undefined))
</script>

<template>
  <header class="pane bar">
    <strong class="logo">algo-synth</strong>
    <details class="build" @toggle="placeBuild">
      <summary :title="buildText()">v{{ page.version }}<template v-if="status.running"> · engine v{{ engineBuild.version || '?' }}</template></summary>
      <div class="build-pop" :style="popAt">
        <pre>{{ buildText() }}</pre>
        <button @click="copyBuild">{{ copied ? 'Copied' : 'Copy' }}</button>
        <a :href="CHANGELOG" target="_blank" rel="noopener">Changelog</a>
      </div>
    </details>
    <!-- Only until audio runs: the browser needs a click to start sound; after it the button would only say so. -->
    <button v-if="!status.running" @click="onPower">Power on</button>
    <span class="seg" role="group" aria-label="View">
      <button :aria-pressed="view.main === 'synths'" @click="view.main = 'synths'">Synths</button>
      <button :aria-pressed="view.main === 'mixer'" @click="view.main = 'mixer'">Mixer</button>
      <button :aria-pressed="view.main === 'composer'" @click="view.main = 'composer'">Composer</button>
      <button :aria-pressed="view.main === 'decks'" @click="view.main = 'decks'">Decks</button>
    </span>
    <!-- The Assistant (#387): a pane beside the view, which can pop out into its own window. -->
    <button :aria-pressed="assistant.shown" :class="{ on: assistant.shown }" title="Ask a language model to change the song" @click="assistant.shown = !assistant.shown">Assistant</button>
    <!-- The Spectral Lab (ADR-0017): its own window, with its own engine. -->
    <button title="Take a WAV apart and resynthesise it, in a window of its own" @click="openLab">Lab</button>
    <button title="Discard the song, the synths and the mix; start with one Modular synth" @click="onNew">New</button>
    <button v-if="picking" title="A MIDI file, its .synths.json setup, a .song, or several" @click="onPick">Open…</button>
    <label v-else class="file" title="A MIDI file, its .synths.json setup, a .song, or several">
      <input type="file" multiple accept=".mid,.midi,audio/midi,.json,application/json,.song" @change="onFile" />Open…
    </label>
    <!-- Save writes the song back into its file, lit when it changed; Save as… picks a new one (#465). -->
    <button :disabled="!status.running || !songChanged" :title="saveTitle()" @click="saveSong()">Save<template v-if="songChanged"> ●</template></button>
    <button :disabled="!status.running || !song.text" title="Save the song into a new .song file" @click="saveSong(true)">Save as…</button>
    <!-- Export WAV (#705): the song bounced offline into a file, faster than it plays; how far, and Cancel, while it runs. -->
    <span v-if="bounce.busy" class="field" title="The song is bouncing to a WAV file">
      Export <progress :value="bounce.progress" max="1" /> <button @click="cancelExport">Cancel</button>
    </span>
    <select
      v-else class="picker export" aria-label="Export the song as a WAV file" :disabled="!status.running || !song.text"
      title="Bounce the song to a WAV file, faster than it plays: its arrangement, then the tail" @change="onExport"
    >
      <option value="">Export WAV…</option>
      <option v-for="(f, i) in FORMATS" :key="f.label" :value="i">{{ f.label }}</option>
    </select>
    <!-- The transport, the only one (ADR-0022): the song plays, pauses where it is, stops back to the top. -->
    <button :disabled="!status.running" :class="{ on: song.playing }" @click="toggle">{{ song.playing ? '❚❚ Pause' : '▶ Play' }}</button>
    <button :disabled="!status.running" title="Stop and go back to the top" @click="stopSong">■ Stop</button>
    <span v-if="songPosition >= 0" class="field">
      <b>bar {{ Math.floor(songPosition / 16) + 1 }}</b> · step {{ (songPosition % 16) + 1 }}<template v-if="scene"> · {{ scene.name }}</template>
    </span>
    <button :disabled="!status.running" @click="getEngine()?.panic()">All notes off</button>
    <!-- A controller connected (#10): its keys play the selected synth; the light blinks on input. -->
    <span v-if="midi.inputs.length" class="field midi" :class="{ on: midi.active }" :title="`MIDI in: ${midi.inputs.join(', ')} → the selected synth`">MIDI</span>
    <span v-else-if="midi.error" class="field muted" :title="midi.error">no MIDI</span>
    <label class="field gain">Master <input type="range" min="0" max="1" step="0.01" :value="gain()" @input="sendGain" /></label>
    <canvas ref="scope" class="scope" width="360" height="40" />
    <span
      v-if="status.running && meter.seen" class="field meter" :class="{ over: (meter.peak ?? meter.load) > BUDGET }"
      title="Time in the audio callback as a share of real time; the budget is 25% (plan.md)"
    >
      DSP <b>{{ meter.peak === null ? '≈' : '' }}{{ pct(meter.load) }}</b>
      <template v-if="meter.peak !== null">· peak {{ pct(meter.peak) }}</template>
      · {{ meter.voices }} {{ meter.voices === 1 ? 'voice' : 'voices' }}
    </span>
    <span class="field muted status">
      {{ status.error || (status.running ? `${status.sampleRate} Hz · wasm worklet` : 'click Power on to start audio') }}
    </span>
  </header>
</template>

<style scoped>
/* The controls keep their size and one height; a bar too narrow for them
   wraps onto a second row rather than squeezing their labels or hiding them. */
.bar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 12px; padding: 8px 12px; overflow: hidden; }
.bar > * { flex-shrink: 0; }
.bar button, .bar .file, .bar .picker { height: 28px; white-space: nowrap; }
.bar .file { display: inline-flex; align-items: center; }
.bar .picker { width: 9em; min-width: 0; font: inherit; letter-spacing: normal; border-radius: 4px; padding-top: 0; padding-bottom: 0; border-color: var(--line); box-shadow: none; background: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M1 1l4 4 4-4' fill='none' stroke='%23d9dde3' stroke-width='1.6' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E") no-repeat right 11px center, var(--panel-2); }
.bar .picker.export { width: 11em; }
.bar > .status { flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; display: block; }
.logo { color: var(--accent); letter-spacing: 0.04em; margin-right: 8px; }
.build { position: relative; color: var(--muted); font-size: 12px; white-space: nowrap; }
.build summary { cursor: pointer; }
.build-pop { position: fixed; z-index: 10; padding: 8px 10px; background: var(--panel); border: 1px solid var(--line); border-radius: 4px; display: flex; flex-direction: column; gap: 6px; }
.build-pop a { color: var(--accent); }
.build-pop pre { margin: 0; font-family: var(--font-mono); color: inherit; }
.on { border-color: var(--accent); color: var(--accent); }
.seg { display: inline-flex; }
.seg button { border-radius: 0; position: relative; }
/* Neighbours overlap by their border, so each line between them is one line. */
.seg button + button { margin-left: -1px; }
.seg button:first-child { border-radius: 4px 0 0 4px; }
.seg button:last-child { border-radius: 0 4px 4px 0; }
.seg button:hover, .seg button[aria-pressed='true'] { z-index: 1; }
.seg button[aria-pressed='true'] { border-color: var(--accent); color: var(--accent); background: var(--panel); }
.field { display: flex; align-items: center; gap: 6px; white-space: nowrap; }
.gain { width: 180px; }
.file { border: 1px solid var(--line); border-radius: 4px; padding: 4px 10px; background: var(--panel-2); cursor: pointer; white-space: nowrap; }
.file:hover { border-color: var(--accent); }
.file input { display: none; }
.muted { color: var(--muted); }
.meter { font-variant-numeric: tabular-nums; }
.meter.over b { color: var(--accent); }
.midi { padding: 2px 6px; border: 1px solid var(--line); border-radius: 4px; color: var(--muted); font-size: 12px; }
.midi.on { border-color: var(--accent); color: var(--accent); }
.scope { background: var(--bg); border: 1px solid var(--line); border-radius: 4px; height: 28px; }
</style>
