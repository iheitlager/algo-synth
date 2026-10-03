<script setup lang="ts">
// The on-screen keyboard under the faceplate (spec 003 Req 3): white keys with
// the black ones over them. A key held from the computer keyboard lights up
// too (`lit`). It emits what is pressed; the engine does the rest.
import { computed } from 'vue'

const props = withDefaults(defineProps<{ from?: number; to?: number; lit?: ReadonlySet<number> }>(), { from: 36, to: 84 })
defineEmits<{ down: [n: number]; up: [n: number] }>()

const isBlack = (n: number) => [1, 3, 6, 8, 10].includes(n % 12)
const whites = computed(() => Array.from({ length: props.to - props.from + 1 }, (_, i) => props.from + i).filter((n) => !isBlack(n)))
const blacks = computed(() => {
  const w = 100 / whites.value.length
  return Array.from({ length: props.to - props.from + 1 }, (_, i) => props.from + i)
    .filter(isBlack)
    // A black key sits across the edge between the white key below it and the next.
    .map((n) => ({ n, left: (whites.value.indexOf(n - 1) + 1) * w, w }))
})
const name = (n: number) => `${['C', 'C♯', 'D', 'D♯', 'E', 'F', 'F♯', 'G', 'G♯', 'A', 'A♯', 'B'][n % 12]}${Math.floor(n / 12) - 1}`
</script>

<template>
  <div class="kbd" role="group" aria-label="Keyboard">
    <button
      v-for="n in whites" :key="n" class="w" :class="{ lit: lit?.has(n), c: n % 12 === 0 }" :aria-label="name(n)" tabindex="-1"
      :style="{ width: `${100 / whites.length}%` }"
      @pointerdown.prevent="$emit('down', n)" @pointerup="$emit('up', n)" @pointerleave="$emit('up', n)" @pointercancel="$emit('up', n)"
    ><span v-if="n % 12 === 0">{{ name(n) }}</span></button>
    <button
      v-for="b in blacks" :key="b.n" class="b" :class="{ lit: lit?.has(b.n) }" :aria-label="name(b.n)" tabindex="-1"
      :style="{ left: `${b.left}%`, width: `${b.w * 0.62}%`, marginLeft: `${-b.w * 0.31}%` }"
      @pointerdown.prevent="$emit('down', b.n)" @pointerup="$emit('up', b.n)" @pointerleave="$emit('up', b.n)" @pointercancel="$emit('up', b.n)"
    />
  </div>
</template>

<style scoped>
.kbd { position: relative; display: flex; height: 92px; background: #07080a; padding: 0 0 2px; border-radius: 0 0 4px 4px; touch-action: none; user-select: none; -webkit-user-select: none; }
.w { height: 100%; padding: 0; border: 0; border-right: 1px solid #07080a; border-radius: 0 0 3px 3px; background: linear-gradient(#e9e4d6, #d3cdbd); cursor: pointer; display: flex; align-items: flex-end; justify-content: center; }
.w span { font: 500 10px var(--con-font-silk); letter-spacing: 0.08em; color: #6d6a60; padding-bottom: 3px; pointer-events: none; }
.w.lit, .w:active { background: linear-gradient(var(--c, #f0b03a), color-mix(in srgb, var(--c, #f0b03a) 70%, white)); }
.b { position: absolute; top: 0; height: 60%; padding: 0; border: 1px solid #000; border-top: 0; border-radius: 0 0 3px 3px; background: linear-gradient(#2c3038, #101216); cursor: pointer; z-index: 1; }
.b.lit, .b:active { background: linear-gradient(var(--c, #f0b03a), color-mix(in srgb, var(--c, #f0b03a) 45%, black)); }
</style>
