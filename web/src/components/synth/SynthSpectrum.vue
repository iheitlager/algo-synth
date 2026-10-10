<script setup lang="ts">
// A live spectrum of this synth's own output (#519), on its faceplate: off
// until asked for, then the engine computes the bands of the synth's bus
// before its strip and this draws them, coloured by level, with a peak that
// falls back slowly. One synth is watched at a time.
import { onBeforeUnmount, ref, watch } from 'vue'
import { turbo } from '../../audio/colormap'
import { SPECTRUM, spectrum, watchSpectrum } from '../../audio/engine'

const props = defineProps<{ s: number; color?: string }>()

// The level a bar reaches the top at, and the bottom of the scale.
const TOP_DB = 0
const BOTTOM_DB = -90
// How fast a held peak falls, in dB per update (about 20 ms).
const PEAK_FALL = 0.6
const KEY = 'algo-synth.spectrum'

const remembered = () => {
  try {
    return localStorage.getItem(KEY) === 'on'
  } catch {
    return false
  }
}
const on = ref(remembered())
const canvas = ref<HTMLCanvasElement | null>(null)
let peaks = new Float32Array(SPECTRUM.bands).fill(BOTTOM_DB)

function toggle() {
  on.value = !on.value
  try {
    localStorage.setItem(KEY, on.value ? 'on' : 'off')
  } catch {
    // A page without storage just forgets.
  }
}

watch([on, () => props.s], ([shown, s]) => {
  peaks.fill(BOTTOM_DB)
  if (shown) watchSpectrum(s)
  else if (spectrum.s === s) watchSpectrum(-1)
}, { immediate: true })

const level = (db: number) => Math.min(1, Math.max(0, (db - BOTTOM_DB) / (TOP_DB - BOTTOM_DB)))

function draw(bands: Float32Array | null) {
  const c = canvas.value
  const ctx = c?.getContext('2d')
  if (!c || !ctx) return
  const dpr = window.devicePixelRatio || 1
  const w = c.clientWidth
  const h = c.clientHeight
  if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) {
    c.width = Math.round(w * dpr)
    c.height = Math.round(h * dpr)
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, w, h)
  const n = SPECTRUM.bands
  const bw = w / n
  // Frequency lines at 100 Hz, 1 kHz and 10 kHz.
  const ratio = Math.log(SPECTRUM.highHz / SPECTRUM.lowHz)
  ctx.fillStyle = 'rgba(255,255,255,0.35)'
  ctx.strokeStyle = 'rgba(255,255,255,0.08)'
  ctx.font = '9px sans-serif'
  for (const hz of [100, 1_000, 10_000]) {
    const x = (Math.log(hz / SPECTRUM.lowHz) / ratio) * w
    ctx.beginPath()
    ctx.moveTo(x, 0)
    ctx.lineTo(x, h)
    ctx.stroke()
    ctx.fillText(hz >= 1_000 ? `${hz / 1_000}k` : `${hz}`, x + 2, 10)
  }
  if (!bands) return
  for (let i = 0; i < n; i++) {
    const db = bands[i] ?? BOTTOM_DB
    peaks[i] = Math.max(db, peaks[i] - PEAK_FALL)
    const v = level(db)
    if (v > 0) {
      const [r, g, b] = turbo(v)
      ctx.fillStyle = `rgb(${r},${g},${b})`
      ctx.fillRect(i * bw, h - v * h, Math.max(1, bw - 0.5), v * h)
    }
    const p = level(peaks[i])
    if (p > 0) {
      ctx.fillStyle = 'rgba(255,255,255,0.7)'
      ctx.fillRect(i * bw, h - p * h, Math.max(1, bw - 0.5), 1.5)
    }
  }
}

watch(() => spectrum.bands, (bands) => {
  if (on.value && spectrum.s === props.s) draw(bands)
})
watch(on, (shown) => {
  if (shown) requestAnimationFrame(() => draw(null))
})

onBeforeUnmount(() => {
  if (spectrum.s === props.s) watchSpectrum(-1)
})
</script>

<template>
  <div class="spectrum">
    <button
      class="toggle" :aria-pressed="on" :style="{ '--c': color }"
      title="Show the spectrum of this synth's own output, before its strip" @click="toggle"
    >
      Spectrum {{ on ? 'on' : 'off' }}
    </button>
    <canvas v-if="on" ref="canvas" aria-label="Spectrum of this synth: frequency across, level up" />
  </div>
</template>

<style scoped>
.spectrum { display: flex; flex-direction: column; gap: 6px; width: 320px; }
.toggle { align-self: flex-start; font-size: 11px; }
.toggle[aria-pressed='true'] { border-color: var(--c, var(--accent)); color: var(--c, var(--accent)); }
canvas { width: 100%; height: 200px; background: #07080a; border-radius: 4px; }
</style>
