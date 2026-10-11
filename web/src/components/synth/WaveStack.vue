<script setup lang="ts">
// A PPG wavetable on a little screen (spec 003 Req 9): its waves stacked back to
// front, the one at the oscillator's position lit. The waves are the engine's
// (`Tables::sketch`); this only draws them.
import { computed } from 'vue'
import { WAVE_POINTS, wavetables } from '../../audio/engine'
import { waveStack } from '../../audio/faceplate'

const props = defineProps<{ table: number; pos: number; label: string }>()
const W = 132
const H = 64
const stack = computed(() => waveStack(wavetables.points, WAVE_POINTS, props.table, props.pos, W, H))
</script>

<template>
  <svg class="stack" :width="W" :height="H" :viewBox="`0 0 ${W} ${H}`" role="img" :aria-label="`${label}: wave ${Math.round(pos * 63) + 1} of 64`">
    <path v-for="(d, i) in stack.ghosts" :key="i" class="ghost" :d="d" />
    <path class="under" :d="stack.lit" />
    <path class="lit" :d="stack.lit" />
  </svg>
</template>

<style scoped>
.stack { display: block; background: #0b0d10; border-radius: 3px; box-shadow: 0 0 0 1px var(--trim, #222) inset; }
path { fill: none; stroke: var(--c, #fff); stroke-linejoin: round; }
.ghost { stroke-width: 0.7; opacity: 0.16; }
/* A dark edge under the lit wave, so it reads in front of the stack. */
.under { stroke: #0b0d10; stroke-width: 4; }
.lit { stroke-width: 1.6; filter: drop-shadow(0 0 2px var(--c, #fff)); }
</style>
