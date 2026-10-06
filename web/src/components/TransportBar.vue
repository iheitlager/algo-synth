<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import {
  engineBuild, getEngine, loadDemo, meter, openFiles, params, pauseSong, playSong, power, saveSetup, saveSong, song, songPosition, status, stopSong,
  view,
} from '../audio/engine'
import { details, page } from '../audio/buildinfo'
import { Param } from '../audio/params'

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

// Loading the demo powers audio on, so start the scope.
async function onDemo() {
  await loadDemo()
  await onPower()
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
// Which build is running (#197): the page's version and the engine's, with the commits in the details.
const buildText = () => details(engineBuild)
const copied = ref(false)
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
// The section playing now, in an arrangement.
const section = computed(() => (song.entry >= 0 ? song.sections[song.arrange[song.entry] ?? -1] : undefined))
</script>

<template>
  <header class="pane bar">
    <strong class="logo">algo-synth</strong>
    <details class="build">
      <summary :title="buildText()">v{{ page.version }}<template v-if="status.running"> · engine v{{ engineBuild.version || '?' }}</template></summary>
      <div class="build-pop">
        <pre>{{ buildText() }}</pre>
        <button @click="copyBuild">{{ copied ? 'Copied' : 'Copy' }}</button>
      </div>
    </details>
    <button :class="{ on: status.running }" @click="onPower">
      {{ status.running ? 'Audio on' : 'Power on' }}
    </button>
    <span class="seg" role="group" aria-label="View">
      <button :aria-pressed="view.main === 'synths'" @click="view.main = 'synths'">Synths</button>
      <button :aria-pressed="view.main === 'mixer'" @click="view.main = 'mixer'">Mixer</button>
      <button :aria-pressed="view.main === 'composer'" @click="view.main = 'composer'">Composer</button>
      <button :aria-pressed="view.main === 'sound'" @click="view.main = 'sound'">Sound</button>
    </span>
    <button @click="onDemo">Demo</button>
    <label class="file" title="A MIDI file, its .synths.json setup, a .song, or several">
      <input type="file" multiple accept=".mid,.midi,audio/midi,.json,application/json,.song" @change="onFile" />Open…
    </label>
    <button :disabled="!status.running" title="Download the synths, their patches and routing as .synths.json" @click="saveSetup">Save setup</button>
    <button :disabled="!status.running || !song.text" title="Download the song as .song text" @click="saveSong">Save song</button>
    <!-- The transport, the only one (ADR-0022): the song plays, pauses where it is, stops back to the top. -->
    <button :disabled="!status.running" :class="{ on: song.playing }" @click="toggle">{{ song.playing ? '❚❚ Pause' : '▶ Play' }}</button>
    <button :disabled="!status.running" title="Stop and go back to the top" @click="stopSong">■ Stop</button>
    <span v-if="songPosition >= 0" class="field">
      <b>bar {{ Math.floor(songPosition / 16) + 1 }}</b> · step {{ (songPosition % 16) + 1 }}<template v-if="section"> · {{ section.name }}</template>
    </span>
    <button :disabled="!status.running" @click="getEngine()?.panic()">All notes off</button>
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
    <span class="field muted">
      {{ status.error || (status.running ? `${status.sampleRate} Hz · wasm worklet` : 'click Power on to start audio') }}
    </span>
  </header>
</template>

<style scoped>
.bar { display: flex; align-items: center; gap: 12px; padding: 8px 12px; overflow: hidden; }
.logo { color: var(--accent); letter-spacing: 0.04em; margin-right: 8px; }
.build { position: relative; color: var(--muted); font-size: 12px; white-space: nowrap; }
.build summary { cursor: pointer; }
.build-pop { position: absolute; z-index: 10; top: 24px; left: 0; padding: 8px 10px; background: var(--panel); border: 1px solid var(--line); border-radius: 4px; display: flex; flex-direction: column; gap: 6px; }
.build-pop pre { margin: 0; font-family: var(--font-mono); color: inherit; }
.on { border-color: var(--accent); color: var(--accent); }
.seg { display: inline-flex; }
.seg button { border-radius: 0; }
.seg button:first-child { border-radius: 4px 0 0 4px; }
.seg button:last-child { border-radius: 0 4px 4px 0; border-left: 0; }
.seg button[aria-pressed='true'] { border-color: var(--accent); color: var(--accent); background: var(--panel); }
.field { display: flex; align-items: center; gap: 6px; white-space: nowrap; }
.gain { width: 180px; }
.file { border: 1px solid var(--line); border-radius: 4px; padding: 4px 10px; background: var(--panel-2); cursor: pointer; white-space: nowrap; }
.file:hover { border-color: var(--accent); }
.file input { display: none; }
.muted { color: var(--muted); }
.meter { font-variant-numeric: tabular-nums; }
.meter.over b { color: var(--accent); }
.scope { background: var(--bg); border: 1px solid var(--line); border-radius: 4px; }
</style>
