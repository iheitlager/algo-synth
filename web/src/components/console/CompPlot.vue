<script setup lang="ts">
// The compressor's transfer curve (#54): input against output level in dB,
// with the 1:1 line dashed.
import { onMounted, ref, watch } from 'vue'
import { clamp, compOutDb } from '../../audio/console'

const props = defineProps<{ threshold: number; ratio: number; makeup: number }>()
const canvas = ref<HTMLCanvasElement | null>(null)
const S = 240

function draw() {
  const cv = canvas.value
  const g = cv?.getContext('2d')
  if (!cv || !g) return
  const accent = getComputedStyle(document.documentElement).getPropertyValue('--con-comp').trim() || '#e8c34a'
  const X = (db: number) => 8 + ((S - 16) * (db + 60)) / 60
  const Y = (db: number) => S - 8 - ((S - 16) * (db + 60)) / 60
  g.fillStyle = '#0d0f13'; g.fillRect(0, 0, S, S)
  g.strokeStyle = '#1f242b'; g.lineWidth = 1
  for (let d = -60; d <= 0; d += 20) {
    g.beginPath(); g.moveTo(X(d), 8); g.lineTo(X(d), S - 8); g.moveTo(8, Y(d)); g.lineTo(S - 8, Y(d)); g.stroke()
  }
  g.setLineDash([6, 6]); g.strokeStyle = '#3a424e'
  g.beginPath(); g.moveTo(X(-60), Y(-60)); g.lineTo(X(0), Y(0)); g.stroke(); g.setLineDash([])
  g.beginPath()
  for (let d = -60; d <= 0; d++) {
    const o = clamp(compOutDb(d, props.threshold, props.ratio, props.makeup), -60, 6)
    if (d === -60) g.moveTo(X(d), Y(o))
    else g.lineTo(X(d), Y(o))
  }
  g.strokeStyle = accent; g.lineWidth = 3.5; g.stroke()
}
onMounted(draw)
watch(() => [props.threshold, props.ratio, props.makeup], draw)
</script>

<template>
  <canvas ref="canvas" :width="S" :height="S" role="img" aria-label="Compressor transfer curve" />
</template>

<style scoped>
canvas { display: block; width: 100%; max-width: 150px; aspect-ratio: 1; background: var(--con-inset); border-radius: 3px; }
</style>
