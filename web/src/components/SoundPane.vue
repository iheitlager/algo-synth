<script setup lang="ts">
// The Sound screen (ADR-0020, #216): design a Modular voice of the song. Its
// text (the voice line and its ctl lines) is edited here and sent to the
// engine, which puts it into the song; the knobs are the voice's controls on
// the synth its track plays; the scope and spectrum show the output; the
// keyboard plays that synth. The view draws and sends, the engine decides.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { editVoice, getEngine, song, status, view } from '../audio/engine'
import { exp, lin } from '../audio/console'
import { Param, type ParamId } from '../audio/params'
import SongEditor from './SongEditor.vue'
import ParamKnob from './console/ParamKnob.vue'
import Keyboard from './synth/Keyboard.vue'

const chosen = ref(0)
const voice = computed(() => song.voices[chosen.value])
const draft = ref('')
// A fresh text from the engine replaces the draft unless the draft is the one being fixed.
watch(
  () => voice.value?.text,
  (text) => {
    if (text !== undefined && !song.voiceError) draft.value = text
  },
  { immediate: true },
)
watch(chosen, () => {
  song.voiceError = null
  draft.value = voice.value?.text ?? ''
})

/** The synth the first track playing this voice is on, or −1. */
const synth = computed(() => song.tracks.find((t) => t.voice === chosen.value && t.synth >= 0)?.synth ?? -1)

const apply = () => editVoice(chosen.value, draft.value.endsWith('\n') ? draft.value : `${draft.value}\n`)
function onKey(e: KeyboardEvent) {
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault()
    apply()
  }
}

const fmt = (v: number) => (Math.abs(v) >= 100 ? v.toFixed(0) : Math.abs(v) >= 10 ? v.toFixed(1) : v.toFixed(2))
const knobs = computed(() =>
  (voice.value?.ctls ?? []).map((c, i) => ({
    ...c,
    id: (Param.Ctl1 + i) as ParamId,
    scale: c.exp ? exp(c.lo, c.hi) : lin(c.lo, c.hi),
  })),
)

const held = ref(new Set<number>())
function down(n: number) {
  if (synth.value < 0) return
  getEngine()?.noteOn(synth.value, n, 0.9)
  held.value = new Set(held.value).add(n)
}
function up(n: number) {
  if (!held.value.has(n)) return
  if (synth.value >= 0) getEngine()?.noteOff(synth.value, n)
  const next = new Set(held.value)
  next.delete(n)
  held.value = next
}

// The scope and the spectrum, from the master analyser (ADR-0003): drawing only.
const scope = ref<HTMLCanvasElement | null>(null)
const spectrum = ref<HTMLCanvasElement | null>(null)
let raf = 0
function draw() {
  const eng = getEngine()
  const sc = scope.value?.getContext('2d')
  const sp = spectrum.value?.getContext('2d')
  if (eng && sc && sp && scope.value && spectrum.value) {
    const a = eng.analyser
    const wave = new Float32Array(a.fftSize)
    a.getFloatTimeDomainData(wave)
    const { width: w, height: h } = scope.value
    sc.clearRect(0, 0, w, h)
    sc.strokeStyle = '#f0b03a'
    sc.lineWidth = 1.5
    sc.beginPath()
    wave.forEach((v, i) => {
      const x = (i / wave.length) * w
      const y = (0.5 - v * 0.45) * h
      if (i === 0) sc.moveTo(x, y)
      else sc.lineTo(x, y)
    })
    sc.stroke()
    const bins = new Float32Array(a.frequencyBinCount)
    a.getFloatFrequencyData(bins)
    const { width: sw, height: shh } = spectrum.value
    sp.clearRect(0, 0, sw, shh)
    sp.fillStyle = '#4fb3a9'
    // Log frequency from 20 Hz to 20 kHz, -100 to 0 dB.
    const nyquist = status.sampleRate / 2 || 24_000
    for (let x = 0; x < sw; x++) {
      const f = 20 * 1000 ** (x / sw)
      const db = bins[Math.min(bins.length - 1, Math.round((f / nyquist) * bins.length))] ?? -100
      const bar = Math.max(0, Math.min(1, (db + 100) / 100)) * shh
      sp.fillRect(x, shh - bar, 1, bar)
    }
  }
  raf = requestAnimationFrame(draw)
}
onMounted(() => (raf = requestAnimationFrame(draw)))
onBeforeUnmount(() => {
  cancelAnimationFrame(raf)
  for (const n of held.value) up(n)
})
</script>

<template>
  <section class="sound" aria-label="Sound">
    <header class="bar">
      <h2>Sound</h2>
      <span v-if="song.voices.length" class="seg" role="group" aria-label="Voice">
        <button v-for="(v, i) in song.voices" :key="v.name" :aria-pressed="chosen === i" @click="chosen = i">{{ v.name }}</button>
      </span>
      <button v-if="voice" :disabled="!status.running" title="Put the voice into the song (Ctrl+Enter)" @click="apply">Apply</button>
      <button class="link" @click="view.main = 'composer'">Song text…</button>
    </header>
    <p v-if="!song.voices.length" class="empty">
      The song has no voice yet. Write one in the song, as
      <code>voice lead = { saw(freq) |&gt; svf(lp, cutoff) }</code> with <code>ctl cutoff = 800 [100 8000 exp]</code>
      under it, and play it with <code>track lead synth Modular lead</code>.
    </p>
    <template v-else>
      <div class="edit" @keydown="onKey">
        <SongEditor v-model="draft" :error="song.voiceError" :disabled="!status.running" />
        <p v-if="song.voiceError" class="err" role="alert">
          Line {{ song.voiceError.line }}, column {{ song.voiceError.col }}: {{ song.voiceError.msg }}
        </p>
      </div>
      <div class="side">
        <div class="knobs">
          <template v-if="synth >= 0">
            <ParamKnob
              v-for="k in knobs" :key="k.name" :synth="synth" :id="k.id" :label="k.name" :scale="k.scale" :def="k.def" :text="fmt"
            />
            <p v-if="!knobs.length" class="hint">Controls appear here: <code>ctl name = value [low high]</code> under the voice.</p>
          </template>
          <p v-else class="hint">No track plays this voice: add <code>track name synth Modular {{ voice?.name }}</code>.</p>
        </div>
        <canvas ref="scope" class="view" width="420" height="90" aria-label="Scope" />
        <canvas ref="spectrum" class="view" width="420" height="90" aria-label="Spectrum" />
      </div>
      <Keyboard class="keys" :from="36" :to="84" :lit="held" @down="down" @up="up" />
    </template>
  </section>
</template>

<style scoped>
.sound {
  display: grid; gap: 8px; min-height: 0;
  grid-template-columns: minmax(0, 1fr) 440px;
  grid-template-rows: auto minmax(0, 1fr) auto;
  grid-template-areas: 'bar bar' 'edit side' 'keys keys';
}
.bar { grid-area: bar; display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.bar h2 { margin: 0; font: 600 15px var(--con-font-silk); letter-spacing: 0.08em; text-transform: uppercase; }
.seg { display: inline-flex; gap: 2px; }
.seg button[aria-pressed='true'] { border-color: var(--accent); color: var(--accent); }
.link { margin-left: auto; }
.empty { grid-column: 1 / -1; color: var(--muted); line-height: 1.6; }
.edit { grid-area: edit; display: flex; flex-direction: column; gap: 4px; min-height: 0; }
.err { margin: 0; color: var(--accent); font-size: 12px; }
.side { grid-area: side; display: flex; flex-direction: column; gap: 8px; min-height: 0; }
.knobs {
  display: flex; flex-wrap: wrap; gap: 12px; align-content: flex-start; padding: 10px;
  min-height: 90px; background: var(--con-panel); border: 1px solid var(--con-line); border-radius: 4px;
}
.hint { margin: 0; color: var(--con-silk-dim); font-size: 12px; }
.view { width: 100%; height: 90px; background: #0b0d10; border: 1px solid var(--con-line); border-radius: 4px; }
.keys { grid-area: keys; }
code { font-family: var(--font-mono); font-size: 12px; }
</style>
