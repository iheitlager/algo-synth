<script setup lang="ts">
// One lane's notes (#137): a single path built when the notes, colour or song
// length change, never as the playhead moves, so playing costs nothing here.
import { computed, toRaw } from 'vue'

const props = defineProps<{ roll: [number, number, number][]; length: number; colour: string }>()

const drawing = computed(() => {
  const roll = toRaw(props.roll)
  let lo = 127
  let hi = 0
  let d = ''
  for (const [start, end, n] of roll) {
    lo = Math.min(lo, n)
    hi = Math.max(hi, n)
    d += `M${start} ${-n - 1}h${Math.max(end - start, 0.02)}v1H${start}z`
  }
  // Pitch range, padded so a single note still has height.
  const [bottom, top] = lo > hi ? [60, 61] : [lo - 1, hi + 2]
  return { d, viewBox: `0 ${-top} ${props.length || 1} ${top - bottom}` }
})
</script>

<template>
  <svg :viewBox="drawing.viewBox" preserveAspectRatio="none" aria-hidden="true">
    <path :d="drawing.d" :fill="colour" />
  </svg>
</template>
