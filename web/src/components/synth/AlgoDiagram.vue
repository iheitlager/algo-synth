<script setup lang="ts">
// A DX7 algorithm drawn as the DX7's manual does (spec 003 Req 9): carriers on the
// bottom row, each modulator above what it feeds, the feedback loop marked. It only
// draws what `audio/dx7.ts` works out from the routing table.
import { computed } from 'vue'
import { algoLayout } from '../../audio/dx7'

const props = defineProps<{ n: number }>()
const CELL_W = 36
const CELL_H = 32
const BOX_W = 24
const BOX_H = 18
const layout = computed(() => algoLayout(props.n))
const width = computed(() => layout.value.cols * CELL_W + 12)
const height = computed(() => layout.value.rows * CELL_H + 6)
/** How many operators share each row, to centre the row. */
const inRow = (rank: number) => layout.value.nodes.filter((x) => x.rank === rank).length
const pos = (op: number) => {
  const x = layout.value.nodes.find((k) => k.op === op)
  if (!x) return { cx: 0, cy: 0 }
  const offset = (layout.value.cols - inRow(x.rank)) / 2
  return { cx: 6 + (x.col + offset) * CELL_W + CELL_W / 2, cy: 3 + (layout.value.rows - 1 - x.rank) * CELL_H + CELL_H / 2 }
}
</script>

<template>
  <svg class="algo" :width="width" :height="height" :viewBox="`0 0 ${width} ${height}`" role="img" :aria-label="`Algorithm ${n + 1}`">
    <line
      v-for="(k, i) in layout.links" :key="i" :x1="pos(k.from).cx" :y1="pos(k.from).cy + BOX_H / 2" :x2="pos(k.to).cx"
      :y2="pos(k.to).cy - BOX_H / 2" stroke="var(--con-silk-dim, #888)" stroke-width="1.4"
    />
    <g v-for="x in layout.nodes" :key="x.op">
      <rect
        :x="pos(x.op).cx - BOX_W / 2" :y="pos(x.op).cy - BOX_H / 2" :width="BOX_W" :height="BOX_H" rx="3"
        :fill="x.carrier ? 'var(--c, #4fd1a5)' : '#0d0f13'" :stroke="x.carrier ? 'var(--c, #4fd1a5)' : '#4a5361'" stroke-width="1.2"
      />
      <text :x="pos(x.op).cx" :y="pos(x.op).cy + 4" text-anchor="middle" font-size="12" font-weight="700" :fill="x.carrier ? '#0d0f13' : 'var(--con-paper, #eee)'">{{ x.op }}</text>
      <path
        v-if="x.feedback" :d="`M${pos(x.op).cx + BOX_W / 2} ${pos(x.op).cy - 3}h6v-9h-${BOX_W / 2 + 6}v${BOX_H / 2 - 9 + 3}`" fill="none"
        stroke="var(--c, #4fd1a5)" stroke-width="1.2"
      />
    </g>
  </svg>
</template>
