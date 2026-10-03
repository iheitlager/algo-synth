<script setup lang="ts">
// The equalizer's response (#54), drawn from the band settings with the same
// filter formulas the engine uses (`console.ts`), 20 Hz to 20 kHz and ±15 dB.
import { onMounted, ref, watch } from 'vue'
import { clamp, eqDb, bandDb, type EqBand } from '../../audio/console'

const props = defineProps<{ bands: EqBand[] }>()
const canvas = ref<HTMLCanvasElement | null>(null)
const W = 560
const H = 150

function draw() {
  const cv = canvas.value
  const g = cv?.getContext('2d')
  if (!cv || !g) return
  const css = getComputedStyle(document.documentElement)
  const accent = css.getPropertyValue('--con-eq').trim() || '#5b9be0'
  const padL = 34
  const padB = 16
  const pw = W - padL - 8
  const ph = H - padB - 8
  const X = (f: number) => padL + (pw * Math.log(f / 20)) / Math.log(1000)
  const Y = (db: number) => 8 + ph * (1 - (db + 15) / 30)
  g.clearRect(0, 0, W, H)
  g.font = '500 17px "IBM Plex Mono", monospace'
  g.textBaseline = 'middle'
  g.fillStyle = '#6c7685'
  g.lineWidth = 1
  for (const db of [-12, -6, 0, 6, 12]) {
    g.beginPath(); g.moveTo(padL, Y(db)); g.lineTo(W - 8, Y(db)); g.strokeStyle = db === 0 ? '#3a424e' : '#1f242b'; g.stroke()
    g.textAlign = 'right'; g.fillText(db === 0 ? '0' : String(db), padL - 6, Y(db))
  }
  g.textAlign = 'center'
  for (const [f, t] of [[100, '100'], [1000, '1k'], [10_000, '10k']] as const) {
    g.beginPath(); g.moveTo(X(f), 8); g.lineTo(X(f), 8 + ph); g.strokeStyle = '#1f242b'; g.stroke()
    g.fillText(t, X(f), H - 7)
  }
  const pts: [number, number][] = []
  for (let i = 0; i <= 160; i++) {
    const f = 20 * 1000 ** (i / 160)
    pts.push([X(f), Y(clamp(eqDb(props.bands, f), -15, 15))])
  }
  const [first] = pts
  const last = pts[pts.length - 1]
  if (!first || !last) return
  g.beginPath()
  pts.forEach(([x, y], i) => (i ? g.lineTo(x, y) : g.moveTo(x, y)))
  g.lineTo(last[0], Y(0)); g.lineTo(first[0], Y(0)); g.closePath()
  g.fillStyle = 'rgba(91,155,224,.16)'; g.fill()
  g.beginPath()
  pts.forEach(([x, y], i) => (i ? g.lineTo(x, y) : g.moveTo(x, y)))
  g.strokeStyle = accent; g.lineWidth = 3; g.stroke()
  for (const b of props.bands) {
    g.beginPath(); g.arc(X(b.freq), Y(clamp(bandDb(b, b.freq), -15, 15)), 6, 0, 7)
    g.fillStyle = '#14171c'; g.fill(); g.lineWidth = 2.5; g.strokeStyle = accent; g.stroke()
  }
}
onMounted(() => { draw(); document.fonts?.ready.then(draw) })
watch(() => props.bands, draw, { deep: true })
</script>

<template>
  <canvas ref="canvas" :width="W" :height="H" role="img" aria-label="Equalizer response" />
</template>

<style scoped>
canvas { display: block; width: 100%; background: var(--con-inset); border-radius: 3px; }
</style>
