<script setup lang="ts">
// An envelope drawn as its curve (spec 003 Req 9), from the attack, decay,
// sustain and release the engine reports. It only draws: the four knobs under
// it change the values.
import { computed } from 'vue'
import { envPath } from '../../audio/faceplate'

const props = withDefaults(defineProps<{ a: number; d: number; s: number; r: number; width?: number; height?: number; label?: string }>(), {
  width: 176,
  height: 56,
})
const path = computed(() => envPath({ a: props.a, d: props.d, s: props.s, r: props.r }, props.width, props.height))
const area = computed(() => `${path.value}L${props.width - 3} ${props.height - 3}L3 ${props.height - 3}Z`)
</script>

<template>
  <svg
    class="env" :width="width" :height="height" :viewBox="`0 0 ${width} ${height}`" role="img"
    :aria-label="`${label ?? 'Envelope'}: attack ${a.toFixed(3)} s, decay ${d.toFixed(3)} s, sustain ${Math.round(s * 100)}%, release ${r.toFixed(3)} s`"
  >
    <rect x="0.5" y="0.5" :width="width - 1" :height="height - 1" rx="3" fill="var(--con-inset, #0d0f13)" stroke="#2b313b" />
    <line v-for="i in 3" :key="i" :x1="3" :x2="width - 3" :y1="(height / 4) * i" :y2="(height / 4) * i" stroke="#ffffff10" stroke-width="1" />
    <path :d="area" fill="var(--c, #f0b03a)" opacity="0.14" />
    <path :d="path" fill="none" stroke="var(--c, #f0b03a)" stroke-width="1.8" stroke-linejoin="round" stroke-linecap="round" />
  </svg>
</template>

<style scoped>
.env { display: block; }
</style>
