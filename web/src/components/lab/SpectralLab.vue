<script setup lang="ts">
// The Spectral Lab window (ADR-0017, spec 009 Req 7): load a WAV, see its
// partial tracks, play the original against its additive resynthesis, and
// send the resynthesis to the main window. The worker analyses and renders;
// this page sends, draws and plays.
import { computed, nextTick, onBeforeUnmount, reactive, ref, shallowRef, watch } from 'vue'
import { peakPath } from '../../audio/sampler'
import {
  Analyser, LAB_CHANNEL, LabEngine, codeMessage, connectMain, type Track,
} from '../../audio/spectral'
import Keyboard from '../synth/Keyboard.vue'

/** Synth and slot of each side: A the original, B the resynthesis. */
const SIDES = { a: { s: 0, slot: 0 }, b: { s: 1, slot: 1 } } as const
type Side = keyof typeof SIDES
const LOWEST_HZ = 30
const FLOOR_DB = -80

const engine = shallowRef<LabEngine | null>(null)
const analyser = shallowRef<Analyser | null>(null)
const state = reactive({
  name: '', busy: '', error: '', main: false, sent: '',
  frames: 0, side: 'b' as Side,
})
const settings = reactive({ window: 4096, hop: 256, top: 0, semitones: 0, stretch: 1 })
const original = shallowRef<ArrayBuffer | null>(null)
const resynth = shallowRef<ArrayBuffer | null>(null)
const tracks = shallowRef<Track[]>([])
const peaks = reactive<{ a: ArrayLike<number>; b: ArrayLike<number> }>({ a: [], b: [] })
const held = reactive(new Set<number>())
const canvas = ref<HTMLCanvasElement | null>(null)
const over = ref(false)

const rate = computed(() => engine.value?.ctx.sampleRate ?? 48_000)
const ratio = computed(() => 2 ** (settings.semitones / 12))

const main = connectMain(new BroadcastChannel(LAB_CHANNEL), (here) => (state.main = here))

async function power() {
  if (engine.value) return
  state.busy = 'starting the audio…'
  try {
    const e = await LabEngine.start()
    analyser.value = await Analyser.start(e.module)
    engine.value = e
    state.busy = ''
  } catch (err) {
    state.busy = ''
    state.error = `could not start the audio: ${err}`
  }
}

async function open(file: File) {
  await power()
  state.name = file.name
  original.value = await file.arrayBuffer()
  await analyse()
}

async function analyse() {
  const e = engine.value
  const a = analyser.value
  const bytes = original.value
  if (!e || !a || !bytes) return
  state.error = ''
  state.busy = 'analysing…'
  const loaded = await e.play(SIDES.a.s, SIDES.a.slot, bytes)
  if (loaded.code < 0) {
    state.busy = ''
    state.error = codeMessage(loaded.code)
    return
  }
  peaks.a = loaded.peaks
  const r = await a.analyse(bytes, rate.value, settings.window, settings.hop)
  state.busy = ''
  if (r.code < 0) {
    state.error = codeMessage(r.code)
    tracks.value = []
    return
  }
  state.frames = r.code
  tracks.value = r.tracks
  await render()
}

async function render() {
  const e = engine.value
  const a = analyser.value
  if (!e || !a || !tracks.value.length) return
  state.busy = 'resynthesising…'
  const wav = await a.render(settings.top, ratio.value, settings.stretch)
  resynth.value = wav
  const loaded = await e.play(SIDES.b.s, SIDES.b.slot, wav)
  state.busy = ''
  if (loaded.code < 0) state.error = codeMessage(loaded.code)
  else peaks.b = loaded.peaks
  state.sent = ''
}

let renderTimer: ReturnType<typeof setTimeout> | undefined
watch(() => [settings.top, settings.semitones, settings.stretch], () => {
  clearTimeout(renderTimer)
  renderTimer = setTimeout(() => void render(), 150)
})
watch(() => [settings.window, settings.hop], () => void analyse())

// A/B: a held key moves to the other side, so the switch is heard at once.
function down(n: number) {
  held.add(n)
  engine.value?.noteOn(SIDES[state.side].s, n)
}
function up(n: number) {
  if (!held.delete(n)) return
  engine.value?.noteOff(SIDES[state.side].s, n)
}
function choose(side: Side) {
  if (side === state.side) return
  for (const n of held) {
    engine.value?.noteOff(SIDES[state.side].s, n)
    engine.value?.noteOn(SIDES[side].s, n)
  }
  state.side = side
}

async function send() {
  if (!resynth.value) return
  const base = state.name.replace(/\.wav$/i, '') || 'sound'
  const code = await main.send(`${base} (resynth).wav`, resynth.value.slice(0))
  state.sent = code < 0 ? codeMessage(code) : 'sent to the main window'
}

function onDrop(e: DragEvent) {
  over.value = false
  const f = e.dataTransfer?.files[0]
  if (f) void open(f)
}
function onPick(e: Event) {
  const input = e.target as HTMLInputElement
  const f = input.files?.[0]
  input.value = ''
  if (f) void open(f)
}

// The tracks: time across, log frequency up, brighter when louder.
function draw() {
  const c = canvas.value
  const ctx = c?.getContext('2d')
  if (!c || !ctx) return
  const dpr = window.devicePixelRatio || 1
  const w = c.clientWidth
  const h = c.clientHeight
  c.width = Math.round(w * dpr)
  c.height = Math.round(h * dpr)
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, w, h)
  const top = Math.log(rate.value / 2)
  const bottom = Math.log(LOWEST_HZ)
  const y = (hz: number) => h - ((Math.log(Math.max(hz, LOWEST_HZ)) - bottom) / (top - bottom)) * h
  const x = (frame: number) => (frame / Math.max(1, state.frames - 1)) * w
  ctx.strokeStyle = 'rgba(255,255,255,0.08)'
  ctx.fillStyle = 'rgba(255,255,255,0.35)'
  ctx.font = '10px sans-serif'
  for (const hz of [100, 1_000, 10_000]) {
    ctx.beginPath()
    ctx.moveTo(0, y(hz))
    ctx.lineTo(w, y(hz))
    ctx.stroke()
    ctx.fillText(hz >= 1_000 ? `${hz / 1_000} kHz` : `${hz} Hz`, 4, y(hz) - 3)
  }
  ctx.lineWidth = 1.2
  for (const t of tracks.value) {
    for (let i = 1; i < t.freq.length; i++) {
      const db = 20 * Math.log10(Math.max(t.amp[i], 1e-9))
      const level = Math.min(1, Math.max(0, (db - FLOOR_DB) / -FLOOR_DB))
      if (level <= 0) continue
      ctx.strokeStyle = `rgba(240,162,59,${(0.1 + 0.9 * level).toFixed(2)})`
      ctx.beginPath()
      ctx.moveTo(x(t.start + i - 1), y(t.freq[i - 1]))
      ctx.lineTo(x(t.start + i), y(t.freq[i]))
      ctx.stroke()
    }
  }
}
watch(tracks, () => void nextTick(draw))
window.addEventListener('resize', draw)

const WAVE_W = 600
const WAVE_H = 48
const waves = computed(() => ({ a: peakPath(peaks.a, WAVE_W, WAVE_H), b: peakPath(peaks.b, WAVE_W, WAVE_H) }))

const goodbye = () => main.close()
window.addEventListener('pagehide', goodbye)
onBeforeUnmount(() => {
  window.removeEventListener('pagehide', goodbye)
  window.removeEventListener('resize', draw)
  engine.value?.panic()
  void engine.value?.ctx.close()
  goodbye()
})
</script>

<template>
  <div class="lab">
    <header>
      <h1>Spectral Lab</h1>
      <span class="status">
        <span v-if="state.name">{{ state.name }}<template v-if="tracks.length"> · {{ tracks.length }} tracks</template></span>
        <span v-if="state.busy" class="busy">{{ state.busy }}</span>
        <span v-if="state.error" class="error" role="alert">{{ state.error }}</span>
      </span>
      <button v-if="!engine" class="on" @click="power">Power on</button>
    </header>

    <label class="drop" :class="{ over }" @dragover.prevent="over = true" @dragleave="over = false" @drop.prevent="onDrop">
      <input type="file" accept=".wav,audio/wav" aria-label="Open a WAV file" @change="onPick">
      Drop a WAV here, or click to open one
    </label>

    <div class="row settings">
      <label>Window
        <select v-model.number="settings.window">
          <option v-for="n in [1024, 2048, 4096, 8192]" :key="n" :value="n">{{ n }}</option>
        </select>
      </label>
      <label>Hop
        <select v-model.number="settings.hop">
          <option v-for="n in [128, 256, 512]" :key="n" :value="n">{{ n }}</option>
        </select>
      </label>
      <label>Partials <b>{{ settings.top || 'all' }}</b>
        <input v-model.number="settings.top" type="range" min="0" max="256" step="1" aria-label="Loudest partials kept, 0 all">
      </label>
      <label>Shift <b>{{ settings.semitones > 0 ? '+' : '' }}{{ settings.semitones }} st</b>
        <input v-model.number="settings.semitones" type="range" min="-12" max="12" step="1" aria-label="Pitch shift in semitones">
      </label>
      <label>Stretch <b>×{{ settings.stretch }}</b>
        <input v-model.number="settings.stretch" type="range" min="0.5" max="4" step="0.25" aria-label="Time stretch">
      </label>
    </div>

    <canvas ref="canvas" class="tracks" aria-label="Partial tracks: time across, frequency up" />

    <div class="waves">
      <svg :viewBox="`0 0 ${WAVE_W} ${WAVE_H}`" preserveAspectRatio="none" aria-label="Original waveform"><path :d="waves.a" class="a" /></svg>
      <svg :viewBox="`0 0 ${WAVE_W} ${WAVE_H}`" preserveAspectRatio="none" aria-label="Resynthesis waveform"><path :d="waves.b" class="b" /></svg>
    </div>

    <div class="row">
      <span class="seg" role="group" aria-label="A/B">
        <button :aria-pressed="state.side === 'a'" @click="choose('a')">A original</button>
        <button :aria-pressed="state.side === 'b'" @click="choose('b')">B resynthesis</button>
      </span>
      <button :disabled="!resynth || !state.main" :title="state.main ? 'Load the resynthesis into the main window\'s samples' : 'Open the app to send it there'" @click="send">
        Send to main window
      </button>
      <span class="status">{{ state.main ? state.sent : 'no main window: open the app to send' }}</span>
    </div>

    <Keyboard :from="48" :to="84" :lit="held" @down="down" @up="up" />
  </div>
</template>

<style scoped>
.lab { display: flex; flex-direction: column; gap: 8px; padding: 10px; height: 100%; box-sizing: border-box; }
header { display: flex; align-items: baseline; gap: 12px; }
h1 { font-size: 15px; margin: 0; }
.status { display: flex; gap: 10px; color: var(--muted, #9aa); font-size: 12px; flex: 1; }
.busy { color: var(--accent); }
.error { color: #e66; }
.drop { border: 1px dashed var(--line); border-radius: 6px; padding: 10px; text-align: center; color: var(--muted, #9aa); cursor: pointer; font-size: 12px; }
.drop.over { border-color: var(--accent); color: var(--text); }
.drop input { display: none; }
.row { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; }
.settings label { display: flex; flex-direction: column; gap: 2px; font-size: 11px; min-width: 110px; }
.tracks { flex: 1; min-height: 200px; width: 100%; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; }
.waves { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
.waves svg { width: 100%; height: 48px; background: var(--panel); border: 1px solid var(--line); border-radius: 4px; }
.waves .a { fill: #8ab4d8; }
.waves .b { fill: var(--accent); }
.seg { display: inline-flex; }
.seg button { border-radius: 0; }
.seg button:first-child { border-radius: 4px 0 0 4px; }
.seg button:last-child { border-radius: 0 4px 4px 0; border-left: 0; }
.seg button[aria-pressed='true'] { border-color: var(--accent); color: var(--accent); background: var(--panel); }
</style>
