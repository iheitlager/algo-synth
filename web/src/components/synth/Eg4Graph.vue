<script setup lang="ts">
// A DX7 envelope drawn from its four rates and four levels (spec 003 Req 9):
// it rises from level 4 to level 1, moves on to 2 and 3, holds while the key is
// down, and returns to level 4 at rate 4. It only draws.
import { computed } from 'vue'
import { egPath } from '../../audio/dx7'

const props = withDefaults(defineProps<{ rates: number[]; levels: number[]; width?: number; height?: number; label?: string }>(), {
  width: 150,
  height: 50,
})
const path = computed(() => egPath(props.rates, props.levels, props.width, props.height))
const area = computed(() => `${path.value}L${props.width - 3} ${props.height - 3}L3 ${props.height - 3}Z`)
</script>

<template>
  <svg
    class="eg" :width="width" :height="height" :viewBox="`0 0 ${width} ${height}`" role="img"
    :aria-label="`${label ?? 'Envelope'}: rates ${rates.join(', ')}, levels ${levels.join(', ')}`"
  >
    <rect x="0.5" y="0.5" :width="width - 1" :height="height - 1" rx="3" fill="var(--con-inset, #0d0f13)" stroke="#2b313b" />
    <line v-for="i in 3" :key="i" :x1="3" :x2="width - 3" :y1="(height / 4) * i" :y2="(height / 4) * i" stroke="#ffffff10" stroke-width="1" />
    <path :d="area" fill="var(--c, #4fd1a5)" opacity="0.14" />
    <path :d="path" fill="none" stroke="var(--c, #4fd1a5)" stroke-width="1.8" stroke-linejoin="round" stroke-linecap="round" />
  </svg>
</template>

<style scoped>
.eg { display: block; }
</style>
