<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { getEngine, loadDemo, loadMidi, meter, play, player, power, status, stop } from '../audio/engine'
import { Param } from '../audio/params'

// The performance counter: worklet time per block against the budget
// (plan.md "Performance budget"; tools/bench.mjs uses the same figure).
const BUDGET = 0.25
const pct = (x: number) => `${(100 * x).toFixed(1)}%`

const gain = ref(0.5)
watch(gain, (v) => getEngine()?.param(Param.MasterGain, v))

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
  getEngine()?.param(Param.MasterGain, gain.value)
  if (!raf) draw()
}
onBeforeUnmount(() => cancelAnimationFrame(raf))

// MIDI player (spec 002 Req 9): loading powers audio on, so start the scope.
async function onDemo() {
  await loadDemo()
  await onPower()
}
async function onFile(e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  await loadMidi(await file.arrayBuffer(), file.name)
  await onPower()
}
const clock = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`
</script>

<template>
  <header class="pane bar">
    <strong class="logo">algo-synth</strong>
    <button :class="{ on: status.running }" @click="onPower">
      {{ status.running ? 'Audio on' : 'Power on' }}
    </button>
    <button @click="onDemo">Demo</button>
    <label class="file"><input type="file" accept=".mid,.midi,audio/midi" @change="onFile" />Open MIDI…</label>
    <button :disabled="!player.loaded" :class="{ on: player.playing }" @click="play">▶ Play</button>
    <button :disabled="!player.loaded" @click="stop">■ Stop</button>
    <span v-if="player.loaded" class="field">
      <b>{{ clock(player.position) }}</b> / {{ clock(player.length) }} · bar {{ Math.floor(player.position / player.bar) + 1 }}
    </span>
    <button :disabled="!status.running" @click="getEngine()?.panic()">All notes off</button>
    <label class="field gain">Master <input v-model.number="gain" type="range" min="0" max="1" step="0.01" /></label>
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
.on { border-color: var(--accent); color: var(--accent); }
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
