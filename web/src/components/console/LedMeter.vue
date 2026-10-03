<script setup lang="ts">
// A vertical LED meter on a canvas (#52): `level` is a peak 0..1; the segments
// run green, amber, red, and a hold dot stays at the highest recent peak.
import { onMounted, ref, watch } from 'vue'
import { ledSegments } from '../../audio/console'

const props = withDefaults(defineProps<{ level: number; height: number; width?: number; segs?: number }>(), { width: 9, segs: 28 })

const canvas = ref<HTMLCanvasElement | null>(null)
// The engine reports the highest level since its last report; the bar falls
// from it smoothly and a hold dot stays at the highest recent peak.
let shown = 0
let peak = 0
const colors = { g: '#3fcf6e', a: '#f0b03a', r: '#f0503a' }

function draw() {
  const cv = canvas.value
  const g = cv?.getContext('2d')
  if (!cv || !g) return
  const { width: W, height: H } = cv
  const gap = 2
  const segH = (H - gap * (props.segs - 1)) / props.segs
    shown = Math.max(props.level, shown * 0.86)
  peak = Math.max(props.level, peak * 0.985)
  const lit = ledSegments(shown, props.segs)
  const hold = Math.round(ledSegments(peak, props.segs)) - 1
  g.clearRect(0, 0, W, H)
  for (let i = 0; i < props.segs; i++) {
    const frac = i / props.segs
    g.fillStyle = frac > 0.93 ? colors.r : frac > 0.78 ? colors.a : colors.g
    g.globalAlpha = i < lit || i === hold ? 1 : 0.13
    g.fillRect(0, H - (i + 1) * (segH + gap) + gap, W, segH)
  }
}
onMounted(() => {
  const root = getComputedStyle(document.documentElement)
  colors.g = root.getPropertyValue('--con-led-g').trim() || colors.g
  colors.a = root.getPropertyValue('--con-led-a').trim() || colors.a
  colors.r = root.getPropertyValue('--con-led-r').trim() || colors.r
  draw()
})
watch(() => props.level, draw, { flush: 'post' })
</script>

<template>
  <canvas ref="canvas" :width="width * 2" :height="height * 2" :style="{ width: `${width}px`, height: `${height}px` }" aria-hidden="true" />
</template>

<style scoped>
canvas { display: block; }
</style>
