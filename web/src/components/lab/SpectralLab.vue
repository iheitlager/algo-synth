<script setup lang="ts">
// The Spectral Lab window (ADR-0017, spec 009 Req 7): load a WAV, see its
// partial tracks, play the original against its additive resynthesis, and
// send the resynthesis to the main window. The worker analyses and renders;
// this page sends, draws and plays.
import { computed, nextTick, onBeforeUnmount, reactive, ref, shallowRef, watch } from 'vue'
import { spectrogramPixels } from '../../audio/colormap'
import { peakPath } from '../../audio/sampler'
import {
  Analyser, Edit, LAB_CHANNEL, LabEngine, NO_EDITS, codeMessage, connectMain, type Spectrogram, type Track,
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
  /** The main window's user slot (0-7) a table or an attack goes to. */
  slot: 0,
})
const settings = reactive({ window: 4096, hop: 256, top: 0, semitones: 0, stretch: 1, noise: true })
// The transforms (spec 010 Req 4), by `Edit` id; `NO_EDITS` leaves the sound alone.
const edits = reactive<number[]>([...NO_EDITS])
// Low- and high-pass cutoffs as slider positions: 0 off, else 50 Hz to 20 kHz.
const cut = reactive({ low: 0, high: 0 })
const cutHz = (x: number) => (x <= 0 ? 0 : 50 * 400 ** x)
watch(cut, () => {
  edits[Edit.LowPass] = cutHz(cut.low)
  edits[Edit.HighPass] = cutHz(cut.high)
})
const freezeOn = ref(false)
const freezeAt = ref(0.1)
watch([freezeOn, freezeAt], () => (edits[Edit.Freeze] = freezeOn.value ? freezeAt.value : -1))
function resetEdits() {
  edits.splice(0, edits.length, ...NO_EDITS)
  cut.low = 0
  cut.high = 0
  freezeOn.value = false
}
const fmtHz = (hz: number) => (hz <= 0 ? 'off' : hz >= 1000 ? `${(hz / 1000).toFixed(1)} kHz` : `${Math.round(hz)} Hz`)
const original = shallowRef<ArrayBuffer | null>(null)
const resynth = shallowRef<ArrayBuffer | null>(null)
const tracks = shallowRef<Track[]>([])
const grams = shallowRef<{ a: Spectrogram | null; b: Spectrogram | null }>({ a: null, b: null })
const view = reactive({ gram: true, partials: true })
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
  const r = await a.analyse(bytes, rate.value, settings.window, settings.hop, settings.noise)
  state.busy = ''
  if (r.code < 0) {
    state.error = codeMessage(r.code)
    tracks.value = []
    return
  }
  state.frames = r.code
  grams.value = { a: r.gram, b: null }
  tracks.value = r.tracks
  await render()
}

async function render() {
  const e = engine.value
  const a = analyser.value
  if (!e || !a || !tracks.value.length) return
  state.busy = 'resynthesising…'
  const { wav, gram } = await a.render(settings.top, ratio.value, settings.stretch, edits)
  resynth.value = wav
  grams.value = { ...grams.value, b: gram }
  const loaded = await e.play(SIDES.b.s, SIDES.b.slot, wav)
  state.busy = ''
  if (loaded.code < 0) state.error = codeMessage(loaded.code)
  else peaks.b = loaded.peaks
  state.sent = ''
}

let renderTimer: ReturnType<typeof setTimeout> | undefined
watch(() => [settings.top, settings.semitones, settings.stretch, ...edits], () => {
  clearTimeout(renderTimer)
  renderTimer = setTimeout(() => void render(), 150)
})
watch(() => [settings.window, settings.hop, settings.noise], () => void analyse())

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
  state.sent = code < 0 ? codeMessage(code) : 'in the app\'s samples: put it on a Sampler\'s zone'
}

// The analysed sound as a wavetable for the PPG, or its attack for the D-50, into
// the main window's user slot (spec 010 Req 9-10).
async function sendUser(kind: 'table' | 'attack') {
  const a = analyser.value
  if (!a || !tracks.value.length) return
  const { values, root } = await a.extract(kind, edits)
  if (!values.length) {
    state.sent = codeMessage(-101)
    return
  }
  const slot = state.slot
  const code = kind === 'table' ? await main.sendTable(slot, values.buffer as ArrayBuffer) : await main.sendAttack(slot, root, values.buffer as ArrayBuffer)
  state.sent = code < 0
    ? codeMessage(code)
    : kind === 'table'
      ? `table in User ${slot + 1}: pick it as a PPG's Table`
      : `attack in User ${slot + 1}: pick it as a D-50 partial's PCM`
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

// The spectrogram of the side being played (the engine's levels in the turbo
// colours), and over it the tracks: time across, log frequency up.
let gramCache: { gram: Spectrogram; image: HTMLCanvasElement } | null = null
function gramImage(gram: Spectrogram): HTMLCanvasElement | null {
  if (gramCache?.gram === gram) return gramCache.image
  const px = spectrogramPixels(gram.levels, gram.bands)
  if (!px.width) return null
  const image = document.createElement('canvas')
  image.width = px.width
  image.height = px.height
  image.getContext('2d')?.putImageData(new ImageData(px.data, px.width, px.height), 0, 0)
  gramCache = { gram, image }
  return image
}

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
  // A resynthesis not rendered yet shows the original's.
  const gram = state.side === 'b' ? grams.value.b ?? grams.value.a : grams.value.a
  const image = view.gram && gram ? gramImage(gram) : null
  if (image) {
    ctx.imageSmoothingEnabled = true
    // Its frames run as the tracks' do; a stretched resynthesis is longer.
    const span = Math.max(1, image.width - 1) / Math.max(1, state.frames - 1)
    ctx.drawImage(image, 0, 0, image.width, image.height, 0, 0, w * span, h)
  }
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
  if (!view.partials) return
  // Over a spectrogram the tracks are white, so they read against any colour.
  const ink = image ? '255,255,255' : '240,162,59'
  for (const t of tracks.value) {
    for (let i = 1; i < t.freq.length; i++) {
      const db = 20 * Math.log10(Math.max(t.amp[i], 1e-9))
      const level = Math.min(1, Math.max(0, (db - FLOOR_DB) / -FLOOR_DB))
      if (level <= 0) continue
      ctx.strokeStyle = `rgba(${ink},${(0.1 + 0.9 * level).toFixed(2)})`
      ctx.beginPath()
      ctx.moveTo(x(t.start + i - 1), y(t.freq[i - 1]))
      ctx.lineTo(x(t.start + i), y(t.freq[i]))
      ctx.stroke()
    }
  }
}
watch([tracks, grams, () => state.side, () => view.gram, () => view.partials], () => void nextTick(draw))
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
      <span class="layers" role="group" aria-label="Layers">
        <label><input v-model="view.gram" type="checkbox"> Spectrogram</label>
        <label><input v-model="view.partials" type="checkbox"> Partials</label>
      </span>
    </div>

    <details class="transforms" open>
      <summary>Transforms <button class="reset" title="Back to the sound as analysed" @click.prevent="resetEdits">Reset</button></summary>
      <div class="row settings">
        <label title="Each partial's noise, kept from the analysis: breath, bow and hammer">
          <span><input v-model="settings.noise" type="checkbox"> Noise</span>
          <input v-model.number="edits[Edit.Noise]" type="range" min="0" max="3" step="0.05" aria-label="Noise amount" :disabled="!settings.noise">
        </label>
        <label>Harmonic stretch <b>×{{ edits[Edit.Stretch].toFixed(2) }}</b>
          <input v-model.number="edits[Edit.Stretch]" type="range" min="0.5" max="2" step="0.01" aria-label="Harmonic stretch">
        </label>
        <label title="The higher a partial, the further it moves: a string becomes a bell">Inharmonic <b>×{{ edits[Edit.Inharmonic].toFixed(2) }}</b>
          <input v-model.number="edits[Edit.Inharmonic]" type="range" min="0.5" max="3" step="0.01" aria-label="Inharmonic stretch">
        </label>
        <label>Freq shift <b>{{ Math.round(edits[Edit.FreqShift]) }} Hz</b>
          <input v-model.number="edits[Edit.FreqShift]" type="range" min="-500" max="500" step="1" aria-label="Frequency shift">
        </label>
        <label>Formant <b>×{{ edits[Edit.Formant].toFixed(2) }}</b>
          <input v-model.number="edits[Edit.Formant]" type="range" min="0.5" max="2" step="0.01" aria-label="Formant scale">
          <span><input type="checkbox" :checked="edits[Edit.KeepFormants] >= 0.5" @change="edits[Edit.KeepFormants] = ($event.target as HTMLInputElement).checked ? 1 : 0"> keep on shift</span>
        </label>
        <label>Smear <b>{{ Math.round(edits[Edit.Smear] * 100) }}%</b>
          <input v-model.number="edits[Edit.Smear]" type="range" min="0" max="0.95" step="0.01" aria-label="Smear">
        </label>
        <label title="Left: odd harmonics only (hollow); right: even only">Odd ↔ even
          <input v-model.number="edits[Edit.OddEven]" type="range" min="0" max="1" step="0.01" aria-label="Odd and even balance">
        </label>
        <label>Low-pass <b>{{ fmtHz(edits[Edit.LowPass]) }}</b>
          <input v-model.number="cut.low" type="range" min="0" max="1" step="0.005" aria-label="Spectral low-pass">
          <input v-model.number="edits[Edit.LowRes]" type="range" min="0" max="1" step="0.01" aria-label="Spectral low-pass resonance" title="Resonance">
        </label>
        <label>High-pass <b>{{ fmtHz(edits[Edit.HighPass]) }}</b>
          <input v-model.number="cut.high" type="range" min="0" max="1" step="0.005" aria-label="Spectral high-pass">
        </label>
        <label title="Harmor's Pluck: the higher a partial, the faster it dies (negative: the other way)">Decay by number <b>{{ edits[Edit.Decay].toFixed(1) }}</b>
          <input v-model.number="edits[Edit.Decay]" type="range" min="-1" max="4" step="0.1" aria-label="Decay by number">
        </label>
        <label>
          <span><input v-model="freezeOn" type="checkbox"> Freeze</span>
          <input v-model.number="freezeAt" type="range" min="0" max="1" step="0.005" aria-label="Freeze position" :disabled="!freezeOn">
        </label>
      </div>
    </details>

    <canvas ref="canvas" class="tracks" aria-label="Spectrogram and partial tracks of the side playing: time across, frequency up" />

    <div class="waves">
      <svg :viewBox="`0 0 ${WAVE_W} ${WAVE_H}`" preserveAspectRatio="none" aria-label="Original waveform"><path :d="waves.a" class="a" /></svg>
      <svg :viewBox="`0 0 ${WAVE_W} ${WAVE_H}`" preserveAspectRatio="none" aria-label="Resynthesis waveform"><path :d="waves.b" class="b" /></svg>
    </div>

    <div class="row">
      <span class="seg" role="group" aria-label="A/B">
        <button :aria-pressed="state.side === 'a'" @click="choose('a')">A original</button>
        <button :aria-pressed="state.side === 'b'" @click="choose('b')">B resynthesis</button>
      </span>
      <button :disabled="!resynth || !state.main" :title="state.main ? 'Load what B plays into a free sample slot of the app, for a Sampler' : 'Open the app to send it there'" @click="send">
        Send as sample
      </button>
      <label class="slot">to
        <select v-model.number="state.slot" aria-label="User slot">
          <option v-for="n in 8" :key="n" :value="n - 1">User {{ n }}</option>
        </select>
      </label>
      <button :disabled="!tracks.length || !state.main" title="The sound's harmonics as a 64-wave table for the PPG Wave" @click="sendUser('table')">
        Send as table
      </button>
      <button :disabled="!tracks.length || !state.main" title="The sound's attack as a PCM sample for a D-50 partial" @click="sendUser('attack')">
        Send attack
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
.transforms summary { cursor: pointer; font-size: 12px; display: flex; align-items: center; gap: 10px; }
.transforms .reset { font-size: 11px; padding: 1px 8px; }
.transforms label span { display: flex; align-items: center; gap: 4px; }
.slot { display: flex; align-items: center; gap: 4px; font-size: 12px; }
.layers { display: flex; flex-direction: column; gap: 2px; font-size: 11px; }
.layers label { display: flex; align-items: center; gap: 4px; min-width: 0; flex-direction: row; }
.seg { display: inline-flex; }
.seg button { border-radius: 0; }
.seg button:first-child { border-radius: 4px 0 0 4px; }
.seg button:last-child { border-radius: 0 4px 4px 0; border-left: 0; }
.seg button[aria-pressed='true'] { border-color: var(--accent); color: var(--accent); background: var(--panel); }
</style>
